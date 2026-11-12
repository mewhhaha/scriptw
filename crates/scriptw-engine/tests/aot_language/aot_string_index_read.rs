//! `string[number]` reads the String's own index property without the
//! wrapper object. Keys outside the string, non-integral keys and String
//! objects keep the general Get, including inherited index-like properties.
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
fn a_numeric_key_reads_one_code_unit_and_everything_else_keeps_get() {
    run(
        r#"
var log = [];
function at(s, k) { return s[k]; }
var s = "a😀z";
log.push(at("abc", 0), at("abc", 2), String(at("abc", 3)), at("abc", -0), String(at("abc", -1)), String(at("abc", 1.5)), String(at("abc", NaN)), String(at("abc", Infinity)));
log.push(at(s, 1).length, at(s, 1).charCodeAt(0), at(s, 3), String(at("", 0)));
log.push(at("abc", "1"), at(new String("xyz"), 1), at([7, 8], 1), String(at("abc", 4294967295)), String(at("abc", 4294967296)));
String.prototype[5] = "proto5";
Object.defineProperty(String.prototype, "7", { get: function () { return typeof this + ":" + this.length; }, configurable: true });
log.push(at("abc", 5), at("abcdefgh", 5), at("abc", 7), at("abcdefgh", 7));
delete String.prototype[5]; delete String.prototype[7];
log.push(String(at("abc", 5)), typeof at("abc", 1), at("abc", 1) === "b", at("q", 0) + at("q", 0));
function first(n) { return n[0] === "$" ? "$" + n : n; }
log.push(first("$x"), first("y"), first(""));
print(log.join());
"#,
        "a,c,undefined,a,undefined,undefined,undefined,undefined,1,55357,z,undefined,b,y,8,undefined,undefined,proto5,f,object:3,h,undefined,string,true,qq,$$x,y,",
    );
}
