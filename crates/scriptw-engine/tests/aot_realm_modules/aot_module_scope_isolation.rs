use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

struct Modules(PathBuf);

impl Drop for Modules {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_modules(files: &[(&str, &str)], expected: &[&str], prelude: Option<&str>) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let fixture = Modules(std::env::temp_dir().join(format!(
        "scriptw-module-scope-isolation-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&fixture.0).expect("create module fixture");
    for (name, source) in files {
        std::fs::write(fixture.0.join(name), source).expect("write module fixture");
    }
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_module(
            files[0].1,
            CompileOptions {
                filename: Some(fixture.0.join(files[0].0).to_str().unwrap().into()),
                module_root: Some(fixture.0.to_str().unwrap().into()),
                module_prelude: prelude.map(str::to_owned),
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("module graph compiles and executes through Wasm");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        expected
            .iter()
            .map(|line| HostOutputEvent::PrintLine((*line).into()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_dependency_free_name_cannot_capture_an_entry_module_lexical() {
    assert_modules(
        &[
            (
                "entry.js",
                "import { dependencyRead } from './dependency.js'; const entryOnly = 'entry'; print(dependencyRead());",
            ),
            (
                "dependency.js",
                "export function dependencyRead() { return typeof entryOnly; }",
            ),
        ],
        &["undefined"],
        None,
    );
}

#[test]
fn imported_aliases_do_not_create_global_own_properties() {
    assert_modules(
        &[
            (
                "entry.js",
                "import { value as localAlias } from './dependency.js'; print(Object.getOwnPropertyDescriptor(globalThis, 'localAlias') === undefined); print(localAlias);",
            ),
            ("dependency.js", "export const value = 3;"),
        ],
        &["true", "3"],
        None,
    );
}

#[test]
fn dependency_free_names_resolve_the_independent_script_prelude() {
    assert_modules(
        &[
            (
                "entry.js",
                "import { dependencyRead } from './dependency.js'; const shared = 'entry'; print(dependencyRead(), helperRead(), shared);",
            ),
            (
                "dependency.js",
                "export function dependencyRead() { return shared; }",
            ),
        ],
        &["global global entry"],
        Some("const shared = 'global'; function helperRead() { return shared; }"),
    );
}

#[test]
fn live_immutable_import_aliases_leave_global_script_bindings_untouched() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
import { value as alias, increment } from './dependency.js';
print(alias, helperRead(), globalThis.alias);
increment();
let immutable = false;
try { alias = 9; } catch (error) { immutable = error instanceof TypeError; }
print(alias, helperRead(), immutable);
const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'alias');
print(descriptor.value, descriptor.get === undefined);
"#,
            ),
            (
                "dependency.js",
                "export let value = 3; export function increment() { value++; }",
            ),
        ],
        &["3 global global", "4 global true", "global true"],
        Some("var alias = 'global'; function helperRead() { return alias; }"),
    );
}

#[test]
fn same_spelled_declarations_and_aliases_stay_in_their_own_modules() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
import { read as first } from './left.js';
import { read as second } from './right.js';
const shared = 'entry';
var localVar = 'entry';
function localFunction() { return localVar; }
print(first(), second(), shared, localFunction());
print(Object.getOwnPropertyDescriptor(globalThis, 'localVar') === undefined);
"#,
            ),
            (
                "left.js",
                "import { left as alias } from './values.js'; const shared = 'left'; var localVar = 1; function localFunction() { return localVar; } export function read() { return shared + alias + localFunction(); }",
            ),
            (
                "right.js",
                "import { right as alias } from './values.js'; const shared = 'right'; var localVar = 2; function localFunction() { return localVar; } export function read() { return shared + alias + localFunction(); }",
            ),
            (
                "values.js",
                "export const left = 'L'; export const right = 'R';",
            ),
        ],
        &["leftL1 rightR2 entry entry", "true"],
        None,
    );
}

