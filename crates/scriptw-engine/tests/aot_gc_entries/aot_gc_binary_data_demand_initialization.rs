use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

fn compile_options() -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    }
}

fn run_options() -> RunOptions {
    RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(120_000),
        ..RunOptions::default()
    }
}

fn assert_modes(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{source}");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(&source, compile_options(), run_options())
            .unwrap_or_else(|error| panic!("binary publication control: {error}\n{source}"));
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0))),
            "{source}",
        );
        assert_eq!(
            observed.output_events,
            [HostOutputEvent::PrintLine("ok".into())],
            "{source}",
        );
    }
}

#[test]
fn first_binary_descriptor_publishes_complete_realm_owned_constructor_graphs() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var names = ['ArrayBuffer','SharedArrayBuffer','DataView','Int8Array','Uint8Array','Uint8ClampedArray',
  'Int16Array','Uint16Array','Int32Array','Uint32Array','Float16Array','Float32Array','Float64Array',
  'BigInt64Array','BigUint64Array'];
var widths = [1,1,1,2,2,4,4,2,4,8,8,8], retained = [];
for (var global of [globalThis, __scriptwCreateRealm().global, __scriptwCreateRealm().global]) {
  var before = Reflect.ownKeys(global);
  for (var name of names) check(before.indexOf(name) >= 0 && Reflect.has(global, name) &&
    Object.hasOwn(global, name) && !Object.prototype.propertyIsEnumerable.call(global, name), name + ' membership');
  var descriptor = Object.getOwnPropertyDescriptor(global, 'DataView');
  check(typeof descriptor.value === 'function' && descriptor.value === global.DataView &&
    !Object.hasOwn(descriptor, 'get') && descriptor.writable && !descriptor.enumerable &&
    descriptor.configurable, 'first descriptor is an ordinary data descriptor');
  var abstract = Object.getPrototypeOf(global.Uint8Array), abstractPrototype = abstract.prototype;
  check(abstract.name === 'TypedArray' && abstract.length === 0 &&
    Object.getPrototypeOf(abstract) === global.Function.prototype &&
    Object.getPrototypeOf(abstractPrototype) === global.Object.prototype &&
    abstractPrototype.constructor === abstract, 'abstract TypedArray graph');
  var abstractKeys = Reflect.ownKeys(abstract);
  check(abstractKeys.slice(0, -1).join('|') === 'length|name|prototype|from|of' &&
    abstractKeys[abstractKeys.length - 1] === Symbol.species,
    'abstract constructor own key order');
  var species = Object.getOwnPropertyDescriptor(abstract, Symbol.species);
  check(species.set === undefined && !species.enumerable && species.configurable &&
    species.get.call(global.Uint8Array) === global.Uint8Array &&
    Object.getPrototypeOf(species.get) === global.Function.prototype, 'species accessor');
  for (var i = 0; i < names.length; ++i) {
    var name = names[i], property = Object.getOwnPropertyDescriptor(global, name), C = property.value;
    var prototype = C.prototype, ownPrototype = Object.getOwnPropertyDescriptor(C, 'prototype');
    check(typeof C === 'function' && C === global[name] && property.writable && !property.enumerable &&
      property.configurable && !Object.hasOwn(property, 'get'), name + ' public data property');
    check(C.name === name && C.length === (name === 'DataView' ? 1 : i < 2 ? 1 : 3) &&
      prototype.constructor === C && !ownPrototype.writable && !ownPrototype.enumerable &&
      !ownPrototype.configurable, name + ' constructor metadata and cycle');
    check(Object.getPrototypeOf(C) === (i < 3 ? global.Function.prototype : abstract) &&
      Object.getPrototypeOf(prototype) === (i < 3 ? global.Object.prototype : abstractPrototype),
      name + ' constructor and prototype inheritance');
    for (var key of ['name','length']) {
      var metadata = Object.getOwnPropertyDescriptor(C, key);
      check(!metadata.writable && !metadata.enumerable && metadata.configurable, name + ' metadata attributes');
    }
    if (i >= 3) {
      check(Reflect.ownKeys(C).slice(0, 4).join(',') === 'length,name,prototype,BYTES_PER_ELEMENT' &&
        Reflect.ownKeys(prototype).slice(0, 2).join(',') === 'constructor,BYTES_PER_ELEMENT', name + ' own key order');
      for (var holder of [C, prototype]) {
        var size = Object.getOwnPropertyDescriptor(holder, 'BYTES_PER_ELEMENT');
        check(size.value === widths[i - 3] && !size.writable && !size.enumerable && !size.configurable,
          name + ' immutable element size');
      }
    }
    for (var key of Reflect.ownKeys(prototype)) {
      var member = Object.getOwnPropertyDescriptor(prototype, key);
      for (var callable of [member.value, member.get, member.set]) {
        if (typeof callable === 'function' && callable !== C) check(
          Object.getPrototypeOf(callable) === global.Function.prototype && !member.enumerable && member.configurable,
          name + ' callable defining Realm');
      }
    }
  }
  check(Reflect.ownKeys(global).join('|') === before.join('|'), 'global insertion order survives publication');
  check(abstractPrototype.toString === global.Array.prototype.toString &&
    abstractPrototype.values === abstractPrototype[Symbol.iterator], 'shared aliases');
  retained.push(global.Uint8Array);
}
check(retained[0] !== retained[1] && retained[1] !== retained[2] &&
  retained[0].prototype !== retained[1].prototype, 'fresh Realm identities');
