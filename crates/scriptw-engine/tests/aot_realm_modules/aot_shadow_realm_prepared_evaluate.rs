use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, ObservedCompletion, ObservedJsValue, RealmBuilder,
    RunOptions, WasmExecutionFailureKind,
};
use scriptw_ir::DynamicSourceRuntimeOperation;

fn assert_modes(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{source}");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(60_000),
                    ..RunOptions::default()
                },
            )
            .unwrap_or_else(|error| {
                panic!("ShadowRealm source controls failed: {error}\n{source}")
            });
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
            "{source}"
        );
        assert!(observed.output_events.is_empty(), "{source}");
    }
}

#[test]
fn finite_evaluation_has_fresh_lexicals_and_strictness_owned_variable_lifetimes() {
    assert_modes(
        r#"
        const realm = new ShadowRealm();
        if (realm.evaluate('let temporary = 1; temporary') !== 1) throw 'first-lexical';
        if (realm.evaluate('let temporary = 2; temporary') !== 2) throw 'repeated-lexical';
        if (realm.evaluate('typeof temporary') !== 'undefined') throw 'lexical-leak';
        if (realm.evaluate('var shared = 3; shared') !== 3) throw 'sloppy-var';
        if (realm.evaluate('shared += 1; shared') !== 4) throw 'shared-var';
        if (realm.evaluate("'use strict'; var hidden = 5; hidden") !== 5) throw 'strict-var';
        if (realm.evaluate('typeof hidden') !== 'undefined') throw 'strict-var-leak';
        if (realm.evaluate('delete shared') !== true) throw 'eval-var-configurability';
        if (realm.evaluate('typeof shared') !== 'undefined') throw 'delete-shared';
        if (realm.evaluate('unboundWrite = 7; unboundWrite') !== 7) throw 'inherited-strictness';
        if (realm.evaluate('this === globalThis') !== true) throw 'global-this';
        const retained = realm.evaluate('let retained = 13; () => retained');
        realm.evaluate('let retained = 19; retained');
        if (retained() !== 13) throw 'retained-eval-environment';
        if (typeof unboundWrite !== 'undefined') throw 'outer-global-leak';
        true;
        "#,
    );
}

#[test]
fn finite_candidates_keep_actual_method_receiver_aliases_and_nested_realm_dispatch() {
    assert_modes(include_str!(
        "../fixtures/shadow_realm/finite_candidates_receiver_aliases.js"
    ));
}

#[test]
fn source_argument_validation_and_parse_errors_precede_shadow_execution() {
    assert_modes(
        r#"
        const realm = new ShadowRealm();
        let conversions = 0;
        const source = { toString() { conversions++; throw 'coercion'; } };
        try { realm.evaluate(source); throw 'object-source-accepted'; }
        catch (error) { if (!(error instanceof TypeError)) throw error; }
        try { realm.evaluate(new String('1')); throw 'boxed-source-accepted'; }
        catch (error) { if (!(error instanceof TypeError)) throw error; }
        try { realm.evaluate(Symbol('source')); throw 'symbol-source-accepted'; }
        catch (error) { if (!(error instanceof TypeError)) throw error; }
        const evaluate = realm.evaluate;
        try { evaluate?.call({}, '123'); throw 'optional-call-receiver-accepted'; }
        catch (error) { if (!(error instanceof TypeError)) throw error; }
        try { evaluate.apply?.({}, ['124']); throw 'optional-apply-receiver-accepted'; }
        catch (error) { if (!(error instanceof TypeError)) throw error; }
        if (conversions !== 0) throw 'source-coercion';
        try { realm.evaluate('let = ;'); throw 'syntax-accepted'; }
        catch (error) { if (!(error instanceof SyntaxError)) throw error; }
        try { realm.evaluate('new.target'); throw 'new-target-accepted'; }
        catch (error) { if (!(error instanceof SyntaxError)) throw error; }
        try { realm.evaluate('throw new SyntaxError("target")'); throw 'target-throw-accepted'; }
        catch (error) { if (!(error instanceof TypeError)) throw error; }
        if (realm.evaluate('2 + 3') !== 5) throw 'realm-after-throw';
        true;
        "#,
    );
}

