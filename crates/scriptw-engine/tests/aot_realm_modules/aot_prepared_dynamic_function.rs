use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_function_trace(source: &str, expected: &[&str]) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
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
        .expect("Function source must compile and execute through Wasm AOT");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        outcome.completion
    );
    let expected = expected
        .iter()
        .map(|line| HostOutputEvent::PrintLine((*line).to_string()))
        .collect::<Vec<_>>();
    assert_eq!(outcome.output_events, expected, "source:\n{source}");
}

fn assert_unmatched_source_tuples(source: &str, tuples: &[&[&str]], for_of: bool) {
    let unit = Engine::new(RealmBuilder::new().build())
        .compile_script(
            source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
        )
        .expect("outer source lowers without expanding finite source admission");
    let script = unit.ir.script.as_ref().expect("Script IR");
    assert!(script.prepared_dynamic_functions.len() <= 256);
    for tuple in tuples {
        assert!(
            !script.prepared_dynamic_functions.iter().any(|entry| entry
                .arguments
                .iter()
                .map(String::as_str)
                .eq(tuple.iter().copied())),
            "actual tuple must retain unmatched runtime dispatch: {tuple:?}"
        );
    }
    if for_of {
        assert!(
            script.body.statements.iter().any(|statement| matches!(
                statement,
                scriptw_ir::StatementIr::ForOfIterator { .. }
            )),
            "the generic iterator must remain in the outer program"
        );
    }
}

#[test]
fn body_reference_hints_keep_live_conversions_and_call_forwarding() {
    let source = r#"
var counter=0;
var p={toString:function(){print('key'+(++counter));return 'key'+counter;}};
var body='return key1+key2+key3;';
print(Function(p,p,p,body)(1,2,3));
print(counter);
counter=0;
print(Function(p+','+p,p,body)(4,5,6));
print(counter);
counter=0;
print(Function(p+','+p+','+p,body)(7,8,9));
print(counter);
var target={};
counter=0;
Function(p,'key2,key3','this.value=key1;').call(target,31);
print(target.value);
print(counter);
counter=0;
Function(p,'key2,key3','this.value=key1;').apply(target,[32]);
print(target.value);
print(counter);
"#;
    for prefix in ["", "'use strict';"] {
        assert_function_trace(
            &format!("{prefix}{source}"),
            &[
                "key1", "key2", "key3", "6", "3", "key1", "key2", "key3", "15", "3", "key1",
                "key2", "key3", "24", "3", "key1", "31", "1", "key1", "32", "1",
            ],
        );
    }
}

#[test]
fn body_reference_hints_preserve_abrupt_conversion_order() {
    let source = r#"
var counter=0;
var marker={};
var p={toString:function(){
  var name='key'+(++counter);
  print(name);
  if(counter===2) throw marker;
  return name;
}};
try { Function(p,p,p,'return key1+key2+key3;'); }
catch(error) { print(error===marker); }
print(counter);
"#;
    for prefix in ["", "'use strict';"] {
        assert_function_trace(&format!("{prefix}{source}"), &["key1", "key2", "true", "2"]);
    }
}

#[test]
fn body_reference_hints_require_an_exact_runtime_source_tuple() {
    let source = r#"
var key1 = 17;
var counter = 3;
var p = { toString() { var name = 'key' + (++counter); print(name); return name; } };
var created = Function(p, 'return key1;');
print(created(42));
print(counter);
print(created.length);
print(created.toString().includes('key4'));
"#;
    for prefix in ["", "'use strict';"] {
        let source = format!("{prefix}{source}");
        assert_unmatched_source_tuples(&source, &[&["key4", "return key1;"]], false);
        assert_function_trace(&source, &["key4", "17", "4", "1", "true"]);
    }
}

#[test]
fn hoisted_hints_retain_execution_at_the_256_body_boundary() {
    let literals = (0..256)
        .map(|index| format!("'return {index};'"))
        .collect::<Vec<_>>()
        .join(",");
    for prefix in ["", "'use strict';"] {
        assert_function_trace(
            &format!(
                "{prefix} [{literals}].forEach(function(body, index) {{ var body; if (index === 0 || index === 255) print(Function(body)()); }});"
            ),
            &["0", "255"],
        );
    }
}