print('ok');
262;
"#,
    );
}

#[test]
fn array_to_string_retains_its_original_binary_alias_after_prepublication_mutation() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
for (var mode of ['delete','assign','define']) {
  var global = __scriptwCreateRealm().global, arrayPrototype = global.Array.prototype;
  var original = arrayPrototype.toString, marker = function replaced() { return 'replacement'; };
  original.probe = mode;
  check(original.call([1,2]) === '1,2', 'eager callable before binary first use');
  if (mode === 'delete') check(Reflect.deleteProperty(arrayPrototype, 'toString'), 'delete array alias');
  else if (mode === 'assign') arrayPrototype.toString = marker;
  else Object.defineProperty(arrayPrototype, 'toString', {value: marker, writable: false, configurable: false});
  gc();
  var C = global.Uint8Array, shared = Object.getPrototypeOf(C.prototype).toString;
  check(shared === original && shared.probe === mode && shared.call(new C([3,4])) === '3,4',
    'binary graph captures original shared callable');
  check((mode === 'delete' ? !Object.hasOwn(arrayPrototype, 'toString') : arrayPrototype.toString === marker),
    'publication preserves array alias mutation');
  var descriptor = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(C.prototype), 'toString');
  check(descriptor.value === original && descriptor.writable && !descriptor.enumerable && descriptor.configurable,
    'typed array alias descriptor');
}
print('ok');
262;
"#,
    );
}

#[test]
fn private_foreign_default_prototypes_survive_all_public_binary_constructor_replacements() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var names = ['ArrayBuffer','SharedArrayBuffer','DataView','Int8Array','Uint8Array','Uint8ClampedArray',
  'Int16Array','Uint16Array','Int32Array','Uint32Array','Float16Array','Float32Array','Float64Array',
  'BigInt64Array','BigUint64Array'];
