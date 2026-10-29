use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

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
        let observation = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                CompileOptions {
                    host_surface_policy: HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
                run_options(),
            )
            .unwrap_or_else(|error| panic!("Intl publication control: {error}\n{source}"));
        assert_eq!(observation.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observation.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0))),
            "{source}",
        );
        assert_eq!(
            observation.output_events,
            [HostOutputEvent::PrintLine("ok".into())],
            "{source}",
        );
    }
}

#[test]
fn first_descriptor_exposes_complete_ordered_constructor_graphs_in_fresh_realms() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var entryKeys = Reflect.ownKeys(globalThis);
check(entryKeys.indexOf('Intl') >= 0 && Reflect.has(globalThis, 'Intl') &&
  Object.hasOwn(globalThis, 'Intl') && !Object.prototype.propertyIsEnumerable.call(globalThis, 'Intl'),
  'unread entry global membership');
var first = __scriptwCreateRealm().global, second = __scriptwCreateRealm().global;
var names = ['DateTimeFormat','Locale','NumberFormat','PluralRules','ListFormat','Collator',
  'DisplayNames','RelativeTimeFormat','Segmenter','DurationFormat'];
var namespaces = [];
for (var global of [globalThis, first, second]) {
  var before = Reflect.ownKeys(global), position = before.indexOf('Intl');
  check(position >= 0, 'unread global membership');
  if (global !== globalThis) check(position > 0 && before[position - 1] === 'Temporal',
    'created global insertion order');
  var descriptor = Object.getOwnPropertyDescriptor(global, 'Intl'), namespace = descriptor.value;
  check(Object.hasOwn(descriptor, 'value') && !Object.hasOwn(descriptor, 'get') &&
    !Object.hasOwn(descriptor, 'set') && descriptor.writable && !descriptor.enumerable &&
    descriptor.configurable, 'ordinary global data property');
  check(namespace === global.Intl && namespace === Reflect.get(global, 'Intl') &&
    Reflect.ownKeys(global).join('|') === before.join('|'), 'stable namespace and global order');
  var keys = Reflect.ownKeys(namespace);
  check(keys.slice(0, -1).join(',') === 'getCanonicalLocales,supportedValuesOf,' + names.join(',') &&
    keys[keys.length - 1] === Symbol.toStringTag, 'complete namespace key order');
  check(Object.getPrototypeOf(namespace) === global.Object.prototype &&
    Object.prototype.toString.call(namespace) === '[object Intl]', 'namespace prototype and tag');
  var tag = Object.getOwnPropertyDescriptor(namespace, Symbol.toStringTag);
  check(tag.value === 'Intl' && !tag.writable && !tag.enumerable && tag.configurable,
    'namespace tag attributes');
  for (var method of ['getCanonicalLocales', 'supportedValuesOf']) {
    var member = Object.getOwnPropertyDescriptor(namespace, method), callable = member.value;
    check(typeof callable === 'function' && callable === namespace[method] && member.writable &&
      !member.enumerable && member.configurable && !Object.hasOwn(member, 'get') &&
      Object.getPrototypeOf(callable) === global.Function.prototype && callable.name === method &&
      callable.length === 1 && !Object.hasOwn(callable, 'prototype'), method + ' complete method');
  }
  for (var name of names) {
    var member = Object.getOwnPropertyDescriptor(namespace, name), constructor = member.value;
    var prototype = constructor.prototype;
    check(typeof constructor === 'function' && constructor === namespace[name] && member.writable &&
      !member.enumerable && member.configurable && !Object.hasOwn(member, 'get'), name + ' member');
    check(Object.getPrototypeOf(constructor) === global.Function.prototype &&
      Object.getPrototypeOf(prototype) === global.Object.prototype &&
      prototype.constructor === constructor && constructor.name === name &&
      constructor.length === (name === 'Locale' ? 1 : name === 'DisplayNames' ? 2 : 0),
      name + ' canonical constructor and defining Realm');
    var ownPrototype = Object.getOwnPropertyDescriptor(constructor, 'prototype');
    var backlink = Object.getOwnPropertyDescriptor(prototype, 'constructor');
    check(!ownPrototype.writable && !ownPrototype.enumerable && !ownPrototype.configurable &&
      backlink.value === constructor && backlink.writable && !backlink.enumerable &&
      backlink.configurable, name + ' prototype attributes');
    for (var key of ['name', 'length']) {
      var metadata = Object.getOwnPropertyDescriptor(constructor, key);
      check(!metadata.writable && !metadata.enumerable && metadata.configurable, name + ' ' + key);
    }
    var prototypeTag = Object.getOwnPropertyDescriptor(prototype, Symbol.toStringTag);
    check(prototypeTag.value === 'Intl.' + name && !prototypeTag.writable &&
      !prototypeTag.enumerable && prototypeTag.configurable, name + ' prototype tag');
    for (var key of Reflect.ownKeys(prototype)) {
      var property = Object.getOwnPropertyDescriptor(prototype, key);
      var callable = property.get === undefined ? property.value : property.get;
      if (typeof callable === 'function') {
        check(Object.getPrototypeOf(callable) === global.Function.prototype &&
          !property.enumerable && property.configurable, name + ' method defining Realm');
      }
    }
    if (name !== 'Locale') check(typeof constructor.supportedLocalesOf === 'function' &&
      Object.getPrototypeOf(constructor.supportedLocalesOf) === global.Function.prototype,
      name + ' static method');
  }
  namespaces.push(namespace);
}
check(namespaces[0] !== namespaces[1] && namespaces[1] !== namespaces[2], 'fresh namespace identities');
for (var name of names) {
  var left = namespaces[1][name], right = namespaces[2][name], marker = {};
  check(left !== right && left.prototype !== right.prototype, name + ' fresh constructor identities');
  namespaces[1][name] = marker;
  check(namespaces[1][name] === marker && namespaces[2][name] === right &&
    left.prototype.constructor === left && Object.getOwnPropertyDescriptor(first, 'Intl').value === namespaces[1],
    name + ' publication aliases retain mutations');
}
print('ok');
262;
"#,
    );
}