#[test]
fn forward_var_sources_keep_live_values_and_redeclaration_effects() {
    let source = r#"
eval(null);
try { var empty = new Function(body); } catch (error) { throw error; }
print(empty());
var body;
var parameters = 'a,b,c';
try { var emptyParameters = new Function(parameters, laterBody); }
catch (error) { throw error; }
print(emptyParameters());
var laterBody;
var numericBody = 'return 1.1;';
try { var numeric = new Function(laterParameters, numericBody); }
catch (error) { throw error; }
print(numeric());
var laterParameters;
body = 'return 17;';
var body;
print(Function(body)());
body = { toString: function () { print('convert'); return 'return 17;'; } };
var body;
print(Function(body)());
function withParameter(body) { var body; return Function(body)(); }
print(withParameter('return 17;'));
function withLocal() {
  try { var result = Function(localBody)(); } catch (error) { throw error; }
  var localBody;
  return result;
}
print(withLocal());
"#;
    for prefix in ["", "'use strict';"] {
        assert_function_trace(
            &format!("{prefix}{source}"),
            &[
                "undefined",
                "undefined",
                "1.1",
                "17",
                "convert",
                "17",
                "17",
                "undefined",
            ],
        );
    }
}

#[test]
fn declared_constructor_aliases_keep_source_comments_and_live_property_reads() {
    let source = r#"
async function asyncSource() {}
async function* generatorSource() {}
var AsyncConstructor = asyncSource.constructor;
var GeneratorConstructor = generatorSource.constructor;
var asyncCreated = AsyncConstructor('a', ' /* a */ b, c /* b */ //', '/* c */ ; /* d */ //');
var generatorCreated = GeneratorConstructor('a', ' /* a */ b, c /* b */ //', '/* c */ ; /* d */ //');
print(asyncCreated.toString() === 'async function anonymous(a, /* a */ b, c /* b */ //\n) {\n/* c */ ; /* d */ //\n}');
print(generatorCreated.toString() === 'async function* anonymous(a, /* a */ b, c /* b */ //\n) {\n/* c */ ; /* d */ //\n}');
var asyncPrototype = Object.getPrototypeOf(asyncSource);
var descriptor = Object.getOwnPropertyDescriptor(asyncPrototype, 'constructor');
print(descriptor.writable === false && descriptor.configurable === true);
Object.defineProperty(asyncPrototype, 'constructor', {
  value: function (body) { print(body); return 31; }
});
var ChangedConstructor = asyncSource.constructor;
print(ChangedConstructor('return 9;'));
Object.defineProperty(asyncPrototype, 'constructor', descriptor);
Object.defineProperty(generatorSource, 'constructor', {
  get: function () {
    print('get');
    return function (body) { print(body); return 32; };
  }
});
var OwnConstructor = generatorSource.constructor;
print(OwnConstructor('return 8;'));
"#;
    for prefix in ["", "'use strict';"] {
        assert_function_trace(
            &format!("{prefix}{source}"),
            &[
                "true",
                "true",
                "true",
                "return 9;",
                "31",
                "get",
                "return 8;",
                "32",
            ],
        );
    }
}

#[test]
fn constructor_parameter_patterns_bind_in_local_foreign_and_resumable_functions() {
    assert_function_trace(
        r#"
var localObject = Function('{value}', 'return value;');
var localArray = Function('[value]', 'return value;');
print(localObject({value: 21}));
print(localArray([22]));
var foreign = __scriptwCreateRealm().global;
var foreignObject = new foreign.Function('{value}', 'return value;');
var foreignArray = new foreign.Function('[value]', 'return value;');
print(foreignObject({value: 23}));
print(foreignArray([24]));
print(Object.getPrototypeOf(foreignObject) === foreign.Function.prototype);
print(localObject.toString() === 'function anonymous({value}\n) {\nreturn value;\n}');
var GeneratorFunction = (function* () {}).constructor;
print(GeneratorFunction('{value}', 'yield value;')({value: 25}).next().value);
var AsyncFunction = (async function () {}).constructor;
AsyncFunction('[value]', 'return value;')([26]).then(function (value) { print(value); });
var AsyncGeneratorFunction = (async function* () {}).constructor;
AsyncGeneratorFunction('{value}', 'yield value;')({value: 27}).next().then(function (result) { print(result.value); });
"#,
        &["21", "22", "23", "24", "true", "true", "25", "26", "27"],
    );
}

