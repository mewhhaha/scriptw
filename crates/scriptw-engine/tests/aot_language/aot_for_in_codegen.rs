use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion,
    ObservedJsValue, RealmBuilder,
};

#[test]
fn repeated_for_in_loops_share_advance_code_and_retain_completion() {
    let mut source = String::from("function repeated(value) { 'use strict'; let count = 0;\n");
    for _ in 0..16 {
        source.push_str("for (var key in value) count++;\n");
    }
    source.push_str(
        r#"
return count;
}
function check(value, label) { if (!value) throw label; }
function keys(value) {
  const result = [];
  for (const key in value) result.push(key);
  return result.join(',');
}
const prototype = {shadowed: 1, inherited: 2};
const object = Object.create(prototype);
Object.defineProperty(object, 'shadowed', {value: 3});
let getterCalls = 0;
Object.defineProperty(object, 'own', {
  enumerable: true, get() { getterCalls++; throw 'enumeration invoked getter'; }
});
object[Symbol()] = 4;
check(repeated(object) === 32 && repeated(null) === 0 && repeated(undefined) === 0 &&
  getterCalls === 0, 'repeated own/inherited keys without values');
const trace = [];
const proxy = new Proxy({first: 1, second: 2}, {
  ownKeys(target) { trace.push('keys'); return Reflect.ownKeys(target); },
  getOwnPropertyDescriptor(target, key) {
    trace.push('descriptor:' + key);
    return Reflect.getOwnPropertyDescriptor(target, key);
  },
  getPrototypeOf() { trace.push('prototype'); return null; }
});
check(keys(proxy) === 'first,second' &&
  trace.filter(event => event !== 'prototype').join(',') === 'keys,descriptor:first,descriptor:second' &&
  trace.filter(event => event === 'prototype').length === 1,
  'one ordered trap observation per key');
const marker = {};
let received, finalized = 0;
try {
  for (const key in new Proxy({first: 1}, {
    getOwnPropertyDescriptor() { throw marker; }
  })) throw 'entered rejected descriptor';
} catch (error) { received = error; }
finally { finalized++; }
check(received === marker && finalized === 1, 'descriptor throw retains active handler');
let visits = '';
for (const key in {first: 1, second: 2}) {
  visits += key;
  if (key === 'first') continue;
  break;
}
check(visits === 'firstsecond', 'continue and break preserve cursor');
const other = __scriptwCreateRealm().global;
const foreignReject = other.Function('value',
  'try { for (var key in value) {} } catch (error) { return error; } throw "missing error";');
const revoked = Proxy.revocable({}, {});
revoked.revoke();
let localError;
try { keys(revoked.proxy); } catch (error) { localError = error; }
const foreignError = foreignReject(revoked.proxy);
check(localError instanceof TypeError && !(localError instanceof other.TypeError) &&
  foreignError instanceof other.TypeError && !(foreignError instanceof TypeError),
  'revoked enumeration errors use execution Realm');
// The final completed advance returns Undefined to its consumer, but must not
// replace the preceding nonempty loop-body StatementList completion.
for (const completionKey in {first: 1, second: 2}) { true; }
"#,
    );
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let engine = Engine::new(RealmBuilder::new().build());
    let compilation = engine
        .profile_script_compilation(
            &source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
        )
        .expect("repeated ForIn source compiles");
    // The React control measured roughly 17 KiB of inline advancement per
    // loop. Sixteen loops must share that algorithm while retaining their
    // independent source bodies and cursors. Inspect and execute one artifact.
    assert!(
        compilation.footprint.largest_code_body_bytes < 128 * 1024,
        "ForIn expansion: {:?}",
        compilation.footprint
    );
    eprintln!(
        "ForIn P: {} bytes, {} code bytes, largest {} bytes",
        compilation.footprint.module_bytes,
        compilation.footprint.code_body_bytes,
        compilation.footprint.largest_code_body_bytes
    );
    let execution = engine
        .profile_wasm_execution(&compilation.artifact, Some(30_000), true)
        .expect("the measured ForIn artifact executes");
    assert_eq!(execution.outcome.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        execution.outcome.completion,
        ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
        "{}",
        execution.outcome.note
    );
}
