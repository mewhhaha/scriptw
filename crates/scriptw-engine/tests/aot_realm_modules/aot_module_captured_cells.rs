use scriptw_engine::{
    configure_compilation_jobs, configure_wasm_runtime, CompileOptions, EmbeddedEvaluation,
    EmbeddedInstance, EmbeddedRuntime, Engine, ExecutionBackend, ForeignValue, HostOutputEvent,
    JsCompletion, JsValue, ObservedCompletion, RealmBuilder, RunOptions, WasmStoreState,
};

fn assert_output(source: &str, module: bool, expected: &[&str]) {
    configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let options = CompileOptions::default();
    let execution = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    };
    let result = if module {
        engine.observe_module(source, options, execution)
    } else {
        engine.observe_script(source, options, execution)
    };
    let observed = result.expect("captured cells compile and execute through Wasm");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "completion: {:?}; output: {:?}; note: {}",
        observed.completion,
        observed.output_events,
        observed.note
    );
    assert_eq!(
        observed.output_events,
        expected
            .iter()
            .map(|line| HostOutputEvent::PrintLine((*line).into()))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn module_closures_share_mutation_and_keep_nested_and_iteration_cells_distinct() {
    assert_output(
        r#"
function make(seed) {
  let shared = seed;
  const closures = [];
  for (let index = 0; index < 3; ++index) {
    let step = index;
    closures.push({
      read() { return [shared, index, step].join(':'); },
      bump() { ++step; shared += step; }
    });
  }
  return closures;
}
function shadowed() {
  let name = 'outer';
  function middle() {
    let name = 'middle';
    return () => name;
  }
  return [() => name, middle(), next => { name = next; }];
}
const closures = make(10);
const names = shadowed();
print(closures.map(item => item.read()).join('|'));
await 0;
closures[1].bump();
closures[2].bump();
names[2]('changed');
print(closures.map(item => item.read()).join('|'));
print(names[0]() + ',' + names[1]());
"#,
        true,
        &[
            "10:0:0|10:1:1|10:2:2",
            "15:0:0|15:1:2|15:2:3",
            "changed,middle",
        ],
    );
}

#[test]
fn module_captured_tdz_and_const_cells_preserve_abrupt_and_rhs_order() {
    assert_output(
        r#"
function region() {
  const trace = [];
  let read, type, write, fixedRead, fixedWrite;
  {
    read = () => later;
    type = () => typeof later;
    write = () => later = (trace.push('write-rhs'), 9);
    for (const action of [read, type, write]) {
      try { action(); trace.push('missed'); }
      catch (error) { trace.push(error instanceof ReferenceError); }
    }
    let later = 7;
    const fixed = 11;
    fixedRead = () => fixed;
    fixedWrite = () => fixed = (trace.push('const-rhs'), 99);
  }
  try { fixedWrite(); }
  catch (error) { trace.push(error instanceof TypeError); }
  return [read(), type(), fixedRead(), trace.join(',')].join('|');
}
print(region());
"#,
        true,
        &["7|number|11|true,true,write-rhs,true,const-rhs,true"],
    );
}

#[test]
fn module_captured_references_survive_rhs_and_numeric_coercion_reentry() {
    assert_output(
        r#"
function make() {
  let cell = 1;
  let callable = function() { return this; };
  function replace() { cell = 10; return 2; }
  return {
    compound() { return cell += replace(); },
    read() { return cell; },
    prepare() { cell = {valueOf() { cell = 100; return 4; }}; },
    update() { return cell++; },
    call() { return callable(); },
    replaceCall(next) { callable = next; }
  };
}
const subject = make();
print(subject.compound() + ',' + subject.read());
subject.prepare();
print(subject.update() + ',' + subject.read());
print(subject.call() === undefined);
subject.replaceCall(function() { return this === undefined; });
print(subject.call());
"#,
        true,
        &["3,3", "4,5", "true", "true"],
    );
}

#[test]
fn arguments_storage_does_not_change_independent_captured_cells() {
    assert_output(
        r#"
function unrelated(value) {
  ++value;
  return arguments[0];
}
function make(seed) {
  let state = seed;
  return step => { state += step; return state; };
}
function escapedArguments(value) {
  const read = () => arguments[0];
  value = 99;
  return read;
}
const counter = make(4);
const readArguments = escapedArguments(8);
print(unrelated(7) + ',' + counter(2) + ',' + counter(3));
print(readArguments());
"#,
        true,
        &["7,6,9", "8"],
    );
}

#[test]
fn with_object_fallback_rechecks_live_bindings_and_call_receiver() {
    assert_output(
        r#"
const scope = {value: 3, fn() { return this === scope; }};
function make() {
  let value = 1;
  with (scope) {
    return {read() { return value; }, write(next) { value = next; }, call() { return fn(); }};
  }
}
const subject = make();
const results = [subject.read()];
delete scope.value;
results.push(subject.read());
subject.write(5);
results.push(subject.read());
scope.value = 9;
results.push(subject.read());
scope[Symbol.unscopables] = {value: true};
results.push(subject.read());
scope[Symbol.unscopables].value = false;
results.push(subject.read(), subject.call());
print(results.join(','));
"#,
        false,
        &["3,1,5,9,5,9,true"],
    );
}

fn normal(completion: JsCompletion) -> JsValue {
    match completion {
        JsCompletion::Normal(value) => value,
        JsCompletion::Throw(value) => panic!("captured-cell operation threw: {value:?}"),
    }
}

fn fresh_instance(
    native: &wasmtime::Engine,
    runtime: &EmbeddedRuntime,
) -> (wasmtime::Store<WasmStoreState>, EmbeddedInstance) {
    let state = runtime.new_state(RealmBuilder::new().build(), ()).unwrap();
    let mut store = wasmtime::Store::new(native, state);
    store.limiter(|state| state.limits_mut());
    store.set_epoch_deadline(u64::MAX / 2);
    let mut linker = wasmtime::Linker::new(native);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    let EmbeddedEvaluation::Settled(completion) = instance.initialize(&mut store).unwrap() else {
        panic!("synchronous captured-cell module unexpectedly suspended")
    };
    normal(completion);
    (store, instance)
}

fn bump(
    store: &mut wasmtime::Store<WasmStoreState>,
    instance: &EmbeddedInstance,
    amount: f64,
) -> f64 {
    let function = normal(instance.get_export(store, "bump").unwrap());
    let argument = instance.value(store, ForeignValue::Number(amount)).unwrap();
    normal(instance.call(store, &function, None, &[argument]).unwrap())
        .as_number()
        .unwrap()
}

#[test]
fn portable_module_reuse_keeps_captured_cells_and_module_roots_per_instance() {
    configure_compilation_jobs(1).unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(
            r#"
let moduleCalls = 0;
function make(seed) {
  let value = seed;
  return amount => { value += amount; ++moduleCalls; return value * 100 + moduleCalls; };
}
const counter = make(10);
export function bump(amount) { return counter(amount); }
"#,
            CompileOptions::default(),
        )
        .unwrap();
    let artifact = compiler.emit_wasm(&unit).unwrap();
    let mut config = wasmtime::Config::new();
    configure_wasm_runtime(&mut config);
    let native = wasmtime::Engine::new(&config).unwrap();
    // EmbeddedRuntime admission requires the real Portable compilation profile.
    let runtime = EmbeddedRuntime::compile(&native, &artifact).unwrap();
    let (mut first_store, first) = fresh_instance(&native, &runtime);
    let (mut second_store, second) = fresh_instance(&native, &runtime);
    first_store.gc(None).unwrap();
    second_store.gc(None).unwrap();
    assert_eq!(bump(&mut first_store, &first, 2.0), 1201.0);
    assert_eq!(bump(&mut second_store, &second, 1.0), 1101.0);
    first_store.gc(None).unwrap();
    assert_eq!(bump(&mut first_store, &first, 3.0), 1502.0);
    assert_eq!(bump(&mut second_store, &second, -1.0), 1002.0);
    assert!(first.take_output(&mut first_store).unwrap().is_empty());
    assert!(second.take_output(&mut second_store).unwrap().is_empty());
}

#[test]
fn current_owner_cells_keep_reentrant_values_and_callee_argument_order() {
    assert_output(
        r#"
function unrelated() { return arguments.length; }
function subject(value, callback) {
  var cell = 5;
  var trace = [];
  function replaceValue() { value = 40; cell = 80; return 2; }
  function replaceCell() { cell = 100; return 2; }
  function changeType() { value = {}; }
  function replacement(input) { trace.push('new'); return 'new:' + input + ':' + (this === undefined); }
  function argument() { trace.push('arg'); callback = replacement; return 'payload'; }
  var sum = value += replaceValue();
  var product = cell *= replaceCell();
  changeType();
  var kind = typeof value;
  var first = callback(argument());
  var second = callback('tail');
  return [sum, product, kind, first, second, trace.join(',')].join('|');
}
print(subject(1, function(input) { return 'old:' + input + ':' + (this === undefined); }));
print(unrelated(1, 2));
"#,
        true,
        &["3|160|object|old:payload:true|new:tail:true|arg,new", "2"],
    );
}

#[test]
fn current_owner_block_cells_keep_tdz_const_and_shadow_boundaries() {
    assert_output(
        r#"
function inspect(seed) {
  var trace = [typeof hoisted];
  var hoisted = seed;
  var readHoisted = () => hoisted;
  {
    try { typeof local; } catch (error) { trace.push(error instanceof ReferenceError); }
    try { local; } catch (error) { trace.push(error instanceof ReferenceError); }
    let local = seed * 2;
    const fixed = 17;
    var readLocal = () => local;
    var readFixed = () => fixed;
    trace.push(typeof local, local);
    {
      let local = 'inner';
      trace.push(local, readLocal());
    }
    try { fixed = (trace.push('const-rhs'), 99); }
    catch (error) { trace.push(error instanceof TypeError); }
    local += (hoisted = 9, 1);
    trace.push(local, readLocal(), readFixed());
  }
  trace.push(hoisted, readHoisted());
  return trace.join(',');
}
print(inspect(3));
"#,
        true,
        &["undefined,true,true,number,6,inner,6,const-rhs,true,7,7,17,9,9"],
    );
}

#[test]
fn react_like_simple_factories_retry_with_live_owned_parameters_and_vars() {
    assert_output(
        r#"
function strictArguments(value) {
  var read = () => arguments[0];
  value = 99;
  return [value, arguments[0], read()].join(',');
}
function lazy(factory, cached) {
  return function load() {
    try {
      if (!cached) {
        cached = {exports: {}};
        factory(cached.exports, cached);
      }
      return cached.exports;
    } catch (error) {
      cached = 0;
      throw error;
    }
  };
}
var attempts = 0;
var marker = {};
var load = lazy(function(exports, module) {
  ++attempts;
  var value = attempts;
  function write(next) { value = next; exports.note = next; module.tag = next; }
  if (value === 1) { write(3); throw marker; }
  exports.count = value;
  write(7);
  module.exports = {count: exports.count, live: value, note: exports.note, tag: module.tag,
    receiver: this === undefined};
});
try { load(); } catch (error) { print(error === marker); }
var first = load();
var second = load();
print([attempts, first === second, first.count, first.live, first.note, first.tag, first.receiver].join(','));
print(strictArguments(8));
"#,
        true,
        &["true", "2,true,2,7,7,7,true", "99,8,8"],
    );
}

#[test]
fn packed_utf16_keeps_nested_function_sources_and_exact_units_across_instances() {
    const INNER_SOURCE: &str =
        r#"function inner(x) { /* café 😀 */ return x + "\u0000\u0001\u0100\ud800"; }"#;
    const EXPORTS: &str = r#"
const nested = outer("Āā");
export function outerSource() { return outer.toString(); }
export function innerSource() { return nested.inner.toString(); }
export function payload() { return nested.value; }
export function callInner(input) { return nested.inner(input); }
export function boundaries() {
  return ["\u0100\u0001", "\u0001\u0100", "\u0000\u0100", "\u0100\u0000",
    "Āā", "\u0100", "\u0001", "\u0000"].join("|");
}
"#;
    let outer_source = format!(
        "function outer(Ā) {{ /* outer 😀 */\n  {INNER_SOURCE}\n  return {{inner, value: inner(Ā)}};\n}}"
    );
    let source = format!("{outer_source}\n{EXPORTS}");
    configure_compilation_jobs(1).unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(&source, CompileOptions::default())
        .unwrap();
    let artifact = compiler.emit_wasm(&unit).unwrap();
    let mut config = wasmtime::Config::new();
    configure_wasm_runtime(&mut config);
    let native = wasmtime::Engine::new(&config).unwrap();
    let runtime = EmbeddedRuntime::compile(&native, &artifact).unwrap();
    for _ in 0..2 {
        let (mut store, instance) = fresh_instance(&native, &runtime);
        store.gc(None).unwrap();
        let read =
            |store: &mut wasmtime::Store<WasmStoreState>, name: &str, arguments: &[JsValue]| {
                let function = normal(instance.get_export(store, name).unwrap());
                normal(instance.call(store, &function, None, arguments).unwrap())
                    .to_utf16(store)
                    .unwrap()
            };
        assert_eq!(
            read(&mut store, "outerSource", &[]),
            outer_source.encode_utf16().collect::<Vec<_>>()
        );
        assert_eq!(
            read(&mut store, "innerSource", &[]),
            INNER_SOURCE.encode_utf16().collect::<Vec<_>>()
        );
        assert_eq!(
            read(&mut store, "payload", &[]),
            [0x0100, 0x0101, 0, 1, 0x0100, 0xd800]
        );
        assert_eq!(
            read(&mut store, "boundaries", &[]),
            "\u{0100}\u{0001}|\u{0001}\u{0100}|\0\u{0100}|\u{0100}\0|Āā|\u{0100}|\u{0001}|\0"
                .encode_utf16()
                .collect::<Vec<_>>()
        );
        let input = instance
            .value(&mut store, ForeignValue::StringUtf16(vec![0x0101, 0]))
            .unwrap();
        assert_eq!(
            read(&mut store, "callInner", &[input]),
            [0x0101, 0, 0, 1, 0x0100, 0xd800]
        );
        assert!(instance.take_output(&mut store).unwrap().is_empty());
    }
}

#[test]
fn private_aliases_observe_callback_shape_changes_and_keep_callee_snapshots() {
    assert_output(
        r#"
function elsewhere() { return arguments.length; }
function holder(initial) {
  let current = initial;
  return {read() { return current; }, replace(next) { current = next; }};
}
function work(mutate, callbacks, next) {
  var object = {value: 1, keep: true, method() { return 'old'; }};
  var alias = object;
  var before = object.value;
  var stable = function(input) { return 'stable:' + input + ':' + (this === undefined); };
  var stableResult = stable((mutate(alias), 'value'));
  var kind = typeof object.value;
  var tag = object.value.tag;
  var method = object.method();
  var selected = callbacks.read();
  var first = selected((callbacks.replace(next), 'arg'));
  var second = callbacks.read()('tail');
  return [before, kind, tag, typeof object.keep, method, stableResult, first, second].join('|');
}
var trace = [];
var callbacks = holder(function(input) {
  trace.push('old:' + input);
  return 'old:' + input + ':' + (this === undefined);
});
print(work(function(object) {
  trace.push('mutate');
  Object.defineProperty(object, 'value', {configurable: true, get() {
    trace.push('get'); return {tag: 'after'};
  }});
  delete object.keep;
  object.method = function() { trace.push('method'); return this === object; };
}, callbacks, function(input) {
  trace.push('new:' + input);
  return 'new:' + input + ':' + (this === undefined);
}));
print(trace.join(','));
print(elsewhere(1, 2));
"#,
        true,
        &[
            "1|object|after|undefined|true|stable:value:true|old:arg:true|new:tail:true",
            "mutate,get,get,method,old:arg,new:tail",
            "2",
        ],
    );
}

#[test]
fn strict_arguments_keep_snapshots_object_aliases_and_escaped_activation_identity() {
    assert_output(
        r#"
function snapshot(value, extra) {
  var original = value;
  value = {tag: 'private'};
  var args = arguments;
  args[0].tag += ':args';
  var trace = [value.tag, original.tag, args[0] === original, value === args[0], extra, args[1]];
  extra = 'changed';
  args[1] = 'written';
  trace.push(extra, args[1], typeof value);
  return {trace: trace.join('|'), escaped: () => arguments};
}
var a = {tag: 'A'}, b = {tag: 'B'};
var first = snapshot(a, 'one');
var second = snapshot(b, 'two');
print(first.trace);
print(second.trace);
var firstArgs = first.escaped();
var secondArgs = second.escaped();
firstArgs[0].tag += ':later';
firstArgs[1] = 'later';
print([firstArgs !== secondArgs, firstArgs[0] === a, secondArgs[0] === b,
  a.tag, b.tag, firstArgs[1], secondArgs[1], firstArgs.length, secondArgs.length].join('|'));
"#,
        true,
        &[
            "private|A:args|true|false|one|one|changed|written|object",
            "private|B:args|true|false|two|two|changed|written|object",
            "true|true|true|A:args:later|B:args|later|written|2|2",
        ],
    );
}

#[test]
fn module_pruned_synthetic_cells_preserve_call_construct_and_subclass_entry_values() {
    assert_output(
        r#"
function context() { return [this, new.target]; }
var receiver = {tag: 'receiver'};
var called = context.call(receiver);
var plain = context();
var constructed = new context();
class Sub extends context {}
var subclassed = new Sub();
print(called[0] === receiver && called[1] === undefined);
print(plain[0] === undefined && plain[1] === undefined);
print(Object.getPrototypeOf(constructed[0]) === context.prototype && constructed[1] === context);
print(Object.getPrototypeOf(subclassed[0]) === Sub.prototype && subclassed[1] === Sub);
"#,
        true,
        &["true", "true", "true", "true"],
    );
}

#[test]
fn module_pruned_middle_activations_keep_ancestor_and_arrow_context_captures() {
    assert_output(
        r#"
function make(value) {
  var owner = this;
  function middle() { return () => [owner, value]; }
  return middle;
}
function context(value) { return () => [this, new.target, arguments[0], value]; }
var receiver = {};
var middle = make.call(receiver, 7);
var read = middle();
var captured = context.call(receiver, 11);
var constructed = new context(13);
await 0;
var values = read(), called = captured(), created = constructed();
print(values[0] === receiver && values[1] === 7);
print(called[0] === receiver && called[1] === undefined && called[2] === 11 && called[3] === 11);
print(Object.getPrototypeOf(created[0]) === context.prototype && created[1] === context && created[2] === 13 && created[3] === 13);
"#,
        true,
        &["true", "true", "true"],
    );
}

#[test]
fn module_arguments_observers_keep_shorthand_and_unrelated_child_arguments() {
    assert_output(
        r#"
function observed(value) { return {arguments}; }
function child(value) { return function() { return arguments[0]; }; }
function snapshot(value) { value = 99; return arguments[0]; }
print(observed(7).arguments[0]);
print(child(11)(13));
print(snapshot(17));
"#,
        true,
        &["7", "13", "17"],
    );
}

#[test]
fn root_module_cells_keep_live_operations_and_saved_callees() {
    assert_output(
        r#"
var state = 1;
let callee = original;
const fixed = 7;
const trace = [];
function original(value) { return 'old:' + value + ':' + (this === undefined); }
function replacement(value) { return 'new:' + value + ':' + (this === undefined); }
function argument() { trace.push('arg'); callee = replacement; return 2; }
function read() { return state; }
function compound() { return state += (state = 40, 2); }
function update() {
  state = {valueOf() { state = 99; return 4; }};
  return state++;
}
function logical() { state = 0; return state ||= (trace.push('logical'), 8); }
function kind() { return typeof state; }
function call() { return callee(argument()); }
function next() { return callee(3); }
function readonly() { fixed = (trace.push('const-rhs'), 9); }
function deep(a, b, c, d, e) {
  function middle() { return function readDeep() { return state + a + b + c + d + e; }; }
  return middle();
}
print(compound());
print(update() + ',' + read());
print(logical() + ',' + kind() + ',' + deep(1, 2, 3, 4, 5)());
print(call());
print(next());
try { readonly(); } catch (error) { print((error instanceof TypeError) + ',' + fixed); }
print(trace.join(','));
"#,
        true,
        &[
            "3",
            "4,5",
            "8,number,23",
            "old:2:true",
            "new:3:true",
            "true,7",
            "logical,arg,const-rhs",
        ],
    );
}

#[test]
fn root_module_tdz_and_module_block_cells_keep_distinct_identities() {
    assert_output(
        r#"
var trace = [];
function read() { return later; }
function kind() { return typeof later; }
function write() { return later = (trace.push('write-rhs'), 9); }
for (const action of [read, kind, write]) {
  try { action(); trace.push('missed'); }
  catch (error) { trace.push(error instanceof ReferenceError); }
}
let later = 7;
const blocks = [], iterations = [];
{
  let later = 'block';
  blocks.push(function readBlock() { return later; });
}
for (let index = 0; index < 2; index++) {
  iterations.push(function readIteration() { return index + ':' + later; });
}
function shadow() {
  let later = 'local';
  return function readLocal() { return later; };
}
print(trace.join(',') + '|' + read() + '|' + kind() + '|' + shadow()() + '|' +
  blocks[0]() + '|' + iterations.map(function(value) { return value(); }).join(','));
"#,
        true,
        &["true,true,write-rhs,true|7|number|local|block|0:7,1:7"],
    );
}