#[test]
fn nested_global_source_candidates_preserve_syntax_errors_and_live_callees() {
    assert_function_trace(
        r#"
eval(null);
var Constructor = Object.getPrototypeOf(async function* () {}).constructor;
Constructor('x = await 42');
Constructor('x = yield');
eval(null);
function rejectAwait() { Constructor('x = await 42', ''); }
function rejectYield() { Constructor('x = yield', ''); }
try { rejectAwait(); } catch (error) { print(error instanceof SyntaxError); }
try { rejectYield(); } catch (error) { print(error instanceof SyntaxError); }
var original = Constructor;
Constructor = function () { return 31; };
function replacement() { return Constructor('x = await 42', ''); }
print(replacement());
function shadow(Constructor) { return Constructor('x = yield', ''); }
print(shadow(function () { return 32; }));
Constructor = original;
async function* rejectAsyncAwait() { Constructor('x = await 43', ''); }
async function* rejectAsyncYield() { Constructor('x = yield 44', ''); }
rejectAsyncAwait().next().then(function () { print('wrong'); }, function (error) {
  print(error instanceof SyntaxError);
  return rejectAsyncYield().next();
}).then(function () { print('wrong'); }, function (error) { print(error instanceof SyntaxError); });
var body = 'return 9;';
function build() { return Function(body); }
print(build()());
"#,
        &["true", "true", "31", "32", "9", "true", "true"],
    );
}

#[test]
fn static_constructor_bodies_are_fresh_global_functions_with_real_parameters() {
    assert_function_trace(
        r#"
var outer = 3;
let lexical = 7;
function create() {
  let outer = 99;
  return Function('value', 'return value + outer + lexical;');
}
var first = create();
var second = create();
print(first(2));
print(first !== second);
print(first.name + ':' + first.length);
print(Function('return typeof anonymous;')());
var closes = Function('value', 'return function () { return value; };');
print(closes(8)());
print(closes(9)());
var Nested = Function('return Function("return 11;");');
print(Nested()());
"#,
        &["12", "true", "anonymous:1", "undefined", "8", "9", "11"],
    );
}

#[test]
fn prepared_generator_async_and_async_generator_use_normal_execution_protocols() {
    assert_function_trace(
        r#"
var GeneratorFunction = (function* () {}).constructor;
var AsyncFunction = (async function () {}).constructor;
var AsyncGeneratorFunction = (async function* () {}).constructor;
var generator = GeneratorFunction('value', 'yield value; return value + 1;')(4);
print(generator.next().value);
print(generator.next().value);
AsyncFunction('value', 'return await value + 1;')(6).then(function (value) { print(value); });
var asyncGenerator = AsyncGeneratorFunction('value', 'yield await value; return value + 1;')(8);
asyncGenerator.next().then(function (result) { print(result.value); return asyncGenerator.next(); }).then(function (result) { print(result.value); });
"#,
        &["4", "5", "7", "8", "9"],
    );
}

#[test]
fn prepared_grammar_errors_are_catchable_and_replacement_identity_is_preserved() {
    assert_function_trace(
        r#"
print('before');
try { Function('a)', 'return 1;'); }
catch (error) { print(error instanceof SyntaxError); }
try { Function('a,a', '"use strict"; return a;'); }
catch (error) { print(error instanceof SyntaxError); }
print(Function('')());
var Original = Function;
Function = function (body) { print(body); return 42; };
print(Function('return 4;'));
Function = Original;
print(Function('return 5;')());
"#,
        &[
            "before",
            "true",
            "true",
            "undefined",
            "return 4;",
            "42",
            "5",
        ],
    );
}

