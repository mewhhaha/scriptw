use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

fn assert_modes(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{source}");
        let outcome = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                CompileOptions {
                    host_surface_policy: HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(120_000),
                    ..RunOptions::default()
                },
            )
            .unwrap_or_else(|error| panic!("Temporal publication control: {error}\n{source}"));
        assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            outcome.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0))),
            "{source}",
        );
        assert_eq!(
            outcome.output_events,
            [HostOutputEvent::PrintLine("ok".into())],
            "{source}",
        );
    }
}

#[test]
fn first_descriptor_exposes_the_complete_stable_data_property_graph() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var before = Reflect.ownKeys(globalThis);
var position = before.indexOf('Temporal');
check(position > 0 && before[position - 1] === 'Atomics' && before[position + 1] === 'Intl',
  'global insertion order before first Get');
check(Reflect.has(globalThis, 'Temporal') && Object.hasOwn(globalThis, 'Temporal') &&
  !Object.prototype.propertyIsEnumerable.call(globalThis, 'Temporal'), 'global membership');
var descriptor = Object.getOwnPropertyDescriptor(globalThis, 'Temporal');
check(Object.hasOwn(descriptor, 'value') && !Object.hasOwn(descriptor, 'get') &&
  !Object.hasOwn(descriptor, 'set') && descriptor.writable && !descriptor.enumerable &&
  descriptor.configurable, 'global is an ordinary data property');
var namespace = descriptor.value;
check(namespace === Temporal && namespace === Reflect.get(globalThis, 'Temporal') &&
  Reflect.ownKeys(globalThis).join('|') === before.join('|'), 'stable namespace and global order');
var names = ['Instant', 'PlainDate', 'ZonedDateTime', 'PlainTime', 'PlainDateTime',
  'PlainYearMonth', 'PlainMonthDay', 'Duration'];
var keys = Reflect.ownKeys(namespace);
check(keys.slice(0, -1).join(',') === 'Now,' + names.join(',') &&
  keys[keys.length - 1] === Symbol.toStringTag, 'complete namespace key order');
check(Object.getPrototypeOf(namespace) === Object.prototype &&
  Object.prototype.toString.call(namespace) === '[object Temporal]', 'namespace prototype and tag');
var tag = Object.getOwnPropertyDescriptor(namespace, Symbol.toStringTag);
check(tag.value === 'Temporal' && !tag.writable && !tag.enumerable && tag.configurable, 'tag attributes');
for (var name of names) {
  var member = Object.getOwnPropertyDescriptor(namespace, name), constructor = member.value;
  check(typeof constructor === 'function' && member.writable && !member.enumerable &&
    member.configurable && !Object.hasOwn(member, 'get'), name + ' constructor data property');
  check(namespace[name] === constructor && Object.getPrototypeOf(constructor) === Function.prototype &&
    Object.getPrototypeOf(constructor.prototype) === Object.prototype &&
    constructor.prototype.constructor === constructor, name + ' canonical constructor cycle');
  check(typeof constructor.from === 'function' && typeof constructor.prototype.toString === 'function',
    name + ' complete methods');
}
var now = namespace.Now, nowKeys = Reflect.ownKeys(now);
check(now === namespace.Now && nowKeys.slice(0, -1).join(',') ===
  'timeZoneId,instant,zonedDateTimeISO,plainDateTimeISO,plainDateISO,plainTimeISO' &&
  nowKeys[nowKeys.length - 1] === Symbol.toStringTag, 'complete stable Now');
var instant = now.instant();
check(Object.getPrototypeOf(instant) === namespace.Instant.prototype &&
  typeof instant.epochNanoseconds === 'bigint', 'Now uses canonical result prototype');
var saved = descriptor.value;
globalThis.Temporal = {};
check(descriptor.value === saved && saved.Instant.prototype.constructor === saved.Instant &&
  saved.Now === now, 'descriptor snapshot survives public replacement');
print('ok');
262;
"#,
    );
}

