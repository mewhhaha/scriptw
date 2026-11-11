use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let observed = Engine::new(RealmBuilder::new().build())
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
        .expect("descriptor edge controls execute compiled Wasm");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine("descriptor-edges:ok".into())]
    );
}

#[test]
fn partial_descriptor_merges_retain_omitted_fields_and_reset_converted_sides() {
    run(r#"
function check(ok, label) { if (!ok) throw label; }
var marker = {}, object = {}, reads = 0, writes = 0, received;
function getFirst() { reads++; return 1; }
function getSecond() { reads++; return marker; }
function setFirst(value) { writes++; received = value; }
function setSecond(value) { writes++; received = value; }
Object.defineProperty(object, 'slot', {get:getFirst, set:setFirst, enumerable:true, configurable:true});
Object.defineProperty(object, 'slot', {enumerable:false}); gc();
var d = Object.getOwnPropertyDescriptor(object, 'slot');
check(d.get === getFirst && d.set === setFirst && !d.enumerable && d.configurable, 'attribute-only accessor retains both fields');
Object.defineProperty(object, 'slot', {get:getSecond}); gc();
d = Object.getOwnPropertyDescriptor(object, 'slot');
check(d.get === getSecond && d.set === setFirst && !d.enumerable && d.configurable, 'getter-only retains setter');
Object.defineProperty(object, 'slot', {set:setSecond}); gc();
d = Object.getOwnPropertyDescriptor(object, 'slot');
check(d.get === getSecond && d.set === setSecond && reads === 0 && writes === 0, 'setter-only retains getter without invoking either');
check(object.slot === marker, 'retained getter callable after GC');
object.slot = marker;
check(reads === 1 && writes === 1 && received === marker, 'retained setter callable after GC');
Object.defineProperty(object, 'slot', {set:undefined}); gc();
d = Object.getOwnPropertyDescriptor(object, 'slot');
check(d.get === getSecond && d.set === undefined, 'present undefined replaces setter');
Object.defineProperty(object, 'slot', {configurable:false});
check(Reflect.defineProperty(object, 'slot', {get:getSecond}), 'same frozen getter permitted');
check(!Reflect.defineProperty(object, 'slot', {get:getFirst}) && !Reflect.defineProperty(object, 'slot', {set:setFirst}), 'frozen accessor replacement rejected');
check(!Reflect.defineProperty(object, 'slot', {value:marker}) && !Reflect.defineProperty(object, 'slot', {enumerable:true}), 'frozen kind and attribute changes rejected');
gc(); d = Object.getOwnPropertyDescriptor(object, 'slot');
check(d.get === getSecond && d.set === undefined && !d.configurable && !d.enumerable, 'rejection retains complete descriptor');
Object.defineProperty(object, 'data', {value:marker, writable:true, enumerable:true, configurable:true});
Object.defineProperty(object, 'data', {enumerable:false}); gc();
d = Object.getOwnPropertyDescriptor(object, 'data');
check(d.value === marker && d.writable && !d.enumerable && d.configurable && !('get' in d), 'attribute-only data retains value');
Object.defineProperty(object, 'data', {get:getFirst}); gc();
d = Object.getOwnPropertyDescriptor(object, 'data');
check(d.get === getFirst && d.set === undefined && !('value' in d) && !('writable' in d) && !d.enumerable && d.configurable, 'data to getter resets setter and data fields');
Object.defineProperty(object, 'data', {writable:true}); gc();
d = Object.getOwnPropertyDescriptor(object, 'data');
check(d.value === undefined && d.writable && !('get' in d) && !d.enumerable && d.configurable, 'accessor to data defaults value');
Object.defineProperty(object, 'data', {set:setFirst}); gc();
d = Object.getOwnPropertyDescriptor(object, 'data');
check(d.get === undefined && d.set === setFirst && !('value' in d), 'data to setter resets getter');
Object.defineProperty(object, 'data', {value:marker}); gc();
d = Object.getOwnPropertyDescriptor(object, 'data');
check(d.value === marker && !d.writable && !('get' in d) && !('set' in d), 'accessor to supplied data defaults writable');
Object.defineProperty(object, 'fresh', {get:getFirst}); gc();
d = Object.getOwnPropertyDescriptor(object, 'fresh');
check(d.get === getFirst && d.set === undefined && !d.enumerable && !d.configurable, 'new partial accessor completes absent fields');
print('descriptor-edges:ok');
"#);
}

#[test]
fn proxy_snapshot_retains_old_getter_and_omitted_setter_across_reentrant_merge_and_gc() {
    run(r#"
function check(ok, label) { if (!ok) throw label; }
for (var key of ['slot', Symbol('slot')]) {
  var target = {}, oldCalls = 0, setValue;
  function oldGetter() { oldCalls++; return 1; }
  function newGetter() { return 2; }
  function setter(value) { setValue = value; }
  Object.defineProperty(target, key, {get:oldGetter, set:setter, enumerable:true, configurable:true});
  var conversions = 0;
  var fields = {
    enumerable:true,
    get configurable() {
      conversions++;
      Object.defineProperty(target, key, {get:newGetter, configurable:false});
      gc();
      return true;
    },
    get:oldGetter,
    set:setter
  };
  var proxy = new Proxy(target, {getOwnPropertyDescriptor() { gc(); return fields; }});
  var before = Reflect.getOwnPropertyDescriptor(proxy, key);
  check(before.get === oldGetter && before.set === setter && before.configurable && before.enumerable && !('value' in before), 'fresh retained accessor snapshot');
  var after = Object.getOwnPropertyDescriptor(target, key);
  check(after.get === newGetter && after.set === setter && !after.configurable && after.enumerable && conversions === 1 && oldCalls === 0, 'partial live merge remains independent of snapshot');
  gc();
  check(Reflect.get(proxy, key) === 2 && Reflect.set(proxy, key, 7) && setValue === 7, 'live accessor edges remain callable after GC');
  var rejected = false;
  try { Reflect.getOwnPropertyDescriptor(proxy, key); } catch (error) { rejected = error instanceof TypeError; }
  check(rejected && conversions === 2 && oldCalls === 0, 'later acquisition observes fixed descriptor');
  check(before.get === oldGetter && before.set === setter && before.configurable, 'public prior snapshot remains independent');
}
print('descriptor-edges:ok');
"#);
}
