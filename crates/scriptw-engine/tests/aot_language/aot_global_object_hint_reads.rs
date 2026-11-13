//! A global read that starts at the Global Environment Record answers from
//! the global object's dictionary entry that the name's String remembers,
//! without a call, only while that record has no declarative names and the
//! entry is a data property keyed by that very String. Module code reads
//! globals this way, Script code too. Each script changes one of those facts
//! between executions of the same sites. Expectations are Node 24's.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, module: bool, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let options = CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    };
    let run = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    };
    let observed = if module {
        engine.observe_module(source, options, run)
    } else {
        engine.observe_script(source, options, run)
    }
    .expect("the source executes compiled Wasm");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}\n{source}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine(expected.into())],
        "{source}"
    );
}

/// A module, and the same body as strict Script function code.
fn run_as_module_and_script(source: &str, expected: &str) {
    run(source, true, expected);
    run(
        &format!("(function () {{\n\"use strict\";\n{source}}})();"),
        false,
        expected,
    );
}

#[test]
fn a_remembered_global_entry_answers_only_while_it_still_holds_the_binding() {
    // The same String used as a key of another dictionary first moves its
    // remembered position; the global property is then replaced, deleted
    // and re-added (at a new position), redefined as an accessor and as an
    // own `undefined`, and finally shadowed by a global lexical binding.
    run_as_module_and_script(
        r#"
var log = [];
var nativeMap = Map;
function read() { try { var v = Map; return v === nativeMap ? "native" : String(v); } catch (e) { return e.constructor.name; } }
function kind() { return typeof Map; }
function isFunction() { return typeof Map == "function"; }
log.push(read(), read(), kind(), isFunction());
var other = {};
other.first = 1;
other.Map = "other";
log.push(other.Map, read(), isFunction());
globalThis.Map = 5;
log.push(read(), kind());
delete globalThis.Map;
log.push(read(), kind(), isFunction());
globalThis.Map = "back";
log.push(read(), kind());
Object.defineProperty(globalThis, "Map", { get: function () { return "getter"; }, configurable: true });
log.push(read(), kind());
Object.defineProperty(globalThis, "Map", { value: undefined, writable: true, configurable: true });
log.push(read(), kind(), isFunction());
globalThis.Map = nativeMap;
log.push(read(), isFunction());
__scriptwRealmEvalScript("let Map = 'lexical';");
log.push(read(), kind(), isFunction(), globalThis.Map === nativeMap);
print(log.join());
"#,
        "native,native,function,true,other,native,true,5,number,ReferenceError,undefined,false,back,string,getter,string,undefined,undefined,false,native,true,lexical,string,false,true",
    );
}