#[test]
fn deletion_replacement_and_accessors_survive_internal_date_initialization() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
for (var mode of ['delete', 'assign', 'define', 'accessor']) {
  var global = __scriptwCreateRealm().global, marker = {}, gets = 0, sets = 0;
  check(Reflect.has(global, 'Temporal') && Reflect.ownKeys(global).indexOf('Temporal') >= 0,
    mode + ' initially present');
  var getter = function() { gets++; return marker; };
  var setter = function(value) { sets++; check(value === marker, 'accessor argument'); };
  if (mode === 'delete') check(Reflect.deleteProperty(global, 'Temporal'), 'delete unread property');
  if (mode === 'assign') global.Temporal = marker;
  if (mode === 'define') Object.defineProperty(global, 'Temporal', {
    value: marker, writable: false, enumerable: true, configurable: false
  });
  if (mode === 'accessor') Object.defineProperty(global, 'Temporal', {
    get: getter, set: setter, enumerable: true, configurable: true
  });
  // The public Temporal property has never been read. Date must reach the
  // canonical intrinsic even if that property no longer names the namespace.
  var result = global.Date.prototype.toTemporalInstant.call(new global.Date(7));
  var prototype = Object.getPrototypeOf(result), Instant = prototype.constructor;
  check(result.epochNanoseconds === 7000000n && Object.getPrototypeOf(prototype) === global.Object.prototype &&
    Object.getPrototypeOf(Instant) === global.Function.prototype &&
    Object.getPrototypeOf(Instant.fromEpochNanoseconds(11n)) === prototype,
    mode + ' internal canonical Instant');
  check(gets === 0 && sets === 0, mode + ' internal path invokes no public accessor');
  if (mode === 'delete') {
    check(!Object.hasOwn(global, 'Temporal') && global.Temporal === undefined, 'no resurrection');
    global.Temporal = marker;
    var keys = Reflect.ownKeys(global), last = keys[keys.length - 1];
    check(last === 'Temporal' && Object.getOwnPropertyDescriptor(global, 'Temporal').enumerable,
      'ordinary reinsertion order and attributes');
  } else if (mode === 'accessor') {
    var descriptor = Object.getOwnPropertyDescriptor(global, 'Temporal');
    check(descriptor.get === getter && descriptor.set === setter && descriptor.enumerable &&
      descriptor.configurable && !Object.hasOwn(descriptor, 'value'), 'replacement stays accessor');
    check(global.Temporal === marker && gets === 1, 'replacement getter');
    global.Temporal = marker;
    check(sets === 1, 'replacement setter');
  } else {
    var descriptor = Object.getOwnPropertyDescriptor(global, 'Temporal');
    check(descriptor.value === marker && global.Temporal === marker &&
      descriptor.writable === (mode === 'assign') && descriptor.enumerable === (mode === 'define') &&
      descriptor.configurable === (mode === 'assign'), mode + ' replacement descriptor');
  }
}
print('ok');
262;
"#,
    );
}

#[test]
fn attribute_only_definition_copy_seal_and_freeze_preserve_data_values() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var global = __scriptwCreateRealm().global;
Object.defineProperties(global, { Temporal: { writable: false, configurable: false } });
var descriptor = Object.getOwnPropertyDescriptor(global, 'Temporal'), namespace = descriptor.value;
check(typeof namespace.Duration === 'function' && !descriptor.writable && !descriptor.enumerable &&
  !descriptor.configurable && !Object.hasOwn(descriptor, 'get'), 'attribute-only definition retains value');
check(Reflect.defineProperty(global, 'Temporal', { value: namespace }) &&
  !Reflect.defineProperty(global, 'Temporal', { value: {} }), 'SameValue for locked data descriptor');
var copy = Object.defineProperties({}, { Temporal: descriptor });
var namespaceCopy = Object.defineProperties({}, Object.getOwnPropertyDescriptors(namespace));
check(copy.Temporal === namespace && namespaceCopy.Now === namespace.Now &&
  namespaceCopy.Instant === namespace.Instant && namespaceCopy[Symbol.toStringTag] === 'Temporal',
  'descriptor copies retain canonical values');
check(!Object.getOwnPropertyDescriptor(copy, 'Temporal').writable &&
  !Object.hasOwn(Object.assign({}, namespace), 'Instant') &&
  !Object.hasOwn({...namespace}, 'Now'), 'copy attributes and nonenumerable members');
for (var operation of ['seal', 'freeze']) {
  var target = __scriptwCreateRealm().global;
  Object[operation](target);
  var locked = Object.getOwnPropertyDescriptor(target, 'Temporal'), value = locked.value;
  check(!locked.configurable && !locked.enumerable && locked.writable === (operation === 'seal') &&
    !Object.hasOwn(locked, 'get') && typeof value.PlainDateTime === 'function', operation + ' data property');
  check(!Reflect.deleteProperty(target, 'Temporal') &&
    Reflect.defineProperty(target, 'Temporal', { value: value }), operation + ' stable SameValue');
  var marker = {};
  check(Reflect.set(target, 'Temporal', marker) === (operation === 'seal'), operation + ' writable state');
  var instant = target.Date.prototype.toTemporalInstant.call(new target.Date(1));
  check(Object.getPrototypeOf(instant) === value.Instant.prototype &&
    target.Temporal === (operation === 'seal' ? marker : value), operation + ' internal identity');
}
print('ok');
262;
"#,
    );
}

