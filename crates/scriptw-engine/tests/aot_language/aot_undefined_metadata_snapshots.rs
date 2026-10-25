use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, RealmBuilder, RunOptions,
};

#[test]
fn metadata_redefinition_and_source_homes_remain_independent_across_realms() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let source = r#"
var foreign = __scriptwCreateRealm();
var otherMax = foreign.evalScript('Math.max');
var otherName = Object.getOwnPropertyDescriptor(otherMax, 'name');
var otherLength = Object.getOwnPropertyDescriptor(otherMax, 'length');
Object.defineProperty(Math.max, 'name', { get() { return 'changed'; }, configurable: true });
Object.defineProperty(Math.max, 'length', { get() { return 101; }, configurable: true });
Object.defineProperty(otherMax, 'length', { value: 99, configurable: true });
var mainName = Object.getOwnPropertyDescriptor(Math.max, 'name');
function First() {} function Second() {}
var replacement = { marker: 7 };
First.prototype = replacement;
var made = Reflect.construct(Object, [], First);
var parent = { get value() { return this.marker; } };
var home = { __proto__: parent, read() { return super.value; } };
class Base { get value() { return this.marker + 1; } }
class Child extends Base { read() { return super.value; } }
var instance = new Child(); instance.marker = 30;
gc();
Math.max !== otherMax && Math.max.name === 'changed' && Math.max.length === 101 &&
Math.min.name === 'min' && Math.min.length === 2 &&
otherName.value === 'max' && otherName.writable === false && otherName.configurable === true &&
otherLength.value === 2 && otherMax.name === 'max' && otherMax.length === 99 &&
typeof mainName.get === 'function' && mainName.set === undefined && !('value' in mainName) &&
Object.getPrototypeOf(made) === replacement && Second.prototype !== replacement &&
home.read.call({ marker: 23 }) === 23 && instance.read() === 31;
"#;
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
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
        .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}
