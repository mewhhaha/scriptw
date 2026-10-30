use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use scriptw_engine::{
    CompileOptions, EmbeddedModuleEntryInput, EmbeddedModuleGoal, EmbeddedModuleGraph,
    EmbeddedModuleReferrer, EmbeddedModuleResolutionInput, EmbeddedModuleSourceInput, Engine,
    ExecutionBackend, HostOutputEvent, HostSurfacePolicy, ModuleLoadingPolicy, ObservedCompletion,
    ObservedJsValue, PromiseRejectionPolicy, RealmBuilder, RunOptions,
};

struct Modules(PathBuf);

impl Drop for Modules {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn checked(source: &str) -> String {
    format!("function assert(value, label) {{ if (!value) throw new Error(label); }}\n{source}")
}

fn assert_run(
    goal: EmbeddedModuleGoal,
    source: &str,
    mut options: CompileOptions,
    expected: &[&str],
) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    options.host_surface_policy = HostSurfacePolicy::Test262;
    options.promise_rejection_policy = PromiseRejectionPolicy::FailRun;
    let run = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    };
    let engine = Engine::new(RealmBuilder::new().build());
    let observed = match goal {
        EmbeddedModuleGoal::Script => engine.observe_script(source, options, run),
        EmbeddedModuleGoal::Module => engine.observe_module(source, options, run),
    }
    .expect("ShadowRealm imports compile and execute through Wasm AOT");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        observed.completion,
        ObservedCompletion::Normal(match goal {
            EmbeddedModuleGoal::Script => ObservedJsValue::Boolean(true),
            EmbeddedModuleGoal::Module => ObservedJsValue::Undefined,
        }),
        "{observed:?}\n{source}"
    );
    assert_eq!(
        observed.output_events,
        expected
            .iter()
            .map(|line| HostOutputEvent::PrintLine((*line).into()))
            .collect::<Vec<_>>(),
        "{source}"
    );
}