#[test]
fn unmatched_shadow_source_retains_uncatchable_typed_runtime_rejection() {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    let error = Engine::new(RealmBuilder::new().build())
        .run_script(
            r#"
            const realm = new ShadowRealm();
            let source = '23;';
            realm.evaluate(source);
            source += '/*' + Math.random() + '*/';
            try { realm.evaluate?.call(realm, source); }
            catch (error) { throw 'must not catch capability rejection'; }
            throw 'must not continue';
            "#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(60_000),
                ..RunOptions::default()
            },
        )
        .expect_err("an unavailable source cannot execute a different prepared candidate");
    assert_eq!(
        error.runtime_dynamic_source_operations(),
        vec![DynamicSourceRuntimeOperation::ShadowRealmEvaluate],
        "{error}"
    );
    assert_eq!(
        error.wasm_execution_failure_kind(),
        Some(WasmExecutionFailureKind::DynamicSource)
    );
    assert!(error.parse_diagnostic().is_none(), "{error}");
    assert!(error.ir_diagnostic().is_none(), "{error}");
}

#[test]
fn finite_templates_run_unchanged_pinned_wrapped_names_and_global_properties() {
    const STA: &str = include_str!("../../../../test262/vendor/test262/harness/sta.js");
    const ASSERT: &str = include_str!("../../../../test262/vendor/test262/harness/assert.js");
    const PROPERTY: &str =
        include_str!("../../../../test262/vendor/test262/harness/propertyHelper.js");
    for fixture in [
        include_str!(
            "../../../../test262/vendor/test262/test/built-ins/ShadowRealm/WrappedFunction/name.js"
        ),
        include_str!(
            "../../../../test262/vendor/test262/test/built-ins/ShadowRealm/prototype/evaluate/globalthis-available-properties.js"
        ),
    ] {
        assert_modes(&format!("{STA}\n{ASSERT}\n{PROPERTY}\n{fixture}\ntrue;"));
    }
}

#[test]
fn finite_template_conversion_keeps_method_get_arguments_and_throw_order_live() {
    assert_modes(
        r#"
const realm = new ShadowRealm();
let trace = '';
const holder = {
  get evaluate() {
    trace += 'get,';
    return ShadowRealm.prototype.evaluate;
  }
};
let sum = 0;
for (const value of [
  { toString() { trace += 'first,'; return '17'; } },
  { toString() { trace += 'second,'; return '19'; } }
]) {
  sum += holder.evaluate.call(realm, `${String(value)};`, (trace += 'extra,', 0));
}
const marker = {};
try {
  holder.evaluate.call(realm, `${{
    toString() { trace += 'throw,'; throw marker; }
  }};`, (trace += 'wrong-extra,', 0));
  throw 'missing-conversion-throw';
} catch (error) {
  if (error !== marker) throw error;
}
sum === 36 && trace === 'get,first,extra,get,second,extra,get,throw,';
"#,
    );
}

#[test]
fn finite_filter_hints_keep_replaced_method_and_callback_results_live() {
    assert_modes(
        r#"
const realm = new ShadowRealm();
const values = ['17', '19', '23'];
let trace = '';
Object.defineProperty(values, 'filter', {
  get() {
    trace += 'get,';
    return function(callback) {
      trace += 'call,';
      callback('17');
      return ['19', '23'];
    };
  }
});
const selected = values.filter(value => {
  trace += 'first:' + value + ',';
  return false;
});
const available = selected.filter(value => {
  trace += 'second:' + value + ',';
  return realm.evaluate(`${value};`) === Number(value);
});
available.join(',') === '19,23'
  && trace === 'get,call,first:17,second:19,second:23,';
"#,
    );
}

#[test]
fn filter_values_outside_the_finite_domain_keep_typed_source_rejection() {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    let error = Engine::new(RealmBuilder::new().build())
        .run_script(
            r#"
const realm = new ShadowRealm();
const values = ['17;', '19;'];
values.filter = function(callback) {
  callback('17;');
  return ['23;/*' + Math.random() + '*/'];
};
const selected = values.filter(value => true);
try {
  selected.filter(value => realm.evaluate(`${value}`));
} catch (error) {
  throw 'must not catch capability rejection';
}
throw 'must not continue';
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(60_000),
                ..RunOptions::default()
            },
        )
        .expect_err("a replaced filter cannot select a different prepared source");
    assert_eq!(
        error.runtime_dynamic_source_operations(),
        vec![DynamicSourceRuntimeOperation::ShadowRealmEvaluate],
        "{error}"
    );
    assert_eq!(
        error.wasm_execution_failure_kind(),
        Some(WasmExecutionFailureKind::DynamicSource)
    );
    assert!(error.parse_diagnostic().is_none(), "{error}");
    assert!(error.ir_diagnostic().is_none(), "{error}");
}

