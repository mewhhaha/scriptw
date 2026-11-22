use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_typeof_fixture(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    for directive in ["", "\"use strict\";"] {
        let source = format!("{directive}\n{source}");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                CompileOptions {
                    host_surface_policy: HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(30_000),
                    ..RunOptions::default()
                },
            )
            .expect("typeof comparisons execute compiled Wasm");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            observed.completion
        );
        assert_eq!(
            observed.output_events,
            [HostOutputEvent::PrintLine("ok".into())]
        );
    }
}

#[test]
fn literal_typeof_comparisons_keep_all_tags_callability_effects_and_abrupts() {
    assert_typeof_fixture(include_str!("../fixtures/typeof_literal_comparisons.js"));
}

#[test]
fn literal_typeof_comparisons_keep_html_dda_local_to_the_actual_exotic() {
    assert_typeof_fixture(include_str!("../fixtures/typeof_html_dda_comparisons.js"));
}

#[test]
fn typeof_if_else_chains_select_the_first_true_test_for_every_class() {
    // If-else chains of typeof tests on one parameter, with equal, loose,
    // negated and never-true spellings and a final else, reached by every
    // class and both exotic slots; a chain cut short by another test; and a
    // lexical binding in its TDZ, which throws before any arm.
    assert_typeof_fixture(
        r#"
function check(value, message) { if (!value) throw message; }
function chain(v) {
  var trace = 'start';
  if (typeof v === 'Object') trace = 'never';
  else if (typeof v == 'undefined') trace = 'undefined';
  else if ('boolean' === typeof v) trace = 'boolean';
  else if (typeof v === 'number') trace = 'number';
  else if (typeof v == 'string') { trace = 'string'; }
  else if (typeof v === 'symbol') trace = 'symbol';
  else if (typeof v === 'bigint') trace = 'bigint';
  else if ('function' == typeof v) trace = 'function';
  else trace = 'object';
  return trace;
}
function negated(v) {
  var kind = 'none';
  if (typeof v != 'number') kind = 'other';
  else if (typeof v === 'number') kind = 'number';
  else if (typeof v === 'string') kind = 'unreached';
  return kind;
}
function cut(v) {
  if (typeof v === 'undefined') return 'undefined';
  else if (typeof v === 'object') return 'object';
  else if (typeof v === 'function') return 'function';
  else if (typeof v !== 'number' && typeof v !== 'string') return 'other';
  else if (typeof v === 'number') return 'number';
  return 'string';
}
function lexical() {
  try {
    if (typeof value === 'number') return 'number';
    else if (typeof value === 'string') return 'string';
    else if (typeof value === 'object') return 'object';
    return 'other';
  } catch (error) { return error instanceof ReferenceError ? 'tdz' : 'wrong'; }
  let value = 1;
}
var callable = function () {};
var revoked = Proxy.revocable(callable, {});
revoked.revoke();
var dda = __scriptwCreateHTMLDDA();
var cases = [
  [undefined, 'undefined'], [null, 'object'], [true, 'boolean'], [0, 'number'], [-0, 'number'],
  [NaN, 'number'], ['', 'string'], ['s', 'string'], [Symbol(), 'symbol'], [1n, 'bigint'],
  [{}, 'object'], [[], 'object'], [(function () { return arguments; })(), 'object'],
  [callable, 'function'], [callable.bind(null), 'function'], [class {}, 'function'],
  [new Proxy(callable, {}), 'function'], [revoked.proxy, 'function'], [new Proxy({}, {}), 'object'],
  [dda, 'undefined'], [Object.create(dda), 'object'], [new Proxy(dda, {}), 'function']
];
for (var round = 0; round < 3; round++) {
  for (var i = 0; i < cases.length; i++) {
    var value = cases[i][0], spelling = cases[i][1];
    check(chain(value) === spelling, 'chain ' + i + ' ' + chain(value));
    check(negated(value) === (spelling === 'number' ? 'number' : 'other'), 'negated ' + i);
    var expected = spelling === 'undefined' || spelling === 'object' || spelling === 'function' ||
      spelling === 'number' || spelling === 'string' ? spelling : 'other';
    check(cut(value) === expected, 'cut ' + i + ' ' + cut(value));
  }
  gc();
}
check(lexical() === 'tdz', 'tdz');
print('ok');
"#,
    );
}