#[test]
fn first_publication_uses_retained_prototypes_and_invokes_no_mutated_hooks() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var global = __scriptwCreateRealm().global;
var objectPrototype = global.Object.prototype, functionPrototype = global.Function.prototype;
var hooks = 0;
function forbidden() { hooks++; throw new Error('initialization invoked user code'); }
Object.defineProperty(global, 'Object', { get: forbidden, configurable: true });
Object.defineProperty(global, 'Function', { get: forbidden, configurable: true });
var objectNames = ['Now', 'Instant', 'PlainDate', 'constructor', 'from', 'add', 'epochNanoseconds'];
for (var key of objectNames) Object.defineProperty(objectPrototype, key, {
  get: forbidden, set: forbidden, configurable: true
});
var functionNames = ['name', 'length', 'prototype'];
for (var key of functionNames) Object.defineProperty(functionPrototype, key, {
  get: forbidden, set: forbidden, configurable: true
});
Object.defineProperty(objectPrototype, Symbol.toStringTag, {
  get: forbidden, set: forbidden, configurable: true
});
var namespace = Object.getOwnPropertyDescriptor(global, 'Temporal').value;
check(hooks === 0 && Object.getPrototypeOf(namespace) === objectPrototype &&
  Object.getPrototypeOf(namespace.Now) === objectPrototype, 'namespace uses retained Object prototype');
for (var name of ['Instant', 'PlainDate', 'ZonedDateTime', 'PlainTime', 'PlainDateTime',
  'PlainYearMonth', 'PlainMonthDay', 'Duration']) {
  var constructor = namespace[name], prototype = constructor.prototype;
  check(Object.getPrototypeOf(constructor) === functionPrototype &&
    Object.getPrototypeOf(prototype) === objectPrototype && prototype.constructor === constructor &&
    constructor.name === name && typeof constructor.from === 'function' &&
    typeof prototype.toString === 'function', name + ' own publication bypasses inherited hooks');
}
var instant = namespace.Now.instant();
check(Object.getPrototypeOf(instant) === namespace.Instant.prototype &&
  typeof instant.epochNanoseconds === 'bigint' && hooks === 0, 'native result keeps canonical graph');
print('ok');
262;
"#,
    );
}

#[test]
fn borrowed_date_and_foreign_new_target_force_their_own_complete_realm_graph() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var dateGlobal = __scriptwCreateRealm().global;
var borrowed = dateGlobal.Date.prototype.toTemporalInstant;
var first = borrowed.call(new Date(2));
var dateNamespace = Object.getOwnPropertyDescriptor(dateGlobal, 'Temporal').value;
check(Object.getPrototypeOf(first) === dateNamespace.Instant.prototype &&
  first.epochNanoseconds === 2000000n && dateNamespace.Instant !== Temporal.Instant &&
  Object.getPrototypeOf(borrowed) === dateGlobal.Function.prototype, 'borrowed Date defining Realm');
var targetGlobal = __scriptwCreateRealm().global;
var NewTarget = targetGlobal.Array.bind(null);
Object.defineProperty(NewTarget, 'prototype', { value: 17, configurable: true });
var duration = Reflect.construct(Temporal.Duration, [0, 0, 0, 3], NewTarget);
var targetNamespace = Object.getOwnPropertyDescriptor(targetGlobal, 'Temporal').value;
check(Object.getPrototypeOf(duration) === targetNamespace.Duration.prototype && duration.days === 3,
  'foreign newTarget initializes default intrinsic in its Realm');
for (var namespace of [dateNamespace, targetNamespace]) {
  for (var name of ['Instant', 'PlainDate', 'ZonedDateTime', 'PlainTime', 'PlainDateTime',
    'PlainYearMonth', 'PlainMonthDay', 'Duration']) {
    check(typeof namespace[name].from === 'function' && namespace[name].prototype.constructor === namespace[name],
      name + ' complete after internal first force');
  }
}
var from = targetNamespace.PlainDate.from;
var date = from.call(Temporal.PlainDate, '2000-05-02');
check(Object.getPrototypeOf(date) === targetNamespace.PlainDate.prototype && date.day === 2,
  'borrowed method uses defining Realm after internal initialization');
var now = targetNamespace.Now.instant;
check(Object.getPrototypeOf(now.call(dateNamespace.Now)) === targetNamespace.Instant.prototype,
  'borrowed Now ignores receiver Realm');