#[test]
fn prepared_function_source_preserves_reflection_and_constructor_behavior() {
    assert_function_trace(
        r#"
var Constructor = Function('value', 'this.value = value;');
var instance = new Constructor(13);
print(instance.value);
print(instance instanceof Constructor);
print(Constructor.toString() === 'function anonymous(value\n) {\nthis.value = value;\n}');
var strict = Function('"use strict"; return this;');
print(strict() === undefined);
"#,
        &["13", "true", "true", "true"],
    );
}

#[test]
fn runtime_argument_coercions_precede_prepared_source_selection() {
    assert_function_trace(
        r#"
Function('value', 'return value;');
try { Function('bad)', 'return value;'); } catch (error) {}
var trace = '';
var compiled = Reflect.construct(Function, [
  { toString() { trace += 'parameter;'; return 'value'; } },
  { toString() { trace += 'body;'; return 'return value;'; } }
]);
print(trace);
print(compiled(17));
trace = '';
try {
  Reflect.construct(Function, [
    { toString() { trace += 'parameter;'; return 'bad)'; } },
    { toString() { trace += 'body;'; return 'return value;'; } }
  ]);
} catch (error) { print(error instanceof SyntaxError); }
print(trace);
"#,
        &["parameter;body;", "17", "true", "parameter;body;"],
    );
}

#[test]
fn unregistered_source_arguments_still_coerce_in_order_and_propagate_abrupt_completion() {
    let mut expected = vec!["true", "first;second;"];
    for _ in 0..4 {
        expected.extend([
            "true",
            "first;second;third;",
            "true",
            "parameter;body;",
            "true",
        ]);
    }
    assert_function_trace(
        r#"
var trace = '';
var sentinel = {};
try {
  Function(
    { toString() { trace += 'first;'; return 'x'; } },
    { toString() { trace += 'second;'; throw sentinel; } },
    { toString() { trace += 'third;'; return ''; } }
  );
} catch (error) { print(error === sentinel); }
print(trace);
var abruptSource = Object.create(null);
abruptSource.toString = function () { trace += 'third;'; throw sentinel; };
var GeneratorFunction = Object.getPrototypeOf(function* () {}).constructor;
var AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function* () {}).constructor;
Function('x', 'return x;');
Function('y', 'return y + 1;');
GeneratorFunction('x', 'return x;');
GeneratorFunction('y', 'return y + 1;');
AsyncFunction('x', 'return x;');
AsyncFunction('y', 'return y + 1;');
AsyncGeneratorFunction('x', 'return x;');
AsyncGeneratorFunction('y', 'return y + 1;');
for (var Constructor of [Function, GeneratorFunction, AsyncFunction, AsyncGeneratorFunction]) {
  trace = '';
  try {
    Reflect.construct(Constructor, [
      { toString() { trace += 'first;'; return 'x'; } },
      { toString() { trace += 'second;'; return 'y'; } },
      abruptSource,
      { toString() { trace += 'late;'; return ''; } }
    ]);
  } catch (error) { print(error === sentinel); }
  print(trace);
  try { Reflect.construct(Constructor, [Symbol()]); }
  catch (error) { print(error instanceof TypeError); }
  trace = '';
  var prepared = Reflect.construct(Constructor, [
    { toString() { trace += 'parameter;'; return 'y'; } },
    { toString() { trace += 'body;'; return 'return y + 1;'; } }
  ]);
  print(trace);
  print(prepared.toString().includes('return y + 1;'));
}
"#,
        &expected,
    );
}

#[test]
fn undefined_source_candidates_preserve_void_operand_effects_and_hoisted_binding_values() {
    assert_function_trace(
        r#"
var calls = 0;
print(Function(void ++calls)());
print(calls);
var generated = Function(hoisted);
var hoisted;
print(generated());
var sentinel = {};
try { Function(void (function () { throw sentinel; })()); }
catch (error) { print(error === sentinel); }
"#,
        &["undefined", "1", "undefined", "true"],
    );
}