#[test]
fn evaluation_cycles_follow_entry_request_order_across_external_dependencies() {
    assert_modules(
        &[
            (
                "entry.js",
                "import './left.js'; import './external.js'; import './right.js'; print('entry');",
            ),
            ("left.js", "import './entry.js'; print('left');"),
            ("external.js", "print('external');"),
            ("right.js", "import './entry.js'; print('right');"),
        ],
        &["left", "external", "right", "entry"],
        None,
    );
}

#[test]
fn cycle_instantiation_hoists_functions_and_keeps_import_tdz_and_immutability() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
import { initial, tdz, immutable, read } from './dependency.js';
export let value = 7;
export function hoisted() { return 5; }
print(initial, tdz, immutable, read());
value = 9;
print(read());
"#,
            ),
            (
                "dependency.js",
                r#"
import { hoisted, value } from './entry.js';
export const initial = hoisted();
export let tdz = false;
try { value; } catch (error) { tdz = error instanceof ReferenceError; }
export let immutable = false;
try { value = 4; } catch (error) { immutable = error instanceof TypeError; }
export function read() { return value; }
"#,
            ),
        ],
        &["5 true true 7", "9"],
        None,
    );
}

#[test]
fn a_completed_cycle_member_remains_evaluating_until_the_component_finishes() {
    assert_modules(
        &[
            (
                "entry.js",
                "import { blocked } from './left.js'; import defer * as right from './right.js'; print(blocked, right.value);",
            ),
            (
                "left.js",
                r#"
import './right.js';
import defer * as right from './right.js';
export let blocked = false;
try { right.value; } catch (error) { blocked = error instanceof TypeError; }
"#,
            ),
            ("right.js", "import './left.js'; export const value = 3;"),
        ],
        &["true 3"],
        None,
    );
}

#[test]
fn a_late_cycle_throw_is_cached_on_members_whose_bodies_already_completed() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
import defer * as left from './left.js';
import defer * as right from './right.js';
globalThis.calls = [];
globalThis.moduleError = {};
for (const read of [() => left.value, () => right.value, () => left.value]) {
  let received;
  try { read(); } catch (error) { received = error; }
  if (received !== globalThis.moduleError) throw 'cycle error identity';
}
print(globalThis.calls.join(','));
"#,
            ),
            (
                "left.js",
                "import './right.js'; globalThis.calls.push('left'); throw globalThis.moduleError; export const value = 1;",
            ),
            (
                "right.js",
                "import './left.js'; globalThis.calls.push('right'); export const value = 2;",
            ),
        ],
        &["right,left"],
        None,
    );
}

#[test]
fn an_unvisited_cycle_member_still_visits_its_dependencies_after_an_earlier_failure() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
import defer * as left from './left.js';
import defer * as unvisited from './unvisited.js';
globalThis.calls = [];
globalThis.moduleError = {};
let received;
try { left.value; } catch (error) { received = error; }
if (received !== globalThis.moduleError || globalThis.calls.join(',') !== 'failing') throw 'first failure';
received = undefined;
try { unvisited.value; } catch (error) { received = error; }
if (received !== globalThis.moduleError) throw 'unvisited error identity';
print(globalThis.calls.join(','));
"#,
            ),
            (
                "left.js",
                "import './failing.js'; import './unvisited.js'; globalThis.calls.push('left'); export const value = 1;",
            ),
            (
                "failing.js",
                "import './left.js'; globalThis.calls.push('failing'); throw globalThis.moduleError;",
            ),
            (
                "unvisited.js",
                "import './side.js'; import './left.js'; globalThis.calls.push('unvisited'); export const value = 2;",
            ),
            ("side.js", "globalThis.calls.push('side');"),
        ],
        &["failing,side"],
        None,
    );
}

#[test]
fn self_cycle_imports_share_one_live_namespace_and_execute_once() {
    assert_modules(
        &[(
            "entry.js",
            r#"
import * as first from './entry.js';
import * as second from './entry.js';
export let value = 3;
value++;
print(first === second, first.value, this === undefined);
"#,
        )],
        &["true 4 true"],
        None,
    );
}
