//! `instanceof` skips the dictionary probe for @@hasInstance while that
//! Symbol has never been hashed. Defining it on a constructor, or on an
//! object between the constructor and %Function.prototype%, must take effect
//! on the next test, and removing it must restore the ordinary answer.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, expected: &str) {
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
        .expect("the script executes compiled Wasm");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn a_later_has_instance_method_on_a_dictionary_constructor_or_hop_is_honoured() {
    run(
        r#"
var log = [];
function check(v, C) { return v instanceof C; }
var o = {};
function K() {}
for (var i = 0; i < 40; i++) K["p" + i] = i;
var k = new K();
log.push(check(k, K), check(o, K), check(o, Date), check(new Date(0), Date), check(o, Int8Array), check(new Int8Array(1), Int8Array));
Object.defineProperty(K, Symbol.hasInstance, { value: function (v) { return v === o; }, configurable: true });
log.push(check(k, K), check(o, K), check(o, Date), check(new Date(0), Date));
delete K[Symbol.hasInstance];
log.push(check(k, K), check(o, K));
var TA = Object.getPrototypeOf(Int8Array);
Object.defineProperty(TA, Symbol.hasInstance, { value: function () { return "yes"; }, configurable: true });
log.push(check(o, Int8Array), check(o, Uint8Array), check(o, Date));
delete TA[Symbol.hasInstance];
log.push(check(o, Int8Array), check(new Int8Array(1), Int8Array));
Object.defineProperty(Date, Symbol.hasInstance, { value: function () { return 1; } });
log.push(check(o, Date), check(o, Map), check(new Map(), Map));
print(log.join());
"#,
        "true,false,false,true,false,true,false,true,false,true,true,false,true,true,false,false,true,true,false,true",
    );
}

#[test]
fn a_class_with_a_static_has_instance_method_does_not_disturb_other_constructors() {
    run(
        r#"
var log = [];
function check(v, C) { return v instanceof C; }
function K() {}
for (var i = 0; i < 40; i++) K["p" + i] = i;
var k = new K();
log.push(check(k, K), check({}, K));
class Even { static [Symbol.hasInstance](n) { return n % 2 === 0; } }
log.push(check(2, Even), check(3, Even), check(k, K), check({}, K), check([], Array), check({}, Array));
print(log.join());
"#,
        "true,false,true,false,true,false,true,false",
    );
}