print('ok');
262;
"#,
    );
}

#[test]
fn later_native_and_prepared_programs_share_the_defining_realms_temporal_graph() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var global = __scriptwCreateRealm().global;
var evaluate = global.eval, convertDate = global.Date.prototype.toTemporalInstant;
check(Reflect.has(global, 'Temporal'), 'Temporal is present before a later program reads it');
// The evaluator is retained before the first Temporal read. The runtime
// comment makes this exact source miss finite prepared-source discovery.
var source = 'var laterNamespace = Temporal; var laterInstant = Temporal.Instant;' +
  'laterInstant.probe = 41; [laterNamespace, laterInstant, laterInstant.prototype, Temporal.Now,' +
  'Temporal.PlainDate.from, Temporal.Now.instant];';
var hook = new Proxy(function() {}, {}); hook();
var first = evaluate(source + '/*' + Math.random() + '*/');
var namespace = first[0], Instant = first[1], prototype = first[2], now = first[3];
check(Object.getOwnPropertyDescriptor(global, 'Temporal').value === namespace &&
  global.Temporal === namespace && namespace.Instant === Instant && Instant.prototype === prototype &&
  namespace.Now === now && Object.getPrototypeOf(namespace) === global.Object.prototype,
  'native later program publishes the original Realm graph');
for (var row of [['Instant', 1], ['PlainDate', 3], ['ZonedDateTime', 2], ['PlainTime', 0],
  ['PlainDateTime', 3], ['PlainYearMonth', 2], ['PlainMonthDay', 2], ['Duration', 0]]) {
  var constructor = namespace[row[0]];
  check(constructor.name === row[0] && constructor.length === row[1] &&
    Object.getPrototypeOf(constructor) === global.Function.prototype &&
    constructor.prototype.constructor === constructor, row[0] + ' metadata after native initialization');
}
var date = convertDate.call(new Date(3));
check(Object.getPrototypeOf(date) === prototype && date.epochNanoseconds === 3000000n,
  'original program Date method uses the later initialized canonical slot');
var marker = {};
global.laterMarker = marker;
Instant.probe = 42;
Object.defineProperty(Instant, 'name', { value: 'RetainedInstant' });
namespace.Instant = marker;
now.probe = marker;
// This literal is a finite prepared source. It must observe the live graph
// and descriptor mutations rather than install a second namespace or slots.
var second = evaluate("({namespace: Temporal, published: Temporal.Instant, original: laterInstant, prototype: laterInstant.prototype, now: Temporal.Now, from: Temporal.PlainDate.from, nowInstant: Temporal.Now.instant, probe: laterInstant.probe, name: laterInstant.name, marker: Temporal.Now.probe, date: new Date(5).toTemporalInstant(), plain: Temporal.PlainDate.from('2000-05-02')});");
check(second.namespace === namespace && second.published === marker && second.original === Instant &&
  second.prototype === prototype && second.now === now && second.from === first[4] &&
  second.nowInstant === first[5] && second.probe === 42 && second.name === 'RetainedInstant' &&
  second.marker === marker, 'prepared later program keeps identities and mutations');
check(Object.getPrototypeOf(second.date) === prototype && second.date.epochNanoseconds === 5000000n &&
  Object.getPrototypeOf(second.plain) === namespace.PlainDate.prototype && second.plain.day === 2 &&
  namespace.Instant === marker && Instant.prototype.constructor === Instant,
  'prepared Date path retains canonical intrinsics without restoring public constructor');
print('ok');
262;
"#,
    );
}