fn assert_files(
    goal: EmbeddedModuleGoal,
    source: &str,
    modules: &[(&str, &str)],
    expected: &[&str],
) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let fixture = Modules(std::env::temp_dir().join(format!(
        "scriptw-shadow-realm-import-value-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&fixture.0).expect("create module fixture");
    let source = checked(source);
    std::fs::write(fixture.0.join("entry.js"), &source).expect("write entry fixture");
    for (name, source) in modules {
        let path = fixture.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create nested module directory");
        std::fs::write(path, source).expect("write module fixture");
    }
    assert_run(
        goal,
        &source,
        CompileOptions {
            filename: Some(fixture.0.join("entry.js").to_str().unwrap().into()),
            module_root: Some(fixture.0.to_str().unwrap().into()),
            ..CompileOptions::default()
        },
        expected,
    );
}

#[test]
fn nested_evaluate_and_loaded_function_sources_share_the_realm_request_catalog() {
    assert_files(
        EmbeddedModuleGoal::Script,
        r#"
const realm = new ShadowRealm();
const start = realm.evaluate(`(function(report) {
    new ShadowRealm().evaluate('(function(report) { void new ShadowRealm().importValue("./prepared.js", "start").then(start => start(report)); })')(report);
})`);
start(value => { assert(value === 42, 'transitive prepared export'); print('nested prepared: ' + value); });
true;
"#,
        &[
            (
                "prepared.js",
                r#"export function start(report) {
                    Function('report', `
                        const realm = new ShadowRealm();
                        const load = realm.importValue;
                        const specifier = './' + 'leaf.js';
                        const values = [
                            load?.call(realm, specifier, 'value'),
                            load.call?.(realm, './call-method.js', 'value'),
                            realm?.importValue.apply(realm, ['./apply-target.js', 'value']),
                            realm.importValue.apply?.(realm, ['./apply-method.js', 'value']),
                            load?.apply(realm, ['./apply-target.js', 'value']),
                            load.apply?.(realm, ['./apply-method.js', 'value']),
                            realm?.importValue.call(realm, specifier, 'value'),
                            realm.importValue.call?.(realm, './call-method.js', 'value')
                        ];
                        void Promise.all(values).then(values => {
                            if (values.join(',') !== '9,10,11,12,11,12,9,10')
                                throw 'optional forwarded exports';
                            report(values[0] + values[1] + values[2] + values[3]);
                        });
                    `)(report);
                }"#,
            ),
            ("leaf.js", "export const value = 9;"),
            ("call-method.js", "export const value = 10;"),
            ("apply-target.js", "export const value = 11;"),
            ("apply-method.js", "export const value = 12;"),
        ],
        &["nested prepared: 42"],
    );
}

#[test]
fn reject_all_catalog_preserves_custom_calls_brand_checks_and_async_host_rejection() {
    assert_run(
        EmbeddedModuleGoal::Script,
        &checked(
            r#"
let calls = 0;
const custom = { importValue(specifier, name) { calls++; return specifier + ':' + name; } };
assert(custom.importValue('./absent.js', 'value') === './absent.js:value' && calls === 1, 'custom callee is preserved');
assert(custom?.importValue.call(custom, './optional-custom.js', 'value') === './optional-custom.js:value' && calls === 2, 'optional custom call is preserved');
assert(custom.importValue.apply?.(custom, ['./optional-custom-apply.js', 'value']) === './optional-custom-apply.js:value' && calls === 3, 'optional custom apply is preserved');
let skipped = 0;
const absent = null;
absent?.importValue.call(absent, (++skipped, './optional-skipped.js'), 'value');
const noCall = { importValue: { call: undefined } };
noCall.importValue.call?.(noCall, (++skipped, './missing-call.js'), 'value');
assert(skipped === 0, 'optional source candidates do not execute skipped arguments');
const native = ShadowRealm.prototype.importValue;
let error;
try { native.call({}, './absent.js', 'value'); } catch (caught) { error = caught; }
assert(error instanceof TypeError, 'invalid receiver throws synchronously');
error = undefined;
try { native?.call({}, './optional-absent.js', 'value'); } catch (caught) { error = caught; }
assert(error instanceof TypeError, 'optional native call keeps receiver validation');
let coerced = 0;
try { native.call({}, { toString() { coerced++; return './absent.js'; } }, 'value'); } catch (caught) { error = caught; }
assert(error instanceof TypeError && coerced === 0, 'brand precedes coercion');
let synchronous = true;
const realm = new ShadowRealm();
let rejections = 0;
function reject(reason) {
    assert(reason instanceof TypeError && !synchronous, 'host rejection remains asynchronous');
    if (++rejections === 2) print('disabled import rejected');
}
realm.importValue('./absent.js', 'value').then(
    () => { throw new Error('disabled loader fulfilled'); },
    reject
);
native?.apply(realm, ['./optional-absent.js', 'value']).then(
    () => { throw new Error('disabled optional loader fulfilled'); },
    reject
);
synchronous = false;
true;
"#,
        ),
        CompileOptions {
            module_loading_policy: ModuleLoadingPolicy::RejectAll,
            ..CompileOptions::default()
        },
        &["disabled import rejected"],
    );
}

#[test]
fn receiver_and_specifier_fail_synchronously_before_export_name_validation() {
    assert_files(
        EmbeddedModuleGoal::Script,
        r#"
const realm = new ShadowRealm();
const method = ShadowRealm.prototype.importValue;
let trace = '';
const marker = {};
const throwing = { toString() { trace += 'specifier;'; throw marker; } };
let error;
try { method.call({}, throwing, 'value'); } catch (caught) { error = caught; }
assert(error instanceof TypeError && trace === '', 'receiver before specifier ToString');
error = undefined;
try { method.call(realm, throwing, 7); } catch (caught) { error = caught; }
assert(error === marker && trace === 'specifier;', 'original synchronous coercion error before export type');
const specifier = { toString() { trace += 'string;'; return './value.js'; } };
const exportName = { toString() { trace += 'export;'; return 'value'; } };
error = undefined;
try { method.call(realm, specifier, exportName); } catch (caught) { error = caught; }
assert(error instanceof TypeError && trace === 'specifier;string;', 'export name is not coerced');
const pending = realm.importValue('./value.js', 'value');
assert(Object.getPrototypeOf(pending) === Promise.prototype, 'valid call returns a Promise');
pending.then(value => { assert(value === 17, 'export value'); print('ordered importValue'); });
true;
"#,
        &[("value.js", "export const value = 17;")],
        &["ordered importValue"],
    );
}

#[test]
fn modules_are_cached_within_one_shadow_realm_and_isolated_from_other_realms() {
    assert_files(
        EmbeddedModuleGoal::Script,
        r#"
globalThis.executions = 0;
const first = new ShadowRealm(), second = new ShadowRealm();
const left = first.importValue('./state.js', 'next');
const right = first.importValue('./state.js', 'next');
assert(left !== right, 'fresh import promises');
Promise.all([import('./state.js'), left, right, second.importValue('./state.js', 'next')]).then(values => {
    const root = values[0], a = values[1], b = values[2], c = values[3];
    assert(a !== b && Object.getPrototypeOf(a) === Function.prototype, 'fresh caller-Realm callable exports');
    assert(a() === 1 && b() === 2 && c() === 1 && root.next() === 1, 'per-Realm live module cells');
    assert(executions === 1 && first.evaluate('executions') === 1 && second.evaluate('executions') === 1, 'one evaluation per Realm');
    return first.importValue('./state.js', 'next').then(again => {
        assert(again !== a && again !== b && again() === 3, 'cached module produces a fresh wrapper');
        assert(first.evaluate('executions') === 1, 'repeat import does not rerun module');
        print('isolated importValue caches');
    });
});
true;
"#,
        &[(
            "state.js",
            "globalThis.executions = (globalThis.executions || 0) + 1; let count = 0; export function next() { return ++count; }",
        )],
        &["isolated importValue caches"],
    );
}

#[test]
fn borrowed_import_uses_intrinsic_promises_and_errors_from_its_defining_realm() {
    assert_files(
        EmbeddedModuleGoal::Script,
        r#"
const foreign = __scriptwCreateRealm().global;
const realm = new ShadowRealm();
const method = foreign.ShadowRealm.prototype.importValue;
const IntrinsicPromise = foreign.Promise;
const then = IntrinsicPromise.prototype.then;
let redirected = 0;
foreign.Promise = function() { redirected++; throw 'global Promise'; };
IntrinsicPromise.resolve = function() { redirected++; throw 'Promise.resolve'; };
IntrinsicPromise.reject = function() { redirected++; throw 'Promise.reject'; };
IntrinsicPromise.prototype.then = function() { redirected++; throw 'Promise.prototype.then'; };
realm.importValue = method;
const first = realm.importValue('./exports.js', 'answer');
const second = method.call(realm, './exports.js', 'answer');
const callable = method.call(realm, './exports.js', 'callback');
const object = method.call(realm, './exports.js', 'object');
const missing = method.call(realm, './exports.js', 'missing');
assert(first !== second && Object.getPrototypeOf(first) === IntrinsicPromise.prototype, 'fresh intrinsic caller-Realm Promise');
let completed = 0;
function done() {
    if (++completed === 5) {
        assert(redirected === 0, 'native capability and reactions ignore mutated Promise properties');
        print('intrinsic importValue promises');
    }
}
function rejected(error) {
    assert(Object.getPrototypeOf(error) === foreign.TypeError.prototype, 'export rejection belongs to borrowed method Realm');
    done();
}
then.call(first, value => { assert(value === 29, 'first primitive export'); done(); });
then.call(second, value => { assert(value === 29, 'second primitive export'); done(); });
then.call(callable, value => {
    assert(Object.getPrototypeOf(value) === foreign.Function.prototype && value(3) === 4, 'callable export is wrapped into method Realm');
    done();
});
then.call(object, () => { throw 'ordinary object export crossed'; }, rejected);
then.call(missing, () => { throw 'missing export fulfilled'; }, rejected);
true;
"#,
        &[(
            "exports.js",
            "export const answer = 29; export const object = {}; export function callback(value) { return value + 1; }",
        )],
        &["intrinsic importValue promises"],
    );
}

#[test]
fn load_parse_and_cached_evaluation_failures_become_fresh_type_errors_without_getters() {
    assert_files(
        EmbeddedModuleGoal::Script,
        r#"
const realm = new ShadowRealm();
realm.evaluate(`
globalThis.throwRuns = 0;
globalThis.errorReads = 0;
globalThis.intrinsicReads = 0;
Object.defineProperty(globalThis, 'SyntaxError', {
    configurable: true,
    get() { intrinsicReads++; throw 'global SyntaxError getter'; }
});
globalThis.TypeError = function() { intrinsicReads++; throw 'global TypeError constructor'; };
`);
let errors = [];
function rejected(error) {
    assert(Object.getPrototypeOf(error) === TypeError.prototype, 'caller TypeError');
    assert(errors.indexOf(error) === -1, 'fresh copy for each rejected import');
    errors.push(error);
}
function fulfilled() { throw 'failed module import fulfilled'; }
const unknown = './' + 'uncatalogued.js';
Promise.all([
    realm.importValue(unknown, 'value').then(fulfilled, rejected),
    realm.importValue('./absent.js', 'value').then(fulfilled, rejected),
    realm.importValue('./syntax.js', 'value').then(fulfilled, rejected),
    realm.importValue('./throws.js', 'value').then(fulfilled, rejected),
    realm.importValue('./throws.js', 'value').then(fulfilled, rejected)
]).then(() => {
    assert(errors.length === 5, 'all unknown, load, parse and evaluation failures settled');
    assert(realm.evaluate('throwRuns') === 1, 'cached original module failure');
    assert(realm.evaluate('errorReads') === 0, 'rejection copying never reads thrown object properties');
    assert(realm.evaluate('intrinsicReads') === 0, 'module dispatcher errors use Realm intrinsics');
    print('importValue failure copies');
});
true;
"#,
        &[
            ("syntax.js", "export const ="),
            (
                "throws.js",
                "globalThis.throwRuns++; throw new Proxy({}, { get() { globalThis.errorReads++; throw 'forbidden Get'; }, getPrototypeOf() { globalThis.errorReads++; throw 'forbidden prototype'; } }); export const value = 1;",
            ),
        ],
        &["importValue failure copies"],
    );
}

#[test]
fn import_value_never_assimilates_the_module_namespaces_then_export() {
    assert_files(
        EmbeddedModuleGoal::Script,
        r#"
const realm = new ShadowRealm();
realm.evaluate('globalThis.thenCalls = 0;');
const first = realm.importValue('./then.js', 'answer');
const second = realm.importValue('./then.js', 'answer');
Promise.all([first, second]).then(values => {
    assert(values[0] === 43 && values[1] === 43, 'selected export survives initial and cached imports');
    assert(realm.evaluate('thenCalls') === 0, 'namespace never enters Promise resolution');
    print('unobserved namespace then');
});
true;
"#,
        &[(
            "then.js",
            "export const answer = 43; export function then(resolve) { globalThis.thenCalls++; resolve('unexpected assimilation'); }",
        )],
        &["unobserved namespace then"],
    );
}

#[test]
fn realm_request_discovery_preserves_custom_import_value_calls_with_invalid_modules() {
    assert_files(
        EmbeddedModuleGoal::Script,
        r#"
let calls = 0;
const custom = { importValue(specifier) { calls++; return 'actual:' + specifier; } };
assert(custom.importValue('./syntax.js', 'x') === 'actual:./syntax.js' && calls === 1, 'actual custom callee executes despite rejected catalog entry');
print('custom importValue preserved');
true;
"#,
        &[("syntax.js", "export const =")],
        &["custom importValue preserved"],
    );
}

#[test]
fn a_module_caller_keeps_the_realms_host_resolution_base() {
    assert_files(
        EmbeddedModuleGoal::Module,
        r#"
import { start } from './nested/bridge.js';
const realm = new ShadowRealm();
const value = await start(realm);
assert(value === 41, 'Realm request resolves at host base, not caller module directory');
print('Realm import resolution base');
"#,
        &[
            (
                "nested/bridge.js",
                "export function start(realm) { return realm.importValue('./value.js', 'answer'); }",
            ),
            ("value.js", "export const answer = 41;"),
            ("nested/value.js", "export const answer = 99;"),
        ],
        &["Realm import resolution base"],
    );
}

#[test]
fn embedded_realm_requests_keep_aliases_distinct_from_script_edges_and_wait_for_tla_cycles() {
    let source = checked(
        r#"
const realm = new ShadowRealm();
realm.evaluate('globalThis.timeline = "";');
const method = realm.importValue;
const specifier = 'selected';
const first = Reflect.apply(method, realm, [specifier, 'total']);
const second = Reflect.apply(method, realm, [specifier, 'total']);
assert(first !== second, 'fresh aliased-call promises');
Promise.all([first, second, import('selected')]).then(values => {
    // Detached calls: a wrapped function wraps its `this` (GetWrappedValue),
    // so calling it as `values[0]()` would pass the Array and throw.
    const [total0, total1] = values;
    assert(total0 !== total1 && total0() === 5 && total1() === 5, 'fresh callable exports after cyclic TLA');
    assert(values[2].answer === 99, 'Script edge keeps its distinct host authority');
    assert(realm.evaluate('timeline') === 'ba', 'dependency cycle and await finish once');
    return Reflect.apply(method, realm, ['not-declared', 'total']).then(
        () => { throw 'undeclared Realm request reused a Script edge'; },
        error => {
            assert(error instanceof TypeError, 'undeclared Realm request rejects');
            assert(realm.evaluate('timeline') === 'ba', 'rejection does not reevaluate modules');
            print('embedded Realm TLA imports');
        }
    );
});
true;
"#,
    );
    let edge = |referrer, specifier: &str, target: &str| EmbeddedModuleResolutionInput {
        referrer,
        specifier: specifier.into(),
        attributes: Vec::new(),
        target: target.into(),
    };
    let graph = EmbeddedModuleGraph::try_new(
        EmbeddedModuleEntryInput {
            goal: EmbeddedModuleGoal::Script,
            identity: "entry.js".into(),
            source: source.clone(),
            meta_url: "scriptw://shadow-realm/entry.js".into(),
        },
        [
            (
                "a.js",
                "import { readB } from './b.js'; export const a = 2; await Promise.resolve(); globalThis.timeline += 'a'; export function total() { return a + readB(); }",
            ),
            (
                "b.js",
                "import { a } from './a.js'; globalThis.timeline += 'b'; export function readB() { return a + 1; }",
            ),
            ("script.js", "export const answer = 99;"),
        ]
        .into_iter()
        .map(|(identity, source)| EmbeddedModuleSourceInput {
            identity: identity.into(),
            source: source.into(),
            meta_url: format!("scriptw://shadow-realm/{identity}"),
        })
        .collect(),
        vec![
            edge(EmbeddedModuleReferrer::Realm, "selected", "a.js"),
            edge(
                EmbeddedModuleReferrer::Script("entry.js".into()),
                "selected",
                "script.js",
            ),
            edge(
                EmbeddedModuleReferrer::Script("entry.js".into()),
                "not-declared",
                "script.js",
            ),
            edge(
                EmbeddedModuleReferrer::Module("a.js".into()),
                "./b.js",
                "b.js",
            ),
            edge(
                EmbeddedModuleReferrer::Module("b.js".into()),
                "./a.js",
                "a.js",
            ),
        ],
    )
    .expect("explicit Realm and source-module request authorities are valid");
    assert_run(
        EmbeddedModuleGoal::Script,
        &source,
        CompileOptions {
            module_loading_policy: ModuleLoadingPolicy::Embedded(Arc::clone(&graph)),
            ..CompileOptions::default()
        },
        &["embedded Realm TLA imports"],
    );
}

#[test]
fn private_module_vars_keep_nonlocal_reads_and_writes_relative_to_the_defining_realm() {
    assert_files(
        EmbeddedModuleGoal::Script,
        r#"
let offset = 7;
const realm = new ShadowRealm();
realm.evaluate('globalThis.offset = 11;');
Promise.all([
  import('./locals.js'),
  realm.importValue('./locals.js', 'sum'),
  realm.importValue('./locals.js', 'replace')
]).then(values => {
  const root = values[0], sum = values[1], replace = values[2];
  assert(root.sum(4) === 13 && sum(4) === 17, 'nonlocal read uses defining Realm');
  assert(replace(23) === 23 && sum(4) === 29, 'retained function observes a live nonlocal write');
  assert(root.sum(4) === 13 && offset === 7, 'foreign writes preserve entry lexical');
  realm.evaluate('offset = 31;');
  assert(sum(4) === 37 && root.sum(4) === 13, 'retained function resolves current defining-Realm binding');
  print('private locals with Realm globals');
});
true;
"#,
        &[(
            "locals.js",
            r#"
export function sum(stop) {
  var total = 0;
  for (var index = 0; index < stop; index++) total += index;
  return total + offset;
}
export function replace(next) { offset = next; return offset; }
"#,
        )],
        &["private locals with Realm globals"],
    );
}