#[test]
fn boxed_source_and_literal_coercion_hooks_execute_before_guarded_code_selection() {
    assert_function_trace(
        r#"
var boxed = Object("return 'boxed';");
print(Function(boxed)());
let number = new Number(1);
print(Function(number)());
var calls = 0;
var body = {toString() { ++calls; return "return 'hook';"; }};
print(Function(body)());
print(calls);
try { Function({}); } catch (error) { print(error instanceof SyntaxError); }
Function("return 'changed';");
Object.defineProperty(boxed, 'toString', {get() {
  ++calls;
  return function () { return "return 'changed';"; };
}});
print(Function(boxed)());
print(calls);
"#,
        &["boxed", "undefined", "hook", "1", "true", "changed", "2"],
    );
}

#[test]
fn reflected_foreign_constructor_preserves_argument_source_and_new_target_getter() {
    assert_function_trace(
        r#"
var other = __scriptwCreateRealm().global;
other.marker = 21;
var reads = 0;
var newTarget = function () {}.bind(null);
Object.defineProperty(newTarget, 'prototype', { get() { reads++; return null; } });
var created = Reflect.construct(other.Function, ['return marker;'], newTarget);
print(created());
print(reads);
print(Object.getPrototypeOf(created) === Function.prototype);
"#,
        &["21", "1", "true"],
    );
}

#[test]
fn function_family_construction_reads_prototype_once_after_source_validation() {
    assert_function_trace(
        r#"
var GeneratorFunction = Object.getPrototypeOf(function* () {}).constructor;
var AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function* () {}).constructor;
var reads = 0;
var newTarget = function () {}.bind(null);
Object.defineProperty(newTarget, 'prototype', {get() { ++reads; return null; }});
Reflect.construct(Function, [], newTarget);
print(reads);
Reflect.construct(GeneratorFunction, [], newTarget);
print(reads);
Reflect.construct(AsyncFunction, [], newTarget);
print(reads);
Reflect.construct(AsyncGeneratorFunction, [], newTarget);
print(reads);
try { Reflect.construct(Function, ['a)', 'return 1;'], newTarget); }
catch (error) { print(error instanceof SyntaxError); }
print(reads);
var sentinel = {};
try {
  Reflect.construct(Function, [{toString() { throw sentinel; }}], newTarget);
} catch (error) { print(error === sentinel); }
print(reads);
Function('return 17;');
var trace = '';
var orderedNewTarget = function () {}.bind(null);
Object.defineProperty(orderedNewTarget, 'prototype', {
  get() { trace += 'prototype;'; return null; }
});
var created = Reflect.construct(Function, [{
  toString() { trace += 'source;'; return 'return 17;'; }
}], orderedNewTarget);
print(trace);
print(created());
"#,
        &[
            "1",
            "2",
            "3",
            "4",
            "true",
            "4",
            "true",
            "4",
            "source;prototype;",
            "17",
        ],
    );
}

#[test]
fn derived_constructor_candidates_survive_effects_without_replacing_the_live_alias() {
    assert_function_trace(
        r#"
eval(null);
var Constructor = Object.getPrototypeOf(async function* () {}).constructor;
eval(null);
var created = Constructor('yield 1;');
created().next().then(function (result) { print(result.value); });
Constructor = function (body) { print(body); return 42; };
print(Constructor('yield 2;'));
void 0;
"#,
        &["yield 2;", "42", "1"],
    );
}

#[test]
fn finite_source_tables_preserve_getters_callback_dispatch_and_live_tuple_selection() {
    assert_function_trace(
        r#"
const entries = [{source: '41'}, {source: '42'}];
var reads = 0;
Object.defineProperty(entries[0], 'source', {get() { ++reads; return '42'; }});
entries.forEach(entry => print(Function('return ' + entry.source)()));
print(reads);
entries.forEach = function (callback) { print('replacement'); };
entries.forEach(entry => print(Function('return ' + entry.source)()));
print(reads);
"#,
        &["42", "42", "1", "replacement", "1"],
    );
}

