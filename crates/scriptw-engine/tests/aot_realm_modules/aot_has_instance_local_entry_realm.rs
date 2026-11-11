use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn borrowed_local_entry_keeps_intrinsic_realm_and_roots_proxy_operands_across_gc() {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compiler worker");
    let source = r#"
var foreign = __scriptwCreateRealm().global;
var foreignIntrinsic = foreign.Function.prototype[Symbol.hasInstance];
var localTarget = function Target() {};
var proxy = new Proxy(localTarget, {
  get(target, key, receiver) {
    if (key === Symbol.hasInstance) return foreignIntrinsic;
    if (key === 'prototype') {
      gc();
      return 1;
    }
    return Reflect.get(target, key, receiver);
  }
});
var candidate = {}, caught;
try { candidate instanceof proxy; }
catch (error) { caught = error; }
if (!caught || Object.getPrototypeOf(caught) !== foreign.TypeError.prototype) {
  throw 'operator-dispatched local entry lost its intrinsic Realm';
}
print('ok');
"#;
    let observation = Engine::new(RealmBuilder::new().build())
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
        .expect("borrowed native local entry executes through Wasm AOT");
    assert_eq!(observation.backend_used, ExecutionBackend::WasmAot);
    assert!(matches!(
        observation.completion,
        ObservedCompletion::Normal(_)
    ));
    assert_eq!(
        observation.output_events,
        vec![HostOutputEvent::PrintLine("ok".into())]
    );
}