#[test]
fn internal_locale_entry_points_initialize_without_resurrecting_public_intl() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var routes = ['number', 'bigint', 'string', 'date', 'dateOnly', 'timeOnly', 'duration'];
for (var route of routes) {
  var global = __scriptwCreateRealm().global, marker = {}, gets = 0, sets = 0;
  var mode = route === 'number' || route === 'dateOnly' ? 'delete' :
    route === 'bigint' || route === 'timeOnly' ? 'assign' : route === 'string' ? 'define' : 'accessor';
  check(Reflect.has(global, 'Intl') && Reflect.ownKeys(global).indexOf('Intl') >= 0, 'unread Intl exists');
  var getter = function() { gets++; return marker; };
  var setter = function(value) { sets++; check(value === marker, 'accessor argument'); };
  if (mode === 'delete') check(Reflect.deleteProperty(global, 'Intl'), 'delete unread Intl');
  if (mode === 'assign') global.Intl = marker;
  if (mode === 'define') Object.defineProperty(global, 'Intl', {
    value: marker, writable: false, enumerable: true, configurable: false
  });
  if (mode === 'accessor') Object.defineProperty(global, 'Intl', {
    get: getter, set: setter, enumerable: true, configurable: true
  });
  if (route === 'number') check(global.Number.prototype.toLocaleString.call(1234, 'en-US') === '1,234', route);
  if (route === 'bigint') check(global.BigInt.prototype.toLocaleString.call(1234n, 'en-US') === '1,234', route);
  if (route === 'string') check(global.String.prototype.localeCompare.call('a', 'b', 'en') < 0, route);
  var date = new global.Date(0), options = {timeZone: 'UTC', year: 'numeric'};
  if (route === 'date') check(global.Date.prototype.toLocaleString.call(date, 'en-US', options) === '1970', route);
  if (route === 'dateOnly') check(global.Date.prototype.toLocaleDateString.call(date, 'en-US', options) === '1970', route);
  if (route === 'timeOnly') check(global.Date.prototype.toLocaleTimeString.call(date, 'en-US', {
    timeZone: 'UTC', hour: 'numeric', hourCycle: 'h23'
  }) === '00', route);
  if (route === 'duration') check(new global.Temporal.Duration(0,0,0,0,1).toLocaleString(
    'en', {style: 'long'}
  ) === '1 hour', route);
  check(gets === 0 && sets === 0, route + ' internal path invokes no public accessor');
  // A foreign default prototype lookup must reuse this graph even though its
  // public namespace has never been observed and may no longer be present.
  var target = global.Array.bind(null);
  Object.defineProperty(target, 'prototype', {value: 0, configurable: true});
  var locale = Reflect.construct(Intl.Locale, ['en'], target), prototype = Object.getPrototypeOf(locale);
  check(Object.getPrototypeOf(prototype) === global.Object.prototype &&
    Object.getPrototypeOf(prototype.constructor) === global.Function.prototype &&
    prototype.constructor.prototype === prototype && locale.language === 'en', route + ' canonical graph');
  check(gets === 0 && sets === 0, route + ' default prototype bypasses public accessor');
  var descriptor = Object.getOwnPropertyDescriptor(global, 'Intl');
  if (mode === 'delete') {
    check(descriptor === undefined && !Object.hasOwn(global, 'Intl') && global.Intl === undefined,
      route + ' no resurrection');
    global.Intl = marker;
    var keys = Reflect.ownKeys(global), reinserted = Object.getOwnPropertyDescriptor(global, 'Intl');
    check(keys[keys.length - 1] === 'Intl' && reinserted.value === marker && reinserted.writable &&
      reinserted.enumerable && reinserted.configurable, route + ' ordinary reinsertion');
  } else if (mode === 'accessor') {
    check(descriptor.get === getter && descriptor.set === setter && descriptor.enumerable &&
      descriptor.configurable && !Object.hasOwn(descriptor, 'value'), 'replacement accessor');
    check(global.Intl === marker && gets === 1, 'replacement Get');
    global.Intl = marker;
    check(sets === 1, 'replacement Set');
  } else {
    check(descriptor.value === marker && global.Intl === marker && descriptor.writable === (mode === 'assign') &&
      descriptor.enumerable === (mode === 'define') && descriptor.configurable === (mode === 'assign'),
      route + ' replacement descriptor survives');
  }
}
print('ok');
262;
"#,
    );
}