for (var mode of ['delete','assign','define','accessor']) {
  var global = __scriptwCreateRealm().global, target = global.Array.bind(null), marker = {}, gets = 0, sets = 0;
  Object.defineProperty(target, 'prototype', {value: undefined});
  for (var name of names) {
    if (mode === 'delete') check(Reflect.deleteProperty(global, name), 'delete unread constructor');
    else if (mode === 'assign') global[name] = marker;
    else if (mode === 'define') Object.defineProperty(global, name, {value: marker, writable: false, configurable: false});
    else Object.defineProperty(global, name, {get: function () { gets++; return marker; },
      set: function () { sets++; }, configurable: true});
  }
  var results = [], buffer = new ArrayBuffer(16), abstract;
  for (var i = 0; i < names.length; ++i) {
    var name = names[i], args = name === 'DataView' ? [buffer] : [2];
    var instance = Reflect.construct(globalThis[name], args, target), prototype = Object.getPrototypeOf(instance);
    var C = prototype.constructor;
    if (i === 3) abstract = Object.getPrototypeOf(C);
    check(C.name === name && C.prototype === prototype && C !== globalThis[name] &&
      Object.getPrototypeOf(C) === (i < 3 ? global.Function.prototype : abstract) &&
      (i < 3 || Object.getPrototypeOf(abstract) === global.Function.prototype),
      name + ' private canonical default prototype');
    if (i >= 3) {
      var value = i >= 13 ? 7n : 7;
      instance[0] = value;
      check(instance.length === 2 && instance[0] === value &&
        Object.getPrototypeOf(instance.buffer) === ArrayBuffer.prototype, name + ' active constructor backing buffer');
      var allocated = new C(1);
      check(Object.getPrototypeOf(allocated) === prototype && Object.getPrototypeOf(allocated.buffer) === results[0],
        name + ' private foreign constructor backing buffer');
    }
    results.push(prototype);
  }
  check(gets === 0 && sets === 0, 'private initialization bypasses public getters and setters');
  for (var name of names) {
    var descriptor = Object.getOwnPropertyDescriptor(global, name);
    if (mode === 'delete') check(descriptor === undefined && !Object.hasOwn(global, name), 'deleted stays deleted');
    else if (mode === 'accessor') check(typeof descriptor.get === 'function' && descriptor.set !== undefined,
      'accessor remains installed');
    else check(descriptor.value === marker && descriptor.writable === (mode === 'assign'), 'replacement remains installed');
  }
  gc();
  check(results[4].constructor.from([8,9]).join(',') === '8,9' &&
    results[13].constructor.of(8n,9n).join(',') === '8,9', 'retained private graph remains callable');
}
print('ok');
262;
"#,
    );
}

#[test]
fn locked_binary_globals_and_poisoned_inherited_properties_preserve_first_publication() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
for (var mode of ['attributes','seal','freeze']) {
  var global = __scriptwCreateRealm().global;
  if (mode === 'attributes') Object.defineProperty(global, 'Uint8Array', {writable: false, configurable: false});
  else Object[mode](global);
  var descriptor = Object.getOwnPropertyDescriptor(global, 'Uint8Array'), C = descriptor.value;
  check(typeof C === 'function' && !Object.hasOwn(descriptor, 'get') && !descriptor.enumerable &&
    !descriptor.configurable && descriptor.writable === (mode === 'seal'), mode + ' locked data descriptor');
  check(!Reflect.deleteProperty(global, 'Uint8Array') && Reflect.defineProperty(global, 'Uint8Array', {value: C}),
    mode + ' locked SameValue');
  check(new C([5]).join(',') === '5' && C.prototype.constructor === C, mode + ' complete constructor');
}
var global = __scriptwCreateRealm().global, hooks = 0;
var objectPrototype = global.Object.prototype, functionPrototype = global.Function.prototype;
function forbidden() { hooks++; throw new Error('publication invoked user code'); }
Object.defineProperty(global, 'Object', {get: forbidden, configurable: true});
Object.defineProperty(global, 'Function', {get: forbidden, configurable: true});
for (var key of ['constructor','buffer','byteLength','BYTES_PER_ELEMENT','join','toString'])
  Object.defineProperty(objectPrototype, key, {get: forbidden, set: forbidden, configurable: true});
for (var key of ['name','length','prototype'])
  Object.defineProperty(functionPrototype, key, {get: forbidden, set: forbidden, configurable: true});
Object.defineProperty(objectPrototype, Symbol.toStringTag, {get: forbidden, set: forbidden, configurable: true});
var C = Object.getOwnPropertyDescriptor(global, 'Float16Array').value, value = new C([1.5]);
check(value[0] === 1.5 && C.prototype.constructor === C && C.BYTES_PER_ELEMENT === 2 &&
  Object.getPrototypeOf(Object.getPrototypeOf(C)) === functionPrototype && hooks === 0,
  'own publication bypasses inherited hooks');
