use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion,
    ObservedJsValue, RealmBuilder, RunOptions, WasmExecutionFailureKind,
};

fn engine() -> Engine {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    Engine::new(RealmBuilder::new().build())
}

fn compile_options() -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    }
}

fn run_options() -> RunOptions {
    RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    }
}

fn assert_normal_true(source: &str) {
    let observed = engine()
        .observe_script(source, compile_options(), run_options())
        .expect("runtime error controls compile and execute through Wasm AOT");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        observed.completion,
        ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
        "{source}\n{observed:?}"
    );
    assert!(observed.output_events.is_empty(), "{observed:?}");
}

// Catalog text comes from data/runtime_error_message.rs; the TDZ text is also
// emitted by the program-owned RuntimeThrow/IdentifierWriteErrorIr path. The
// duplicate-declaration message is a prepared-source diagnostic, not R text.
const REPEATED_THROWS: &str = r#"
function check(value, label) { if (!value) throw label; }
function nativeType() { Array.from([], 0); }
function nativeRange() { new Array(-1); }
function nativeUri() { decodeURIComponent('%'); }
function programReference() { let value = later; let later = 1; return value; }
function programSyntax() { return eval('let duplicate; let duplicate;'); }
var saved = [], syntaxText = null;
function capture(thunk, Type, name, message) {
  var error, returned = false;
  try { thunk(); returned = true; } catch (caught) { error = caught; }
  check(!returned && error instanceof Type, 'precise throwing completion: ' + name);
  gc();
  check(Object.getPrototypeOf(error) === Type.prototype && error.name === name,
    'fresh intrinsic error kind: ' + name);
  check(typeof error.message === 'string' && error.message.length !== 0 &&
    (message === null || error.message === message), 'message replaces prior kind: ' + name);
  var descriptor = Object.getOwnPropertyDescriptor(error, 'message');
  check(descriptor.value === error.message && descriptor.writable &&
    !descriptor.enumerable && descriptor.configurable, 'own message descriptor: ' + name);
  for (var i = 0; i < saved.length; i++) {
    check(error !== saved[i][0], 'each throw allocates a fresh error');
  }
  saved.push([error, name, error.message, Type.prototype]);
  return error;
}
for (var turn = 0; turn < 4; turn++) {
  capture(nativeType, TypeError, 'TypeError', 'Array.from mapper is not callable');
  capture(nativeRange, RangeError, 'RangeError', 'Invalid array length');
  capture(nativeUri, URIError, 'URIError', 'URI contains an incomplete percent escape');
  capture(programReference, ReferenceError, 'ReferenceError',
    'lexical binding accessed before initialization');
  var syntax = capture(programSyntax, SyntaxError, 'SyntaxError', syntaxText);
  check(syntax.message.includes('duplicate'), 'program-owned syntax diagnostic content');
  syntaxText = syntax.message;
}
gc();
for (var i = 0; i < saved.length; i++) {
  var prior = saved[i];
  check(prior[0].name === prior[1] && prior[0].message === prior[2] &&
    Object.getPrototypeOf(prior[0]) === prior[3], 'later throws do not mutate retained errors');
}
// Leave a different caught diagnostic immediately before each Rust-appended
// uncaught control, so stale Name or Message cannot satisfy that control.
try { nativeType(); } catch (caught) {}
gc();
true;
"#;

#[test]
fn repeated_throw_tails_replace_diagnostics_and_keep_program_messages_across_gc() {
    assert_normal_true(REPEATED_THROWS);
    // Structured completion intentionally exposes object category only. The
    // public run_script boundary reads the diagnostic Name/Message globals.
    for (tail, kind, content) in [
        ("nativeRange();", "RangeError", "Invalid array length"),
        (
            "programReference();",
            "ReferenceError",
            "lexical binding accessed before initialization",
        ),
        ("programSyntax();", "SyntaxError", "duplicate"),
    ] {
        let source = format!("{REPEATED_THROWS}\n{tail}");
        let failure = engine()
            .run_script(&source, compile_options(), run_options())
            .expect_err("the final native/compiler throw must escape the earlier catch");
        assert_eq!(
            failure.wasm_execution_failure_kind(),
            Some(WasmExecutionFailureKind::JavaScriptException),
            "{failure}\n{source}"
        );
        assert_eq!(
            failure.wasm_javascript_exception_constructor_name(),
            Some(kind),
            "{failure}\n{source}"
        );
        assert!(
            failure.message().contains(&format!("{kind}:")) && failure.message().contains(content),
            "current diagnostic replaces the caught TypeError: {failure}\n{source}"
        );
        assert!(failure.parse_diagnostic().is_none(), "{failure}");
        assert!(failure.ir_diagnostic().is_none(), "{failure}");
    }
}