#[test]
fn foreign_default_prototypes_retain_segmenter_auxiliaries_and_legacy_formatter_slots() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var global = __scriptwCreateRealm().global, target = global.Array.bind(null);
Object.defineProperty(target, 'prototype', {value: undefined, configurable: true});
// GetPrototypeFromConstructor is the first operation to need this Realm's
// Intl graph, before reading any namespace or public constructor there.
var initial = Reflect.construct(Intl.Segmenter, ['en'], target);
var namespace = Object.getOwnPropertyDescriptor(global, 'Intl').value;
check(Object.getPrototypeOf(initial) === namespace.Segmenter.prototype, 'cold Segmenter default prototype');
for (var name of ['DateTimeFormat','Locale','NumberFormat','PluralRules','ListFormat','Collator',
  'DisplayNames','RelativeTimeFormat','Segmenter','DurationFormat']) {
  var args = name === 'Locale' ? ['en'] : name === 'DisplayNames' ? ['en', {type: 'language'}] :
    ['en', {timeZone: 'UTC'}];
  var result = Reflect.construct(Intl[name], args, target), prototype = namespace[name].prototype;
  check(Object.getPrototypeOf(result) === prototype && prototype.constructor === namespace[name] &&
    Object.getPrototypeOf(prototype) === global.Object.prototype, name + ' canonical default prototype');
}
var segments = initial.segment('AB'), segmentsPrototype = Object.getPrototypeOf(segments);
var iterator = segments[Symbol.iterator](), iteratorPrototype = Object.getPrototypeOf(iterator);
var ordinaryIteratorPrototype = Object.getPrototypeOf(Object.getPrototypeOf(new global.Array()[Symbol.iterator]()));
check(Object.getPrototypeOf(segmentsPrototype) === global.Object.prototype &&
  Object.getPrototypeOf(iteratorPrototype) === ordinaryIteratorPrototype &&
  Object.getPrototypeOf(segmentsPrototype.containing) === global.Function.prototype &&
  Object.getPrototypeOf(iteratorPrototype.next) === global.Function.prototype, 'auxiliary defining Realm');
check(Object.getPrototypeOf(segments.containing(0)) === global.Object.prototype &&
  Object.getPrototypeOf(iterator.next()) === global.Object.prototype &&
  Object.prototype.toString.call(iterator) === '[object Segmenter String Iterator]', 'auxiliary result owners');