print('ok');
262;
"#,
    );
}

#[test]
fn retained_and_prepared_programs_share_binary_species_iterators_and_canonical_backing_buffers() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var realm = __scriptwCreateRealm(), global = realm.global, evaluate = global.eval;
var source = "var saved = Uint8Array; [saved,saved.prototype,Object.getPrototypeOf(saved).prototype];";
var hook = new Proxy(function () {}, {}); hook();
var first = evaluate(source + '/*' + Math.random() + '*/');
var C = first[0], prototype = first[1], abstractPrototype = first[2], marker = {};
C.probe = marker; Object.defineProperty(C, 'name', {value: 'RetainedBytes'});
global.Uint8Array = marker; delete global.ArrayBuffer; delete global.DataView;
gc();
var second = evaluate("[saved,saved.prototype,Object.getPrototypeOf(saved).prototype,saved.probe,saved.name,Uint8Array];");
check(second[0] === C && second[1] === prototype && second[2] === abstractPrototype &&
  second[3] === marker && second[4] === 'RetainedBytes' && second[5] === marker, 'later program identity and mutation');
var value = new C([1,2,3]), bufferPrototype = Object.getPrototypeOf(value.buffer);
check(bufferPrototype.constructor.name === 'ArrayBuffer' &&
  Object.getPrototypeOf(bufferPrototype) === global.Object.prototype && !Object.hasOwn(global, 'ArrayBuffer'),
  'private backing buffer ignores deleted public constructor');
var calls = 0;
value.constructor = {[Symbol.species]: function (length) { calls++; return new C(length); }};
var mapped = value.map(function (x) { return x + 1; });
check(calls === 1 && Object.getPrototypeOf(mapped) === prototype && mapped.join(',') === '2,3,4' &&
  Object.getPrototypeOf(mapped.buffer) === bufferPrototype, 'species keeps canonical owners');
var iterator = mapped.values(), next = iterator.next();
check(Object.getPrototypeOf(next) === global.Object.prototype && next.value === 2 && !next.done &&
  abstractPrototype.values === abstractPrototype[Symbol.iterator], 'iterator result defining Realm');
var transferred = value.buffer.transfer();
check(value.length === 0 && Object.getPrototypeOf(transferred) === bufferPrototype &&
  new C(transferred).join(',') === '1,2,3', 'transfer private canonical allocation');
gc();
check(iterator.next().value === 3 && global.Uint8Array === marker && !Object.hasOwn(global, 'DataView'),
  'retained entries after GC preserve public mutations');
print('ok');
262;
"#,
    );
}

