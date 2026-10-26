use super::*;

fn normal(completion: JsCompletion) -> JsValue {
    match completion {
        JsCompletion::Normal(value) => value,
        JsCompletion::Throw(value) => panic!("unexpected JavaScript throw: {value:?}"),
    }
}

#[test]
fn compact_stored_values_roundtrip_every_value_kind_through_source_free_embedding() {
    let compiled = artifact(
        r#"
export const absent = undefined, nothing = null, yes = true, no = false;
export const nan = NaN, negativeZero = -0, infinity = Infinity, fraction = 1.5;
export const text = '\0\ud800\udfff', bigint = 12345678901234567890n;
export const symbol = Symbol('compact'), object = {}, array = [];
export const argumentsValue = (function () { return arguments; })(1);
export let retained;
export function echo(value) { retained = value; return value; }
export function identical(left, right) { return Object.is(left, right); }
"#,
        ForeignModuleRegistry::default(),
    );
    let bytes = compiled
        .encode_portable_bundle(&IntlCompilationProfile::Minimal)
        .unwrap();
    let loaded = Engine::new(RealmBuilder::new().build())
        .load_portable_bundle(&bytes)
        .unwrap();
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &loaded).unwrap();
    let mut store = store(&runtime, 0.0);
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    let echo = normal(instance.get_export(&mut store, "echo").unwrap());
    let identical = normal(instance.get_export(&mut store, "identical").unwrap());
    for name in [
        "absent",
        "nothing",
        "yes",
        "no",
        "nan",
        "negativeZero",
        "infinity",
        "fraction",
        "text",
        "bigint",
        "symbol",
        "object",
        "array",
        "argumentsValue",
        "echo",
    ] {
        let original = normal(instance.get_export(&mut store, name).unwrap());
        let echoed = normal(
            instance
                .call(&mut store, &echo, None, std::slice::from_ref(&original))
                .unwrap(),
        );
        store.gc(None).unwrap();
        let matches = normal(
            instance
                .call(&mut store, &identical, None, &[original, echoed])
                .unwrap(),
        );
        assert_eq!(matches.as_boolean(), Some(true), "{name}");
    }
}

#[test]
fn compact_descriptor_flags_preserve_every_attribute_and_kind_transition() {
    let compiled = artifact(
        r#"
function check(ok) { if (!ok) throw 'compact descriptor mismatch'; }
for (let bits = 0; bits < 8; bits++) {
  const writable = !!(bits & 1), enumerable = !!(bits & 2), configurable = !!(bits & 4);
  const target = Object.create(null);
  Object.defineProperty(target, 'value', {value:-0, writable, enumerable, configurable});
  const proxy = new Proxy(target, {});
  for (const object of [target, proxy]) {
    const d = Object.getOwnPropertyDescriptor(object, 'value');
    check(Object.is(d.value, -0) && d.writable === writable && d.enumerable === enumerable && d.configurable === configurable);
    check(!('get' in d) && !('set' in d));
    check(Object.keys(object).length === (enumerable ? 1 : 0));
    const copy = {...object};
    check(Object.hasOwn(copy, 'value') === enumerable);
    if (enumerable) check(Object.is(copy.value, -0));
    check(JSON.stringify(object) === (enumerable ? '{"value":0}' : '{}'));
  }
  const get = function () { return target; };
  if (configurable) {
    Object.defineProperty(target, 'value', {get});
    const accessor = Object.getOwnPropertyDescriptor(proxy, 'value');
    check(accessor.get === get && accessor.set === undefined && !('writable' in accessor));
    check(accessor.enumerable === enumerable && accessor.configurable);
    Object.defineProperty(target, 'value', {value:-0});
    const data = Object.getOwnPropertyDescriptor(target, 'value');
    check(!data.writable && data.enumerable === enumerable && data.configurable && Object.is(data.value, -0));
  } else {
    check(!Reflect.defineProperty(proxy, 'value', {get}));
  }
  Object.freeze(target);
  const frozen = Object.getOwnPropertyDescriptor(proxy, 'value');
  check(!frozen.writable && !frozen.configurable && frozen.enumerable === enumerable && Object.is(frozen.value, -0));
  check(!Reflect.deleteProperty(proxy, 'value'));
}
for (let bits = 0; bits < 4; bits++) {
  const enumerable = !!(bits & 1), configurable = !!(bits & 2), key = Symbol('key');
  const target = {}, get = function () { return target; }, set = function (value) {};
  Object.defineProperty(target, key, {get, set, enumerable, configurable});
  const proxy = new Proxy(target, {}), d = Object.getOwnPropertyDescriptor(proxy, key);
  check(d.get === get && d.set === set && d.enumerable === enumerable && d.configurable === configurable);
  check(!('value' in d) && !('writable' in d) && proxy[key] === target);
  Object.freeze(target);
  const frozen = Object.getOwnPropertyDescriptor(proxy, key);
  check(frozen.get === get && frozen.set === set && !frozen.configurable && frozen.enumerable === enumerable);
  check(!Reflect.defineProperty(proxy, key, {value:1}) && !Reflect.deleteProperty(proxy, key));
}
print('compact-descriptors-ok');
"#,
        ForeignModuleRegistry::default(),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &compiled).unwrap();
    let mut store = store(&runtime, 0.0);
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert_output(
        instance.evaluate(&mut store).unwrap(),
        "compact-descriptors-ok",
    );
}
