use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedRunOutcome, PromiseRejectionPolicy, RealmBuilder,
    RunOptions,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Modules(PathBuf);
impl Drop for Modules {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn observe(
    files: &[(&str, &str)],
    prelude: Option<&str>,
    policy: PromiseRejectionPolicy,
) -> ObservedRunOutcome {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let fixture = Modules(std::env::temp_dir().join(format!(
        "scriptw-async-module-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&fixture.0).unwrap();
    for (name, source) in files {
        std::fs::write(fixture.0.join(name), source).unwrap();
    }
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_module(
            files[0].1,
            CompileOptions {
                filename: Some(fixture.0.join(files[0].0).to_str().unwrap().into()),
                module_root: Some(fixture.0.to_str().unwrap().into()),
                module_prelude: prelude.map(str::to_owned),
                host_surface_policy: HostSurfacePolicy::Test262,
                promise_rejection_policy: policy,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("compiled Module graph executes");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    observed
}

fn success(files: &[(&str, &str)], expected: &[&str], prelude: Option<&str>) {
    let observed = observe(files, prelude, PromiseRejectionPolicy::FailRun);
    assert_eq!(
        observed.completion,
        ObservedCompletion::Normal(ObservedJsValue::Undefined)
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
fn async_owners_keep_live_import_cells_separate_from_suspended_lexical_chains() {
    success(
        &[
            (
                "entry.js",
                r#"import { value, replace, read } from './dependency.js';
const shared = 'entry';
if (typeof arguments !== 'undefined' || this !== undefined) throw 'module lexical owner';
{ let value = 'shadow'; await 0; if (value !== 'shadow') throw 'resumed lexical scope'; }
if (value !== 3 || read() !== 'dependency') throw 'canonical import environment';
replace(8); await 0;
if (value !== 8 || helperRead() !== 'global' || shared !== 'entry') throw 'live or global capture';
print('canonical async environments');"#,
            ),
            (
                "dependency.js",
                "const shared = 'dependency'; export let value = 3; export function replace(next) { value = next; } export function read() { return shared; } await 0;",
            ),
        ],
        &["canonical async environments"],
        Some("const shared = 'global'; function helperRead() { return shared; }"),
    );
}

#[test]
fn deferred_requests_promote_async_leaves_without_evaluating_synchronous_wrappers() {
    success(
        &[
            (
                "entry.js",
                r#"import './setup.js'; import defer * as left from './left.js'; import defer * as right from './right.js';
if (events.join(',') !== 'A start,B start,A end,B end') throw events.join(',');
events.push('entry');
if (left.value !== 1 || right.value !== 2) throw 'exports';
if (events.join(',') !== 'A start,B start,A end,B end,entry,left,right') throw events.join(',');
print('flattened deferred dependencies');"#,
            ),
            ("setup.js", "globalThis.events = [];"),
            (
                "left.js",
                "import { value } from './a.js'; events.push('left'); export { value };",
            ),
            (
                "right.js",
                "import { value } from './b.js'; events.push('right'); export { value };",
            ),
            (
                "a.js",
                "events.push('A start'); await 0; events.push('A end'); export const value = 1;",
            ),
            (
                "b.js",
                "events.push('B start'); await 0; events.push('B end'); export const value = 2;",
            ),
        ],
        &["flattened deferred dependencies"],
        None,
    );
}

#[test]
fn executing_async_self_and_transitive_deferred_reads_do_not_poison_evaluation() {
    success(
        &[
            (
                "entry.js",
                r#"import defer * as self from './entry.js'; import defer * as dependent from './dependent.js';
globalThis.dependentCalls = 0;
function check() { for (const read of [() => self.missing, () => dependent.value]) { let caught = false; try { read(); } catch (error) { caught = error instanceof TypeError; } if (!caught) throw 'active namespace accepted'; } }
check(); await 0; check();
if (dependentCalls !== 0) throw 'dependent body entered';
export const value = 9;
import('./entry.js').then(ns => { if (ns.value !== 9 || dependent.value !== 9 || dependentCalls !== 1) throw 'post-completion access'; print('readiness is retryable'); });"#,
            ),
            (
                "dependent.js",
                "import { value } from './entry.js'; globalThis.dependentCalls++; export { value };",
            ),
        ],
        &["readiness is retryable"],
        None,
    );
}

#[test]
fn duplicate_dependencies_on_one_runtime_cycle_root_release_the_parent_once() {
    success(
        &[
            (
                "entry.js",
                "import './setup.js'; import { a } from './a.js'; import { b } from './b.js'; events.push('entry'); if (a + b !== 3 || events.join(',') !== 'B start,B end,A start,A end,entry') throw events.join(','); print('cycle occurrence counts');",
            ),
            ("setup.js", "globalThis.events = [];"),
            (
                "a.js",
                "import './b.js'; events.push('A start'); await 0; events.push('A end'); export const a = 1;",
            ),
            (
                "b.js",
                "import './a.js'; events.push('B start'); await 0; events.push('B end'); export const b = 2;",
            ),
        ],
        &["cycle occurrence counts"],
        None,
    );
}

#[test]
fn internal_evaluation_waits_do_not_observe_mutable_promise_hooks() {
    success(
        &[
            (
                "entry.js",
                "import { value } from './dependency.js'; if (value !== 7) throw 'export'; print('intrinsic evaluation waits');",
            ),
            (
                "dependency.js",
                r#"Object.defineProperty(Promise.prototype, 'constructor', { get() { throw 'constructor hook'; }, configurable: true });
Object.defineProperty(Promise, Symbol.species, { get() { throw 'species hook'; }, configurable: true });
Promise.prototype.then = function() { throw 'then hook'; };
await 0; export const value = 7;"#,
            ),
        ],
        &["intrinsic evaluation waits"],
        None,
    );
}

#[test]
fn ordinary_source_await_still_performs_promise_resolve() {
    success(
        &[(
            "entry.js",
            r#"const sentinel = {}; const promise = Promise.resolve(0);
Object.defineProperty(promise, 'constructor', { get() { throw sentinel; } });
let caught = false; try { await promise; } catch (error) { caught = error === sentinel; }
if (!caught) throw 'source Await bypassed constructor'; print('ordinary await protocol');"#,
        )],
        &["ordinary await protocol"],
        None,
    );
}

#[test]
fn dynamic_deferred_join_starts_all_leaves_and_preserves_namespace_identity() {
    success(
        &[
            (
                "entry.js",
                r#"globalThis.events = []; globalThis.wrapperCalls = 0;
Promise.all = function() { throw 'public Promise.all'; };
const first = await import.defer('./wrapper.js');
if (wrapperCalls !== 0 || events.join(',') !== 'A start,B start,A end,B end') throw events.join(',');
const second = await import.defer('./wrapper.js');
if (first !== second || wrapperCalls !== 0) throw 'namespace identity';
if (first.value !== 3 || wrapperCalls !== 1 || second.value !== 3) throw 'deferred body';
print('parallel deferred join');"#,
            ),
            (
                "wrapper.js",
                "import { a } from './a.js'; import { b } from './b.js'; globalThis.wrapperCalls++; export const value = a + b;",
            ),
            (
                "a.js",
                "events.push('A start'); await 0; events.push('A end'); export const a = 1;",
            ),
            (
                "b.js",
                "events.push('B start'); await 0; events.push('B end'); export const b = 2;",
            ),
        ],
        &["parallel deferred join"],
        None,
    );
}

#[test]
fn concurrent_imports_keep_fresh_promises_and_the_exact_undefined_rejection() {
    success(
        &[
            (
                "entry.js",
                r#"const first = import('./bad.js'); const second = import('./bad.js'); if (first === second) throw 'reused import promise';
let count = 0;
try { await first; } catch (error) { if (error !== undefined) throw 'replaced rejection'; count++; }
try { await second; } catch (error) { if (error !== undefined) throw 'replaced rejection'; count++; }
if (count !== 2) throw 'missing rejection'; print('undefined import rejection');"#,
            ),
            ("bad.js", "await 0; throw undefined;"),
        ],
        &["undefined import rejection"],
        None,
    );
}

#[test]
fn a_late_async_sibling_cannot_replace_or_reenter_an_aborted_entry() {
    for policy in [
        PromiseRejectionPolicy::Ignore,
        PromiseRejectionPolicy::FailRun,
    ] {
        let observed = observe(
            &[
                (
                    "entry.js",
                    "import './async.js'; import './failure.js'; throw 'entry body must not execute';",
                ),
                ("async.js", "await 0; print('late sibling completed');"),
                ("failure.js", "throw undefined;"),
            ],
            None,
            policy,
        );
        assert_eq!(
            observed.completion,
            ObservedCompletion::Throw(ObservedJsValue::Undefined)
        );
        assert_eq!(
            observed.output_events,
            vec![HostOutputEvent::PrintLine("late sibling completed".into())]
        );
    }
}

#[test]
fn malformed_dynamic_only_targets_reject_their_import_after_a_tla_entry_resumes() {
    success(
        &[
            (
                "entry.js",
                "await 0; let caught = false; try { await import('./invalid.js'); } catch (error) { caught = error instanceof SyntaxError; } if (!caught) throw 'missing syntax rejection'; print('isolated dynamic syntax rejection');",
            ),
            ("invalid.js", "invalid syntax!"),
        ],
        &["isolated dynamic syntax rejection"],
        None,
    );
}

#[test]
fn async_module_resource_lifetimes_begin_after_instantiation_and_dispose_after_await() {
    success(
        &[(
            "entry.js",
            r#"const events = [];
{ using resource = { [Symbol.dispose]() { events.push('dispose'); } }; events.push('before'); await 0; events.push('after'); }
if (events.join(',') !== 'before,after,dispose') throw events.join(',');
print('async module resource lifetime');"#,
        )],
        &["async module resource lifetime"],
        None,
    );
}

#[test]
fn implicit_module_await_waits_for_statement_block_and_loop_disposal() {
    success(
        &[
            (
                "entry.js",
                r#"import { events as statement } from './statement.js';
import { events as block } from './block.js';
import { events as classic } from './classic.js';
import { events as iteration } from './iteration.js';
if (statement.join(',') !== 'body,dispose,disposed') throw statement.join(',');
if (block.join(',') !== 'body,dispose,disposed,after') throw block.join(',');
if (classic.join(',') !== 'before,dispose,disposed,after') throw classic.join(',');
if (iteration.join(',') !== 'body,dispose,disposed,after') throw iteration.join(',');
print('implicit module disposal completes before importers');"#,
            ),
            (
                "statement.js",
                r#"export const events = [];
await using resource = { [Symbol.asyncDispose]() { events.push('dispose'); return Promise.resolve().then(() => events.push('disposed')); } };
events.push('body');"#,
            ),
            (
                "block.js",
                r#"export const events = [];
{ await using resource = { [Symbol.asyncDispose]() { events.push('dispose'); return Promise.resolve().then(() => events.push('disposed')); } }; events.push('body'); }
events.push('after');"#,
            ),
            (
                "classic.js",
                r#"export const events = ['before'];
for (await using resource = { [Symbol.asyncDispose]() { events.push('dispose'); return Promise.resolve().then(() => events.push('disposed')); } }; false;) { throw 'unreachable'; }
events.push('after');"#,
            ),
            (
                "iteration.js",
                r#"export const events = [];
for (await using resource of [{ [Symbol.asyncDispose]() { events.push('dispose'); return Promise.resolve().then(() => events.push('disposed')); } }]) { events.push('body'); }
events.push('after');"#,
            ),
        ],
        &["implicit module disposal completes before importers"],
        None,
    );
}

#[test]
fn nullish_module_disposal_still_has_an_asynchronous_completion() {
    success(
        &[
            (
                "entry.js",
                "import { events } from './dependency.js'; if (events.join(',') !== 'body,queued') throw events.join(','); print('nullish disposal awaited');",
            ),
            (
                "dependency.js",
                r#"export const events = [];
await using resource = null;
events.push('body');
Promise.resolve().then(() => events.push('queued'));"#,
            ),
        ],
        &["nullish disposal awaited"],
        None,
    );
}

#[test]
fn implicit_disposal_rejection_prevents_importer_entry_and_preserves_undefined() {
    for policy in [
        PromiseRejectionPolicy::Ignore,
        PromiseRejectionPolicy::FailRun,
    ] {
        let observed = observe(
            &[
                (
                    "entry.js",
                    "import './dependency.js'; throw 'importer must not run';",
                ),
                (
                    "dependency.js",
                    r#"await using resource = { [Symbol.asyncDispose]() { print('dispose'); return Promise.reject(undefined); } };
print('body');"#,
                ),
            ],
            None,
            policy,
        );
        assert_eq!(
            observed.completion,
            ObservedCompletion::Throw(ObservedJsValue::Undefined)
        );
        assert_eq!(
            observed.output_events,
            [
                HostOutputEvent::PrintLine("body".into()),
                HostOutputEvent::PrintLine("dispose".into()),
            ]
        );
    }
}

#[test]
fn module_capture_import_cells_preserve_tdz_write_order_and_hoisted_calls() {
    success(
        &[
            (
                "entry.js",
                r#"import './setup.js';
import { value, bump, hoisted } from './owner.js';
import { read, held, next } from './reader.js';
import { renamed } from './relay.js';
function readRelay() { return renamed; }
print(trace.join(','));
const before = read();
const first = held();
const second = next();
bump(); gc();
print([before, read(), readRelay(), hoisted(), first, second].join(','));"#,
            ),
            ("setup.js", "globalThis.trace = [];"),
            (
                "owner.js",
                r#"import './reader.js';
export var ready = 9;
export let value = 5;
export let callable = function old(value) { return 'old:' + value + ':' + (this === undefined); };
export function hoisted() { return ready; }
export function bump() { ++value; }
export function replace() { callable = function next(value) { return 'new:' + value + ':' + (this === undefined); }; }"#,
            ),
            (
                "reader.js",
                r#"import { value as alias, callable as callAlias, replace, hoisted } from './owner.js';
export function read() { return alias; }
function kind() { return typeof alias; }
function plain() { alias = (trace.push('plain-rhs'), 99); }
function compound() { alias += (trace.push('compound-rhs'), 1); }
export function held() { return callAlias((replace(), 3)); }
export function next() { return callAlias(4); }
trace.push('hoisted:' + typeof hoisted());
try { read(); } catch (error) { trace.push('read:' + (error instanceof ReferenceError)); }
try { kind(); } catch (error) { trace.push('type:' + (error instanceof ReferenceError)); }
try { plain(); } catch (error) { trace.push('plain:' + (error instanceof TypeError)); }
try { compound(); } catch (error) { trace.push('compound:' + (error instanceof ReferenceError)); }"#,
            ),
            ("relay.js", "export { value as renamed } from './owner.js';"),
        ],
        &[
            "hoisted:undefined,read:true,type:true,plain-rhs,plain:true,compound:true",
            "5,6,6,9,old:3:true,new:4:true",
        ],
        None,
    );
}

#[test]
fn module_capture_async_root_cells_remain_live_after_await_and_evaluation() {
    success(
        &[
            (
                "entry.js",
                r#"import { before, after, read, write, nested } from './owner.js';
import { alias } from './relay.js';
function readAlias() { return alias; }
if (before !== 2 || after !== 5 || read() !== 5 || nested() !== 6) throw 'suspended root';
write(8); gc();
if (read() !== 8 || readAlias() !== 8 || nested() !== 9) throw 'live completed root';
await import('./owner.js').then(function(namespace) {
  if (namespace.read !== read || namespace.read() !== 8) throw 'retained namespace';
  namespace.write(11); gc();
  if (read() !== 11 || readAlias() !== 11 || nested() !== 12) throw 'late root identity';
});
print('live async root cells');"#,
            ),
            (
                "owner.js",
                r#"export let value = 2;
export function read() { return value; }
export function write(next) { value = next; }
function make(padding) { return function inner() { return value + padding; }; }
export const nested = make(1);
export const before = read();
await 0;
write(5); gc();
export const after = read();"#,
            ),
            ("relay.js", "export { value as alias } from './owner.js';"),
        ],
        &["live async root cells"],
        None,
    );
}