#[test]
fn portable_foreign_bytes_and_retained_get_initialize_independent_realms_after_gc() {
    use scriptw_engine::{
        EmbeddedEvaluation, EmbeddedRuntime, ForeignValue, JsCompletion, JsValue,
    };

    fn normal(completion: JsCompletion) -> JsValue {
        match completion {
            JsCompletion::Normal(value) => value,
            JsCompletion::Throw(value) => panic!("retained binary operation threw: {value:?}"),
        }
    }

    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(
            r#"
export const foreign = __scriptwCreateRealm().global;
const names = ['ArrayBuffer','SharedArrayBuffer','DataView','Int8Array','Uint8Array','Uint8ClampedArray',
  'Int16Array','Uint16Array','Int32Array','Uint32Array','Float16Array','Float32Array','Float64Array',
  'BigInt64Array','BigUint64Array'];
for (const name of names) Reflect.deleteProperty(globalThis, name);
let firstViewPrototype, firstBufferPrototype;
export function inspect(bytes) {
  const prototype = Object.getPrototypeOf(bytes), bufferPrototype = Object.getPrototypeOf(bytes.buffer);
  if (firstViewPrototype === undefined) { firstViewPrototype = prototype; firstBufferPrototype = bufferPrototype; }
  return bytes.length === 3 && bytes[0] === 0 && bytes[1] === 128 && bytes[2] === 255 &&
    prototype === firstViewPrototype && bufferPrototype === firstBufferPrototype &&
    prototype.constructor.name === 'Uint8Array' && bufferPrototype.constructor.name === 'ArrayBuffer' &&
    Object.getPrototypeOf(bufferPrototype) === Object.prototype &&
    Object.getPrototypeOf(prototype.constructor) === Object.getPrototypeOf(prototype).constructor &&
    names.every(function (name) { return !Object.hasOwn(globalThis, name); });
}
export function inspectForeign(C) {
  const typed = Object.getPrototypeOf(C), prototype = typed.prototype;
  return C === foreign.Uint16Array && C !== firstViewPrototype.constructor &&
    C.prototype.constructor === C && typed === Object.getPrototypeOf(foreign.Float16Array) &&
    Object.getPrototypeOf(typed) === foreign.Function.prototype &&
    Object.getPrototypeOf(prototype) === foreign.Object.prototype &&
    prototype.toString === foreign.Array.prototype.toString && new C([513])[0] === 513;
}
"#,
            compile_options(),
        )
        .unwrap();
    let artifact = compiler.emit_wasm(&unit).unwrap();
    let mut config = wasmtime::Config::new();
    scriptw_engine::configure_wasm_runtime(&mut config);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let runtime = EmbeddedRuntime::compile(&engine, &artifact).unwrap();
    // Reuse the same Portable runtime image in two stores. Each store must
    // begin with independent dormant Realms and preserve its own ready graph.
    for _ in 0..2 {
        let state = runtime.new_state(RealmBuilder::new().build(), ()).unwrap();
        let mut store = wasmtime::Store::new(&engine, state);
        store.limiter(|state| state.limits_mut());
        store.set_epoch_deadline(u64::MAX / 2);
        let mut linker = wasmtime::Linker::new(&engine);
        runtime.register(&mut store, &mut linker).unwrap();
        let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
        assert!(matches!(
            instance.initialize(&mut store).unwrap(),
            EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
        ));
        let inspect = normal(instance.get_export(&mut store, "inspect").unwrap());
        let inspect_foreign = normal(instance.get_export(&mut store, "inspectForeign").unwrap());
        let foreign = normal(instance.get_export(&mut store, "foreign").unwrap());
        store.gc(None).unwrap();
        for _ in 0..2 {
            let bytes = instance
                .value(&mut store, ForeignValue::Bytes(vec![0, 128, 255]))
                .unwrap();
            store.gc(None).unwrap();
            assert_eq!(
                instance.to_bytes(&mut store, &bytes).unwrap(),
                [0, 128, 255]
            );
            assert_eq!(
                normal(instance.call(&mut store, &inspect, None, &[bytes]).unwrap()).as_boolean(),
                Some(true),
                "foreign Bytes allocation keeps canonical prototypes and deleted public globals",
            );
        }
        let constructor = normal(
            instance
                .get_property(
                    &mut store,
                    &foreign,
                    &"Uint16Array".encode_utf16().collect::<Vec<_>>(),
                )
                .unwrap(),
        );
        store.gc(None).unwrap();
        assert_eq!(
            normal(
                instance
                    .call(&mut store, &inspect_foreign, None, &[constructor])
                    .unwrap()
            )
            .as_boolean(),
            Some(true),
            "retained host Get publishes a separate complete foreign graph",
        );
        assert!(instance.take_output(&mut store).unwrap().is_empty());
    }
}