var localWrapper = Object.create(Intl.NumberFormat.prototype);
Intl.NumberFormat.call(localWrapper, 'en');
var localKey = Object.getOwnPropertySymbols(localWrapper)[0], sharedKey;
global.Intl = null;
for (var constructor of [namespace.DateTimeFormat, namespace.NumberFormat]) {
  var wrapper = Object.create(constructor.prototype);
  check(constructor.call(wrapper, 'en', {timeZone: 'UTC'}) === wrapper, 'legacy wrapper identity');
  var key = Object.getOwnPropertySymbols(wrapper)[0], descriptor = Object.getOwnPropertyDescriptor(wrapper, key);
  check(key.description === 'IntlLegacyConstructedSymbol' && key !== localKey &&
    !descriptor.writable && !descriptor.enumerable && !descriptor.configurable &&
    Object.getPrototypeOf(descriptor.value) === constructor.prototype, 'legacy defining Realm slot');
  if (sharedKey !== undefined) check(key === sharedKey, 'legacy services share one Realm symbol');
  sharedKey = key;
  check(wrapper.format === descriptor.value.format && typeof wrapper.format(0) === 'string',
    'legacy callable survives namespace replacement');
}
var later = initial.segment('C'), next = later[Symbol.iterator]();
check(Object.getPrototypeOf(later) === segmentsPrototype && Object.getPrototypeOf(next) === iteratorPrototype &&
  next.next().value.segment === 'C' && global.Intl === null, 'auxiliary prototypes remain stable after replacement');
print('ok');
262;
"#,
    );
}

#[test]
fn first_publication_bypasses_poisoned_globals_and_preserves_locked_data_properties() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var global = __scriptwCreateRealm().global, hooks = 0;
var objectPrototype = global.Object.prototype, functionPrototype = global.Function.prototype;
function forbidden() { hooks++; throw new Error('publication invoked user code'); }
Object.defineProperty(global, 'Object', {get: forbidden, configurable: true});
Object.defineProperty(global, 'Function', {get: forbidden, configurable: true});
for (var key of ['DateTimeFormat','Locale','constructor','supportedLocalesOf','format','resolvedOptions']) {
  Object.defineProperty(objectPrototype, key, {get: forbidden, set: forbidden, configurable: true});
}
for (var key of ['name','length','prototype']) {
  Object.defineProperty(functionPrototype, key, {get: forbidden, set: forbidden, configurable: true});
}
Object.defineProperty(objectPrototype, Symbol.toStringTag, {get: forbidden, set: forbidden, configurable: true});
var namespace = Object.getOwnPropertyDescriptor(global, 'Intl').value;
check(Object.getPrototypeOf(namespace) === objectPrototype && hooks === 0, 'retained namespace prototype');
for (var name of ['DateTimeFormat','Locale','NumberFormat','PluralRules','ListFormat','Collator',
  'DisplayNames','RelativeTimeFormat','Segmenter','DurationFormat']) {
  var constructor = namespace[name];
  check(Object.getPrototypeOf(constructor) === functionPrototype && constructor.name === name &&
    Object.getPrototypeOf(constructor.prototype) === objectPrototype &&
    constructor.prototype.constructor === constructor, name + ' own publication bypasses inherited hooks');
}
check(typeof new namespace.NumberFormat('en').format(12) === 'string' && hooks === 0,
  'first internal operation invokes no publication hooks');
for (var mode of ['attributes', 'seal', 'freeze']) {
  var target = __scriptwCreateRealm().global;
  if (mode === 'attributes') Object.defineProperty(target, 'Intl', {writable: false, configurable: false});
  else Object[mode](target);
  var descriptor = Object.getOwnPropertyDescriptor(target, 'Intl'), value = descriptor.value;
  check(Object.hasOwn(descriptor, 'value') && !Object.hasOwn(descriptor, 'get') &&
    !descriptor.enumerable && !descriptor.configurable && descriptor.writable === (mode === 'seal') &&
    typeof value.DurationFormat === 'function', mode + ' complete locked data value');
  check(!Reflect.deleteProperty(target, 'Intl') &&
    Reflect.defineProperty(target, 'Intl', {value: value}), mode + ' locked SameValue');
  var copy = Object.defineProperties({}, {Intl: descriptor});
  check(copy.Intl === value && !Object.keys(value).length, mode + ' copied descriptor identity');
  var marker = {};
  check(Reflect.set(target, 'Intl', marker) === (mode === 'seal'), mode + ' writable state');
  check(target.Number.prototype.toLocaleString.call(12, 'en') === '12' &&
    target.Intl === (mode === 'seal' ? marker : value), mode + ' internal first use keeps public state');
}
print('ok');
262;
"#,
    );
}

#[test]
fn later_native_and_prepared_programs_observe_one_live_intl_graph() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var global = __scriptwCreateRealm().global, evaluate = global.eval;
var numberLocale = global.Number.prototype.toLocaleString;
var source = 'var savedNamespace = Intl, savedConstructor = Intl.NumberFormat;' +
  '[savedNamespace,savedConstructor,savedConstructor.prototype,Intl.Segmenter.prototype.segment];';
