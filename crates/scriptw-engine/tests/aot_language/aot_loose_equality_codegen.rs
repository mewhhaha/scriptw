use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion,
    ObservedJsValue, RealmBuilder,
};

#[test]
fn repeated_loose_equalities_share_codegen_and_preserve_runtime_semantics() {
    let mut source =
        String::from("function repeated(left, right) { let same = 0, different = 0;\n");
    for _ in 0..64 {
        source.push_str("if (left == right) same++; if (left != right) different++;\n");
    }
    source.push_str(
        r#"
return same * 1000 + different;
}
function check(value, label) { if (!value) throw label; }
function equal(left, right) { return left == right; }
function different(left, right) { return left != right; }
check(repeated('64', 64) === 64000, 'repeated equal');
check(repeated(64n, '64.0') === 64, 'repeated unequal');

let trace = [];
const coercible = {
  [Symbol.toPrimitive](hint) { trace.push('primitive:' + hint); return '1'; }
};
function readLeft() { trace.push('read:left'); return coercible; }
function readRight() { trace.push('read:right'); return 1n; }
check(readLeft() == readRight(), 'ordered equality');
check(trace.join(',') === 'read:left,read:right,primitive:default', 'evaluate once before coercion');
trace = [];
check(!(readLeft() != readRight()), 'ordered inequality');
check(trace.join(',') === 'read:left,read:right,primitive:default', 'inequality evaluates once');

let received, finalizers = 0;
const marker = false;
const abrupt = { [Symbol.toPrimitive]() { throw marker; } };
function caught(left, right) {
  try { return equal(left, right); }
  catch (error) { received = error; return 'caught'; }
  finally { finalizers++; }
}
check(caught(NaN, 0) === false && caught(abrupt, 0) === 'caught' &&
  caught(0, abrupt) === 'caught' && received === marker && finalizers === 3,
  'normal false and thrown false');
try { different(abrupt, 1); throw 'missing inequality throw'; }
catch (error) { check(error === marker, 'inequality preserves throw'); }
let skipped = 0;
const object = { valueOf() { skipped++; throw 'unexpected object coercion'; } };
check(equal(object, object) && !equal(object, {}) && !equal(object, null) &&
  !equal(undefined, object) && skipped === 0, 'identity and null do not coerce');

const rows = [
  [null, undefined, true], [null, 0, false], [undefined, 0, false],
  [false, '0', true], [true, '1', true], [true, 1n, true], [false, 0n, true],
  [0, -0, true], [NaN, NaN, false], [Infinity, 'Infinity', true],
  [0n, '', true], [0n, '\uFEFF\u00A0', true], [255n, '0xFF', true],
  [-18446744073709551617n, '-18446744073709551617', true],
  [9007199254740993n, 9007199254740992, false],
  [9007199254740992n, 9007199254740992, true],
  [1n, 1.5, false], [0n, NaN, false], [1n, Infinity, false],
  [0n, null, false], [1n, Symbol(), false]
];
for (const row of rows) {
  check(equal(row[0], row[1]) === row[2] && equal(row[1], row[0]) === row[2], 'primitive equality');
  check(different(row[0], row[1]) === !row[2], 'primitive inequality');
}
for (const text of ['foo', '+', '-', '1.0', '1e0', '1n', '1_0', '+0x1',
  '-0x1', '0x', '0o8', '0b2', '1 0', '\u200B0', '\uD800']) {
  check(!equal(0n, text) && !equal(text, 0n) &&
    different(1n << 70n, text), 'malformed BigInt text is normal false');
}
const htmlDda = __scriptwCreateHTMLDDA();
check(equal(htmlDda, null) && equal(undefined, htmlDda) &&
  !equal(htmlDda, false) && equal(htmlDda, htmlDda), 'HTMLDDA equality');

const other = __scriptwCreateRealm().global;
const foreignEqual = other.Function('left', 'right', 'return left == right;');
const localInvalid = { [Symbol.toPrimitive]() { return {}; } };
const foreignInvalid = other.eval('({ [Symbol.toPrimitive]() { return {}; } })');
let foreignError, localError;
try { foreignEqual(localInvalid, 1); } catch (error) { foreignError = error; }
try { equal(foreignInvalid, 1); } catch (error) { localError = error; }
check(foreignError instanceof other.TypeError && !(foreignError instanceof TypeError) &&
  localError instanceof TypeError && !(localError instanceof other.TypeError),
  'coercion failures use comparison execution Realm');
const foreignMarker = other.eval('({marker: 1})');
const foreignThrow = other.Function('marker',
  'return { [Symbol.toPrimitive]() { throw marker; } };')(foreignMarker);
received = undefined;
try { equal(foreignThrow, 1); } catch (error) { received = error; }
check(received === foreignMarker && foreignEqual(64n, '64') &&
  !foreignEqual(64n, '64.0'), 'cross-Realm values and original throw');
true;
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
        .expect("loose equality workload compiles through Wasm AOT");
    // 128 dynamic equality sites previously repeated the complete BigInt
    // parser/comparison bodies. The outlined workload's measured largest body
    // is 176,904 bytes. Allow ordinary code growth up to one quarter of the
    // one-MiB body that triggers expensive native codegen.
    // Execute this exact profiled artifact below rather than compiling again.
    eprintln!(
        "loose equality P: {} bytes, {} code bytes, largest {} bytes",
        compilation.footprint.module_bytes,
        compilation.footprint.code_body_bytes,
        compilation.footprint.largest_code_body_bytes
    );
    assert!(
        compilation.footprint.largest_code_body_bytes < 256 * 1024,
        "loose equality expansion: {:?}",
        compilation.footprint
    );
    let execution = engine
        .profile_wasm_execution(&compilation.artifact, Some(30_000), true)
        .expect("the measured loose equality artifact executes");
    assert_eq!(execution.outcome.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        execution.outcome.completion,
        ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
        "{}",
        execution.outcome.note
    );
}