#[test]
fn rooted_snapshot_resolves_unread_binary_data_and_original_shared_to_string_alias() {
    use scriptw_engine::{SnapshotLimits, SnapshotOutcome};
    use scriptw_runtime::rooted_snapshot::{
        SnapshotDescriptor, SnapshotKey, SnapshotNode, SnapshotValue,
    };

    fn property<'a>(node: &'a SnapshotNode, name: &str) -> &'a SnapshotDescriptor {
        let units = name.encode_utf16().collect::<Vec<_>>();
        &node
            .properties
            .iter()
            .find(|property| {
                matches!(&property.key, SnapshotKey::String { units: key } if key.units() == units)
            })
            .unwrap_or_else(|| panic!("snapshot missing {name}"))
            .descriptor
    }

    fn object_id(descriptor: &SnapshotDescriptor) -> u32 {
        match descriptor {
            SnapshotDescriptor::Data {
                value: SnapshotValue::Object { id },
                ..
            } => *id,
            other => panic!("expected object-valued binary data descriptor: {other:?}"),
        }
    }

    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script_graph(
            r#"
(function () {
  const root = __scriptwCreateRealm().global, keys = Reflect.ownKeys, remove = Reflect.deleteProperty, collect = gc;
  globalThis.Uint8Array = {replacement: true};
  root.savedToString = root.Array.prototype.toString;
  delete root.Array.prototype.toString;
  Object.setPrototypeOf(root, null);
  Object.setPrototypeOf(root.Function.prototype, null);
  delete root.Function.prototype.constructor;
  delete root.Object.prototype.constructor;
  for (const key of keys(root)) if (key !== 'Uint8Array' && key !== 'ArrayBuffer' && key !== 'savedToString') remove(root, key);
  collect();
  return root;
})();
"#,
            compile_options(),
            run_options(),
            SnapshotLimits::HARD_MAX,
        )
        .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
        panic!("binary snapshot rejected: {:?}", outcome.completion.outcome);
    };
    let root = &graph.nodes()[0];
    for name in ["ArrayBuffer", "Uint8Array"] {
        let descriptor = property(root, name);
        assert!(matches!(
            descriptor,
            SnapshotDescriptor::Data {
                writable: true,
                enumerable: false,
                configurable: true,
                ..
            }
        ));
        let constructor_id = object_id(descriptor);
        let constructor = &graph.nodes()[constructor_id as usize];
        let prototype = &graph.nodes()[object_id(property(constructor, "prototype")) as usize];
        assert_eq!(
            object_id(property(prototype, "constructor")),
            constructor_id
        );
    }
    let constructor = &graph.nodes()[object_id(property(root, "Uint8Array")) as usize];
    let prototype = &graph.nodes()[object_id(property(constructor, "prototype")) as usize];
    let SnapshotValue::Object { id } = &prototype.prototype else {
        panic!("concrete TypedArray prototype must inherit the abstract prototype");
    };
    let abstract_prototype = &graph.nodes()[*id as usize];
    assert_eq!(
        object_id(property(abstract_prototype, "toString")),
        object_id(property(root, "savedToString")),
        "snapshot retains the original shared eager callable after deleting the Array alias",
    );
    assert!(outcome.output_events.is_empty());
}