#[test]
fn returned_callable_hints_run_unchanged_pinned_nested_realm_fixtures() {
    const STA: &str = include_str!("../../../../test262/vendor/test262/harness/sta.js");
    const ASSERT: &str = include_str!("../../../../test262/vendor/test262/harness/assert.js");
    for fixture in [
        include_str!(
            "../../../../test262/vendor/test262/test/built-ins/ShadowRealm/prototype/evaluate/nested-realms.js"
        ),
        include_str!(
            "../../../../test262/vendor/test262/test/built-ins/ShadowRealm/prototype/evaluate/wrapped-function-multiple-different-realms-nested.js"
        ),
    ] {
        assert_modes(&format!("{STA}\n{ASSERT}\n{fixture}\ntrue;"));
    }
}

#[test]
fn equal_prepared_text_keeps_distinct_realm_and_captured_lexical_state_live() {
    assert_modes(
        r#"
globalThis.marker = 'outer';
const first = new ShadowRealm();
const second = new ShadowRealm();
first.evaluate('globalThis.marker = "first";');
second.evaluate('globalThis.marker = "second";');
const source = `
  const owner = globalThis.marker;
  const inner = new ShadowRealm();
  let calls = 0;
  (text) => owner + ':' + (++calls) + ':' + inner.evaluate(text);
`;
const left = first.evaluate(source);
const right = second.evaluate(source);
if (left === right) throw 'same-wrapper';
if (left('17;') !== 'first:1:17') throw 'first-capture';
if (right('19;') !== 'second:1:19') throw 'second-capture';
first.evaluate('globalThis.marker = "changed";');
if (left('23;') !== 'first:2:23') throw 'retained-first-environment';
if (right('29;') !== 'second:2:29') throw 'retained-second-environment';
globalThis.marker === 'outer';
"#,
    );
}

#[test]
fn returned_callable_hints_keep_live_callee_get_arguments_method_and_throws() {
    assert_modes(
        r#"
const realm = new ShadowRealm();
const original = realm.evaluate(`
  const inner = new ShadowRealm();
  (source) => inner.evaluate(source);
`);
let trace = '';
let run = original;
run = function(source, extra) { trace += 'call,'; return 'local:' + source; };
const holder = { get run() { trace += 'get,'; return run; } };
if (holder.run((trace += 'arg,', '31;'), (trace += 'extra,', 0)) !== 'local:31;') {
  throw 'reassigned-callee';
}
const marker = {};
try {
  holder.run((trace += 'throw,', (() => { throw marker; })()), (trace += 'wrong-extra,', 0));
  throw 'missing-argument-throw';
} catch (error) {
  if (error !== marker) throw error;
}
if (trace !== 'get,arg,extra,call,get,throw,') throw trace;
if (original('37;') !== 37) throw 'original-wrapper';
let coercions = 0;
try {
  original({ toString() { coercions++; return '41;'; } });
  throw 'object-argument-accepted';
} catch (error) {
  if (!(error instanceof TypeError)) throw error;
}
if (coercions !== 0) throw 'invented-source-coercion';
try {
  original('throw new Error("inner");');
  throw 'missing-inner-throw';
} catch (error) {
  if (!(error instanceof TypeError)) throw error;
}
const replaceMethod = realm.evaluate(`
  const inner = new ShadowRealm();
  (source, replace) => {
    if (replace) inner.evaluate = function(text) { return 'replacement:' + text; };
    return inner.evaluate(source);
  };
`);
if (replaceMethod('43;', false) !== 43) throw 'original-method';
replaceMethod('47;', true) === 'replacement:47;';
"#,
    );
}

#[test]
fn unknown_returned_callable_source_keeps_uncatchable_typed_runtime_rejection() {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    const SOURCE: &str = r#"
const realm = new ShadowRealm();
const run = realm.evaluate('const inner = new ShadowRealm(); source => inner.evaluate(source)');
let text = '53;';
if (run(text) !== 53) throw 'known-source';
text += '/*' + Math.random() + '*/';
try { run(text); }
catch (error) { throw 'must not catch capability rejection'; }
throw 'must not continue';
"#;
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{SOURCE}");
        let error = Engine::new(RealmBuilder::new().build())
            .run_script(
                &source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(60_000),
                    ..RunOptions::default()
                },
            )
            .expect_err("an unknown nested source cannot select another prepared unit");
        assert_eq!(
            error.runtime_dynamic_source_operations(),
            vec![DynamicSourceRuntimeOperation::ShadowRealmEvaluate],
            "{error}"
        );
        assert_eq!(
            error.wasm_execution_failure_kind(),
            Some(WasmExecutionFailureKind::DynamicSource)
        );
        assert!(error.parse_diagnostic().is_none(), "{error}");
        assert!(error.ir_diagnostic().is_none(), "{error}");
    }
}