var hook = new Proxy(function() {}, {}); hook();
var first = evaluate(source + '/*' + Math.random() + '*/');
var namespace = first[0], constructor = first[1], prototype = first[2];
check(Object.getOwnPropertyDescriptor(global, 'Intl').value === namespace && namespace.NumberFormat === constructor &&
  Object.getPrototypeOf(constructor) === global.Function.prototype && prototype.constructor === constructor,
  'native first publication uses original Realm owners');
var marker = {};
global.savedMarker = marker;
namespace.NumberFormat = marker;
constructor.probe = marker;
Object.defineProperty(constructor, 'name', {value: 'RetainedNumberFormat'});
var second = evaluate("({namespace:Intl,published:Intl.NumberFormat,constructor:savedConstructor,prototype:savedConstructor.prototype,probe:savedConstructor.probe,name:savedConstructor.name,segment:Intl.Segmenter.prototype.segment,formatted:(12).toLocaleString('en')});");
check(second.namespace === namespace && second.published === marker && second.constructor === constructor &&
  second.prototype === prototype && second.probe === marker && second.name === 'RetainedNumberFormat' &&
  second.segment === first[3] && second.formatted === '12' && numberLocale.call(12, 'en') === '12',
  'prepared program and retained builtin preserve identities and mutations');
global.Intl = marker;
check(evaluate("Intl === savedMarker;") && numberLocale.call(12, 'en') === '12' && global.Intl === marker,
  'later internal formatting does not restore the namespace');
print('ok');
262;
"#,
    );
}