#[test]
fn borrowed_and_explicit_realm_throw_tails_ignore_tampered_error_globals() {
    assert_normal_true(
        r#"
function check(value, label) { if (!value) throw label; }
var realm = __scriptwCreateRealm(), foreign = realm.global;
var LocalType = TypeError, LocalRange = RangeError, LocalUri = URIError;
var LocalReference = ReferenceError, LocalSyntax = SyntaxError;
var ForeignType = foreign.TypeError, ForeignRange = foreign.RangeError;
var ForeignUri = foreign.URIError, ForeignReference = foreign.ReferenceError;
var ForeignSyntax = foreign.SyntaxError;
var localFrom = Array.from, localArray = Array, localDecode = decodeURIComponent;
var remoteFrom = foreign.Array.from, remoteArray = foreign.Array;
var remoteDecode = foreign.decodeURIComponent;
var saved = [], poisonCalls = 0;
function poison() { poisonCalls++; throw 'public error constructor must not run'; }
TypeError = poison; RangeError = poison; URIError = poison;
ReferenceError = poison; SyntaxError = poison;
foreign.TypeError = poison; foreign.RangeError = poison; foreign.URIError = poison;
foreign.ReferenceError = poison; foreign.SyntaxError = poison;
Array.from = poison; foreign.Array.from = poison;
decodeURIComponent = poison; foreign.decodeURIComponent = poison;
function capture(thunk, Type, OtherType, name, content) {
  var error, returned = false;
  try { thunk(); returned = true; } catch (caught) { error = caught; }
  gc();
  check(!returned && error instanceof Type && !(error instanceof OtherType),
    'called or explicit Realm instanceof: ' + name);
  check(Object.getPrototypeOf(error) === Type.prototype && error.constructor === Type &&
    error.name === name, 'canonical prototype after global tampering: ' + name);
  check(typeof error.message === 'string' && error.message.includes(content),
    'selected caller resolves its message before shared error tail: ' + name);
  for (var i = 0; i < saved.length; i++) {
    check(error !== saved[i][0], 'foreign and local throws allocate independently');
  }
  saved.push([error, Type.prototype, error.message]);
}
for (var turn = 0; turn < 3; turn++) {
  capture(function () { localFrom.call(null, [], 0); }, LocalType, ForeignType,
    'TypeError', 'Array.from mapper is not callable');
  capture(function () { remoteFrom.call(null, [], 0); }, ForeignType, LocalType,
    'TypeError', 'Array.from mapper is not callable');
  capture(function () { new localArray(-1); }, LocalRange, ForeignRange,
    'RangeError', 'Invalid array length');
  capture(function () { new remoteArray(-1); }, ForeignRange, LocalRange,
    'RangeError', 'Invalid array length');
  capture(function () { localDecode.call(null, '%'); }, LocalUri, ForeignUri,
    'URIError', 'URI contains an incomplete percent escape');
  capture(function () { remoteDecode.call(null, '%'); }, ForeignUri, LocalUri,
    'URIError', 'URI contains an incomplete percent escape');
  capture(function () { realm.evalScript('(function () { let value = later; let later = 1; return value; })();'); },
    ForeignReference, LocalReference, 'ReferenceError', 'lexical binding accessed before initialization');
  capture(function () { realm.evalScript('let duplicate; let duplicate;'); },
    ForeignSyntax, LocalSyntax, 'SyntaxError', 'duplicate');
}
gc();
for (var i = 0; i < saved.length; i++) {
  check(Object.getPrototypeOf(saved[i][0]) === saved[i][1] &&
    saved[i][0].message === saved[i][2], 'retained error survives later Realm and kind changes');
}
poisonCalls === 0;
"#,
    );
}