#[test]
fn primitive_wrappers_observe_conversion_hook_order_and_abrupt_completion() {
    assert_function_trace(
        r#"
var wrappers = [Object('text'), Object(1), Object(true)];
var log = '';
for (var wrapper of wrappers) {
  Object.defineProperty(wrapper, 'toString', {get() {
    log += 'get;';
    return function () { log += 'call;'; return {}; };
  }});
  wrapper.valueOf = function () { log += 'value;'; return 'converted'; };
  print(String(wrapper));
}
print(log);
var sentinel = {};
var boxed = Object(1);
Object.defineProperty(boxed, Symbol.toPrimitive, {get() { throw sentinel; }});
try { Function(boxed); } catch (error) { print(error === sentinel); }
"#,
        &[
            "converted",
            "converted",
            "converted",
            "get;call;value;get;call;value;get;call;value;",
            "true",
        ],
    );
}

#[test]
fn captured_body_hints_admit_the_original_eight_bodies_and_reject_strict_variants() {
    let source = include_str!("../fixtures/finite_capture_sources/escaped_strict_bodies.js");
    for prefix in ["", "'use strict';"] {
        assert_function_trace(&format!("{prefix}{source}"), &["16"]);
    }
}

#[test]
fn captured_body_hints_preserve_escaped_shadowed_and_reassigned_cells() {
    let source = r#"
var source = 'return 999;';
function first(source) { var text = source; return () => Function(text)(); }
function second(source) { var text = 'return 22;'; return () => Function(text)(); }
var one = first('return 11;');
var two = second('return 777;');
print(one()); print(two());
function block() {
  var text = 'return 33;';
  var outside = () => Function(text)();
  { let text = 'return 44;'; var inside = () => Function(text)(); }
  return [outside, inside];
}
var blocks = block(); print(blocks[0]()); print(blocks[1]());
function mutable(source) {
  var text = source;
  return [() => Function(text)(), () => { text = 'return 56;'; }];
}
var changed = mutable('return 55;');
print(changed[0]()); changed[1](); print(changed[0]());
"#;
    for prefix in ["", "'use strict';"] {
        assert_function_trace(
            &format!("{prefix}{source}"),
            &["11", "22", "33", "44", "55", "56"],
        );
    }
}

#[test]
fn captured_body_hints_preserve_parameter_records_and_recursive_activations() {
    let source = r#"
function separate(source = 'return 71;', early = () => Function(source)()) {
  var source = 'return 72;';
  return [early, () => Function(source)()];
}
var pair = separate(); print(pair[0]()); print(pair[1]());
function recursive(depth, source) {
  var text = source;
  var read = () => Function(text)();
  if (depth) { var other = recursive(0, 'return 82;'); return [read, other[0]]; }
  return [read];
}
var reads = recursive(1, 'return 81;'); print(reads[0]()); print(reads[1]());
"#;
    for prefix in ["", "'use strict';"] {
        assert_function_trace(&format!("{prefix}{source}"), &["71", "72", "81", "82"]);
    }
}

#[test]
fn captured_body_hints_keep_live_conversion_effects_before_strict_rejection() {
    let source = r#"
function reject() {
  var converted = { toString: function() {
    print('convert'); return '"use strict"; l\\u0065t: 42;';
  } };
  return () => {
    try { Function(converted); }
    catch (error) { if (!(error instanceof SyntaxError)) throw error; print('SyntaxError'); return; }
    throw new Error('missing strict rejection');
  };
}
var run = reject(); run(); run();
"#;
    for prefix in ["", "'use strict';"] {
        assert_function_trace(
            &format!("{prefix}{source}"),
            &["convert", "SyntaxError", "convert", "SyntaxError"],
        );
    }
}

#[test]
fn global_recursive_source_hints_keep_distinct_escaped_activation_cells() {
    let source = r#"
function recursive(depth, source) {
  var text = source; var read = () => Function(text)();
  if (depth) { var next = recursive(0, 'return 82;'); return [read, next[0]]; }
  return [read];
}
var reads = recursive(1, 'return 81;'); print(reads[0]()); print(reads[1]());
"#;
    for directive in ["", "'use strict';"] {
        assert_function_trace(&format!("{directive}{source}"), &["81", "82"]);
    }
}

