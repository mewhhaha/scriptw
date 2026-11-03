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