#[test]
fn dynamic_bytes_module_first_use_allocates_canonical_intrinsics_after_public_deletion() {
    use scriptw_engine::{
        EmbeddedModuleBytesInput, EmbeddedModuleEntryInput, EmbeddedModuleGoal,
        EmbeddedModuleGraph, EmbeddedModuleInput, EmbeddedModuleReferrer,
        EmbeddedModuleResolutionInput, ModuleLoadingPolicy,
    };

    const ENTRY: &str = r#"
const names = ['ArrayBuffer','SharedArrayBuffer','DataView','Int8Array','Uint8Array','Uint8ClampedArray',
  'Int16Array','Uint16Array','Int32Array','Uint32Array','Float16Array','Float32Array','Float64Array',
  'BigInt64Array','BigUint64Array'];
for (const name of names) Reflect.deleteProperty(globalThis, name);
gc();
const namespace = await import('./data.bin', {with: {type: 'bytes'}}), bytes = namespace.default;
function check(value, label) { if (!value) throw new Error(label); }
const prototype = Object.getPrototypeOf(bytes), C = prototype.constructor;
const bufferPrototype = Object.getPrototypeOf(bytes.buffer), B = bufferPrototype.constructor;
check(C.name === 'Uint8Array' && C.prototype === prototype &&
  Object.getPrototypeOf(Object.getPrototypeOf(C)) === Function.prototype, 'view canonical Realm');
check(B.name === 'ArrayBuffer' && B.prototype === bufferPrototype &&
  Object.getPrototypeOf(bufferPrototype) === Object.prototype && bytes.buffer.immutable, 'buffer canonical Realm');
check(bytes.join(',') === '0,255,128,65' && Array.from(bytes.values()).join(',') === '0,255,128,65', 'bytes and iterator');
check(names.every(function (name) { return !Object.hasOwn(globalThis, name); }), 'public globals remain deleted');
gc();
const repeated = await import('./data.bin', {with: {type: 'bytes'}});
check(repeated === namespace && repeated.default === bytes && Object.getPrototypeOf(new C([3])) === prototype &&
  Object.getPrototypeOf(new B(1)) === bufferPrototype, 'repeated module and retained constructors after GC');
print('ok');
"#;
    let graph = EmbeddedModuleGraph::try_new_typed(
        EmbeddedModuleEntryInput {
            goal: EmbeddedModuleGoal::Module,
            identity: "entry.js".into(),
            source: ENTRY.into(),
            meta_url: "scriptw://entry.js".into(),
        },
        vec![EmbeddedModuleInput::Bytes(EmbeddedModuleBytesInput {
            identity: "data.bin".into(),
            bytes: vec![0, 255, 128, 65],
            meta_url: "scriptw://data.bin".into(),
        })],
        vec![EmbeddedModuleResolutionInput {
            referrer: EmbeddedModuleReferrer::Module("entry.js".into()),
            specifier: "./data.bin".into(),
            attributes: vec![("type".into(), "bytes".into())],
            target: "data.bin".into(),
        }],
    )
    .unwrap();
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_module(
            ENTRY,
            CompileOptions {
                module_loading_policy: ModuleLoadingPolicy::Embedded(graph),
                ..compile_options()
            },
            run_options(),
        )
        .unwrap();
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(matches!(observed.completion, ObservedCompletion::Normal(_)));
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine("ok".into())]
    );
}

#[test]
fn agent_received_shared_buffer_first_use_keeps_deleted_constructor_globals_deleted() {
    const WORKER: &str = r#"
const names = ['ArrayBuffer','SharedArrayBuffer','DataView','Int8Array','Uint8Array','Uint8ClampedArray',
  'Int16Array','Uint16Array','Int32Array','Uint32Array','Float16Array','Float32Array','Float64Array',
  'BigInt64Array','BigUint64Array'];
for (const name of names) Reflect.deleteProperty(globalThis, name);
gc();
const message = __scriptwAgentReceiveBroadcast(), buffer = message[0];
const prototype = Object.getPrototypeOf(buffer), C = prototype.constructor;
if (message[1] !== 37 || C.name !== 'SharedArrayBuffer' || C.prototype !== prototype ||
    Object.getPrototypeOf(C) !== Function.prototype || Object.getPrototypeOf(prototype) !== Object.prototype ||
    buffer.byteLength !== 4 || Object.getPrototypeOf(new C(8)) !== prototype ||
    !names.every(function (name) { return !Object.hasOwn(globalThis, name); }))
  throw new Error('agent received buffer has incomplete canonical graph');
gc();
if (Object.getOwnPropertyDescriptor(C, Symbol.species).get.call(C) !== C)
  throw new Error('agent retained SharedArrayBuffer species');
__scriptwAgentReport('binary-ready');
__scriptwAgentLeaving();
"#;
    let source = format!(
        r#"
__scriptwAgentStart({WORKER:?});
__scriptwAgentBroadcast(new SharedArrayBuffer(4), 37);
var report = null;
while ((report = __scriptwAgentGetReport()) === null) __scriptwAgentSleep(1);
if (report !== 'binary-ready') throw new Error('missing agent binary result');
true;
"#
    );
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_wasm_aot_script_with_agents(
            &source,
            compile_options(),
            Some(120_000),
            true,
            String::new(),
        )
        .expect("agent receive compiles and executes through Wasm AOT");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}