#[test]
fn portable_retained_host_get_keeps_unread_foreign_intl_separate_from_mutated_entry_realm() {
    use scriptw_engine::{
        EmbeddedEvaluation, EmbeddedInstance, EmbeddedRuntime, ForeignValue, JsCompletion, JsValue,
        WasmStoreState,
    };

    fn normal(completion: JsCompletion) -> JsValue {
        match completion {
            JsCompletion::Normal(value) => value,
            JsCompletion::Throw(value) => panic!("retained Intl host operation threw: {value:?}"),
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
    let unit = compiler
        .compile_module(
            "export const global = globalThis; export const foreign = __scriptwCreateRealm().global;\n\
             export function same(a, b) { return a === b; }\n\
             export function prototype(value) { return Object.getPrototypeOf(value); }\n\
             export function put(target, key, value) { target[key] = value; }",
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
        )
        .unwrap();
    // emit_wasm produces the Portable runtime profile; all Intl selection here
    // happens through retained host Get after module initialization and GC.
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
    let foreign = normal(instance.get_export(&mut store, "foreign").unwrap());
    let same = normal(instance.get_export(&mut store, "same").unwrap());
    let prototype_function = normal(instance.get_export(&mut store, "prototype").unwrap());
    let put = normal(instance.get_export(&mut store, "put").unwrap());
    let entry_namespace = get(&instance, &mut store, &global, "Intl");
    let entry_constructor = get(&instance, &mut store, &entry_namespace, "NumberFormat");
    let number_key = instance
        .value(
            &mut store,
            ForeignValue::StringUtf16("NumberFormat".encode_utf16().collect()),
        )
        .unwrap();
    normal(
        instance
            .call(
                &mut store,
                &put,
                None,
                &[entry_namespace.clone(), number_key, global.clone()],
            )
            .unwrap(),
    );
    store.gc(None).unwrap();
    let namespace = get(&instance, &mut store, &foreign, "Intl");
    let foreign_function = get(&instance, &mut store, &foreign, "Function");
    let foreign_function_prototype = get(&instance, &mut store, &foreign_function, "prototype");
    for name in [
        "DateTimeFormat",
        "Locale",
        "NumberFormat",
        "PluralRules",
        "ListFormat",
        "Collator",
        "DisplayNames",
        "RelativeTimeFormat",
        "Segmenter",
        "DurationFormat",
    ] {
        let constructor = get(&instance, &mut store, &namespace, name);
        let prototype = get(&instance, &mut store, &constructor, "prototype");
        let backlink = get(&instance, &mut store, &prototype, "constructor");
        assert_eq!(
            normal(
                instance
                    .call(&mut store, &same, None, &[constructor.clone(), backlink])
                    .unwrap()
            )
            .as_boolean(),
            Some(true),
            "{name} retained constructor cycle",
        );
        let owner = normal(
            instance
                .call(&mut store, &prototype_function, None, &[constructor])
                .unwrap(),
        );
        assert_eq!(
            normal(
                instance
                    .call(
                        &mut store,
                        &same,
                        None,
                        &[owner, foreign_function_prototype.clone()]
                    )
                    .unwrap()
            )
            .as_boolean(),
            Some(true),
            "{name} captured foreign Function prototype",
        );
    }
    let constructor = get(&instance, &mut store, &namespace, "NumberFormat");
    assert_eq!(
        normal(
            instance
                .call(
                    &mut store,
                    &same,
                    None,
                    &[entry_constructor, constructor.clone()]
                )
                .unwrap()
        )
        .as_boolean(),
        Some(false),
        "fresh Realm constructor identity",
    );
    let formatter = normal(instance.call(&mut store, &constructor, None, &[]).unwrap());
    let format = get(&instance, &mut store, &formatter, "format");
    store.gc(None).unwrap();
    let reread = get(&instance, &mut store, &foreign, "Intl");
    assert_eq!(
        normal(
            instance
                .call(&mut store, &same, None, &[namespace, reread])
                .unwrap()
        )
        .as_boolean(),
        Some(true),
        "retained namespace identity after GC",
    );
    let published = get(&instance, &mut store, &entry_namespace, "NumberFormat");
    assert_eq!(
        normal(
            instance
                .call(&mut store, &same, None, &[global, published])
                .unwrap()
        )
        .as_boolean(),
        Some(true),
        "foreign publication preserves entry Realm mutation",
    );
    assert!(!normal(instance.call(&mut store, &format, None, &[]).unwrap()).is_undefined());
    assert!(instance.take_output(&mut store).unwrap().is_empty());
}

#[test]
fn rooted_snapshot_forces_unread_foreign_intl_after_entry_namespace_replacement() {
    use scriptw_engine::{SnapshotLimits, SnapshotOutcome};
    use scriptw_runtime::rooted_snapshot::{
        SnapshotDescriptor, SnapshotKey, SnapshotNode, SnapshotValue,
    };

    fn property<'a>(node: &'a SnapshotNode, name: &str) -> &'a SnapshotDescriptor {
        let units = name.encode_utf16().collect::<Vec<_>>();
        &node.properties.iter().find(|property| {
            matches!(&property.key, SnapshotKey::String { units: key } if key.units() == units)
        }).unwrap_or_else(|| panic!("snapshot missing {name}")).descriptor
    }

    fn object_id(descriptor: &SnapshotDescriptor) -> u32 {
        match descriptor {
            SnapshotDescriptor::Data {
                value: SnapshotValue::Object { id },
                ..
            } => *id,
            other => panic!("expected object-valued Intl data descriptor: {other:?}"),
        }
    }

    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script_graph(
            r#"
(function() {
  const root = __scriptwCreateRealm().global, keys = Reflect.ownKeys, remove = Reflect.deleteProperty, collect = gc;
  globalThis.Intl = {replacement: true};
  Object.setPrototypeOf(root, null);
  Object.setPrototypeOf(root.Function.prototype, null);
  delete root.Function.prototype.constructor;
  delete root.Object.prototype.constructor;
  for (const key of keys(root)) if (key !== 'Intl') remove(root, key);
  collect();
  return root;
})();
"#,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            run_options(),
            SnapshotLimits::HARD_MAX,
        )
        .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
        panic!("Intl snapshot rejected: {:?}", outcome.completion.outcome);
    };
    let descriptor = property(&graph.nodes()[0], "Intl");
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
    assert_eq!(namespace.properties.len(), 13);
    for name in [
        "DateTimeFormat",
        "Locale",
        "NumberFormat",
        "PluralRules",
        "ListFormat",
        "Collator",
        "DisplayNames",
        "RelativeTimeFormat",
        "Segmenter",
        "DurationFormat",
    ] {
        let constructor_id = object_id(property(namespace, name));
        let constructor = &graph.nodes()[constructor_id as usize];
        let prototype = &graph.nodes()[object_id(property(constructor, "prototype")) as usize];
        assert_eq!(
            object_id(property(prototype, "constructor")),
            constructor_id,
            "{name}"
        );
        if name != "Locale" {
            object_id(property(constructor, "supportedLocalesOf"));
        }
    }
    object_id(property(namespace, "getCanonicalLocales"));
    object_id(property(namespace, "supportedValuesOf"));
    assert!(outcome.output_events.is_empty());
}