#[test]
fn mutually_recursive_source_hints_keep_all_live_captured_cells() {
    let source = r#"
function first(depth, source) {
  var text = source; var read = () => Function(text)();
  if (depth) { var next = second(depth - 1, 'return 92;'); return [read, next[0], next[1]]; }
  return [read];
}
function second(depth, source) {
  var text = source; var read = () => Function(text)();
  if (depth) { var next = first(depth - 1, 'return 93;'); return [read, next[0]]; }
  return [read];
}
var reads = first(2, 'return 91;'); print(reads[0]()); print(reads[1]()); print(reads[2]());
"#;
    for directive in ["", "'use strict';"] {
        assert_function_trace(&format!("{directive}{source}"), &["91", "92", "93"]);
    }
}

#[test]
fn record_iteration_retains_all_256_primary_property_sources_and_low_count_conversions() {
    let records = (0..256)
        .map(|index| format!("{{body: 'return {index};'}}"))
        .collect::<Vec<_>>()
        .join(",");
    for directive in ["", "'use strict';"] {
        assert_function_trace(
            &format!(
                r#"
{directive}
function build(record) {{ return () => Function(record.body)(); }}
var index = 0;
for (var value of [{records}]) {{
 var read = build(value);
 if (index === 0 || index === 255) print(read());
 index++;
}}
function indirect() {{ build({{body: 'return 997;'}}); }}
indirect();
print(Function({{toString() {{ return 'return 9;'; }} }})());
var rejected = false;
try {{ Function({{body: 'return 9;'}}); }} catch (error) {{ rejected = error instanceof SyntaxError; }}
print(rejected);
"#
            ),
            &["0", "255", "9", "true"],
        );
    }
}

#[test]
fn overflowing_for_of_sources_reach_the_unmatched_live_tuple_boundary() {
    let literals = (0..257)
        .map(|index| format!("'return {index};'"))
        .collect::<Vec<_>>()
        .join(",");
    for directive in ["", "'use strict';"] {
        for head in ["var body", "let body", "const body"] {
            let source = format!(
                r#"
{directive}
var visits = 0, conversions = 0, trace = '';
for ({head} of [{literals}]) {{
  var index = visits++;
  if (index === 0 || index === 256) {{
    print(Function({{toString() {{ conversions++; trace += index + ';'; return body; }}}})());
  }}
}}
print(visits);
print(conversions);
print(trace);
"#
            );
            assert_unmatched_source_tuples(&source, &[&["return 0;"], &["return 256;"]], true);
            assert_function_trace(&source, &["0", "256", "257", "2", "0;256;"]);
        }
    }
}

#[test]
fn strict_header_rejections_keep_conversion_effects_and_the_constructor_realm() {
    let source = r#"
var conversions = 0;
var foreign = __scriptwCreateRealm().global;
try { Function("function* yield() { 'use strict'; (yield 3) + (yield 4); }"); }
catch (error) { if (error.constructor !== SyntaxError) throw error; }
try { foreign.Function("function* yield() { 'use strict'; (yield 3) + (yield 4); }"); }
catch (error) { if (error.constructor !== foreign.SyntaxError) throw error; }
Function('function* yield() { yield 17; } return yield().next().value;');
var invalid = {
  toString() {
    conversions++;
    print('invalid-conversion');
    return "function* yield() { 'use strict'; (yield 3) + (yield 4); }";
  }
};
try { Function(invalid); print('missing-local-rejection'); }
catch (error) { print(error.constructor === SyntaxError); }
try { foreign.Function(invalid); print('missing-foreign-rejection'); }
catch (error) {
  print(error.constructor === foreign.SyntaxError);
  print(!(error instanceof SyntaxError));
}
var valid = {
  toString() {
    conversions++;
    print('valid-conversion');
    return 'function* yield() { yield 17; } return yield().next().value;';
  }
};
print(Function(valid)());
print(conversions);
"#;
    for caller_directive in ["", "'use strict';"] {
        assert_function_trace(
            &format!("{caller_directive}{source}"),
            &[
                "invalid-conversion",
                "true",
                "invalid-conversion",
                "true",
                "true",
                "valid-conversion",
                "17",
                "3",
            ],
        );
    }
}