#[test]
fn retained_host_reads_initialize_a_complete_namespace_without_source_references() {
    use scriptw_engine::{
        EmbeddedEvaluation, EmbeddedInstance, EmbeddedRuntime, JsCompletion, JsValue,
        WasmStoreState,
    };

    fn normal(completion: JsCompletion) -> JsValue {
        match completion {
            JsCompletion::Normal(value) => value,
            JsCompletion::Throw(value) => panic!("retained host operation threw: {value:?}"),
        }
    }

    fn get(
        instance: &EmbeddedInstance,
        store: &mut wasmtime::Store<WasmStoreState>,
        target: &JsValue,
        key: &str,
    ) -> JsValue {
        normal(
            instance
                .get_property(store, target, &key.encode_utf16().collect::<Vec<_>>())
                .unwrap(),
        )
    }

    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let compiler = Engine::new(RealmBuilder::new().build());
    // Product policy and this source carry no Temporal member or identifier.
    // All namespace selection happens through live host Get after module entry.
    let unit = compiler
        .compile_module(
            "export const global = globalThis; export function same(a, b) { return a === b; }",
            CompileOptions::default(),
        )
        .unwrap();
    let artifact = compiler.emit_wasm(&unit).unwrap();
    let mut config = wasmtime::Config::new();
    scriptw_engine::configure_wasm_runtime(&mut config);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let runtime = EmbeddedRuntime::compile(&engine, &artifact).unwrap();
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
    let global = normal(instance.get_export(&mut store, "global").unwrap());
    let same = normal(instance.get_export(&mut store, "same").unwrap());
    store.gc(None).unwrap();
    let namespace = get(&instance, &mut store, &global, "Temporal");
    let now = get(&instance, &mut store, &namespace, "Now");
    for name in [
        "Instant",
        "PlainDate",
        "ZonedDateTime",
        "PlainTime",
        "PlainDateTime",
        "PlainYearMonth",
        "PlainMonthDay",
        "Duration",
    ] {
        let constructor = get(&instance, &mut store, &namespace, name);
        let prototype = get(&instance, &mut store, &constructor, "prototype");
        let backlink = get(&instance, &mut store, &prototype, "constructor");
        assert_eq!(
            normal(
                instance
                    .call(&mut store, &same, None, &[constructor, backlink])
                    .unwrap()
            )
            .as_boolean(),
            Some(true),
            "{name} canonical cycle",
        );
    }
    store.gc(None).unwrap();
    for (target, key, retained) in [
        (&global, "Temporal", namespace.clone()),
        (&namespace, "Now", now.clone()),
    ] {
        let reread = get(&instance, &mut store, target, key);
        assert_eq!(
            normal(
                instance
                    .call(&mut store, &same, None, &[retained, reread])
                    .unwrap()
            )
            .as_boolean(),
            Some(true),
            "retained {key} identity after GC",
        );
    }
    let instant_method = get(&instance, &mut store, &now, "instant");
    let instant = normal(
        instance
            .call(&mut store, &instant_method, None, &[])
            .unwrap(),
    );
    assert!(!get(&instance, &mut store, &instant, "epochNanoseconds").is_undefined());
    assert!(instance.take_output(&mut store).unwrap().is_empty());
}

#[test]
fn rooted_snapshot_resolves_unread_global_data_property_and_complete_constructor_cycles() {
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
            other => panic!("expected ordinary object-valued data descriptor: {other:?}"),
        }
    }

    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script_graph(
            r#"
(function() {
  const root = globalThis, keys = Reflect.ownKeys, remove = Reflect.deleteProperty, collect = gc;
  Object.setPrototypeOf(root, null);
  Object.setPrototypeOf(Function.prototype, null);
  delete Function.prototype.constructor;
  delete Object.prototype.constructor;
  // Keep the unread global property while bounding the unrelated snapshot.
  for (const key of keys(root)) if (key !== 'Temporal') remove(root, key);
  collect();
  return root;
})();
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(120_000),
                ..RunOptions::default()
            },
            SnapshotLimits::HARD_MAX,
        )
        .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
        panic!(
            "Temporal snapshot rejected: {:?}",
            outcome.completion.outcome
        );
    };
    let global = &graph.nodes()[0];
    let descriptor = property(global, "Temporal");
    assert!(matches!(
        descriptor,
        SnapshotDescriptor::Data {
            writable: true,
            enumerable: false,
            configurable: true,
            ..
        }
    ));
    let namespace = &graph.nodes()[object_id(descriptor) as usize];
    assert_eq!(namespace.properties.len(), 10);
    for name in [
        "Instant",
        "PlainDate",
        "ZonedDateTime",
        "PlainTime",
        "PlainDateTime",
        "PlainYearMonth",
        "PlainMonthDay",
        "Duration",
    ] {
        let constructor_id = object_id(property(namespace, name));
        let constructor = &graph.nodes()[constructor_id as usize];
        let prototype = &graph.nodes()[object_id(property(constructor, "prototype")) as usize];
        assert_eq!(
            object_id(property(prototype, "constructor")),
            constructor_id,
            "{name}"
        );
        object_id(property(prototype, "toString"));
    }
    let now = &graph.nodes()[object_id(property(namespace, "Now")) as usize];
    assert_eq!(now.properties.len(), 7);
    for name in [
        "timeZoneId",
        "instant",
        "zonedDateTimeISO",
        "plainDateTimeISO",
        "plainDateISO",
        "plainTimeISO",
    ] {
        object_id(property(now, name));
    }
    assert!(outcome.output_events.is_empty());
}
