//! A function body is compiled before the script statements that run ahead of
//! its first call. Any of those statements can replace a builtin global or one
//! of its methods (ECMA-262 19.1-19.4 make them writable and configurable), so
//! the body must observe the replacement whichever write path made it: a
//! computed key, an accessor, a reflective or bulk builtin write, an alias, a
//! helper function, an indirect eval or a named method store. Each program
//! uses a replacement returning a kind the intrinsic never returns, so a
//! result kind kept from the intrinsic shows up as a wrong sum. Expectations
//! are Node 22's.
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
        "{:?}\n{source}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine(expected.into())],
        "{source}"
    );
}

const RUN: &str = "function run() { return Math.imul(2, 3) + 1; }";
const REPLACEMENT: &str = "{ imul() { return \"x\"; } }";

#[test]
fn every_dormant_function_kind_observes_a_computed_key_math_replacement() {
    run(
        r#"
function imul() { return Math.imul(2, 3) + 1; }
function floor(x) { return Math.floor(x) + 1; }
function minmax() { return Math.min(2, 3) + Math.max(2, 3); }
function abs() { return Math.abs(-2) + 1; }
function pi() { return Math.PI + 1; }
class C { m() { return Math.sqrt(4) + 1; } static s() { return Math.imul(2, 3) + 1; } }
var arrow = () => Math.imul(2, 3) + 1;
var object = { get g() { return Math.imul(2, 3) + 1; } };
function outer() { return function inner() { return Math.imul(2, 3) + 1; }; }
var inner = outer();
globalThis["Ma" + "th"] = { imul() { return "i"; }, floor() { return "f"; }, min() { return "m"; },
    max() { return "M"; }, abs() { return "a"; }, sqrt() { return "s"; }, PI: "p" };
print([imul(), floor(2.5), minmax(), abs(), pi(), new C().m(), C.s(), arrow(), object.g, inner()].join());
"#,
        "i1,f1,mM,a1,p1,s1,i1,i1,i1,i1",
    );
}

#[test]
fn a_dormant_function_observes_every_global_object_write_path() {
    for (write, expected) in [
        (
            "Object.defineProperty(globalThis, \"Math\", { get() { return { imul() { return \"y\"; } }; } });"
                .to_string(),
            "y1",
        ),
        (format!("Reflect.set(globalThis, \"Math\", {REPLACEMENT});"), "x1"),
        (
            format!("Reflect.defineProperty(globalThis, \"Math\", {{ value: {REPLACEMENT} }});"),
            "x1",
        ),
        (format!("Object.assign(globalThis, {{ Math: {REPLACEMENT} }});"), "x1"),
        (format!("var g = globalThis; g.Math = {REPLACEMENT};"), "x1"),
        (format!("this.Math = {REPLACEMENT};"), "x1"),
        (
            format!(
                "function set(o, k, v) {{ o[k] = v; }} set(globalThis, \"Math\", {REPLACEMENT});"
            ),
            "x1",
        ),
    ] {
        run(&format!("{RUN}\n{write}\nprint(run());"), expected);
    }
    run(
        "function run() { try { return Math.imul(2, 3) + 1; } catch (e) { return e.constructor.name; } }\n\
         delete globalThis[\"Ma\" + \"th\"];\n\
         print(run());",
        "ReferenceError",
    );
}

#[test]
fn a_dormant_function_observes_every_method_write_path() {
    for write in [
        "Math.imul = function () { return \"x\"; };",
        "Math[\"im\" + \"ul\"] = function () { return \"x\"; };",
        "var m = Math; m.imul = function () { return \"x\"; };",
        "Object.assign(Math, { imul() { return \"x\"; } });",
        "function set(o, v) { o.imul = v; } function replace() { set(Math, function () { return \"x\"; }); } replace();",
        "(0, eval)(\"Math.imul = function () { return 'x'; }\");",
        "var o = { valueOf() { Math.imul = function () { return \"x\"; }; return 1; } }; +o;",
    ] {
        run(&format!("{RUN}\n{write}\nprint(run());"), "x1");
    }
    run(
        "function run() { return \"abc\".charCodeAt(0) + 1; }\n\
         String.prototype[\"charCode\" + \"At\"] = function () { return \"c\"; };\n\
         print(run());",
        "c1",
    );
}

#[test]
fn dormant_conversions_and_jobs_observe_replaced_globals() {
    run(
        r#"
function number() { return Number("3") + 1; }
function string() { return String(5) + 1; }
function integer() { return parseInt("3") + 1; }
globalThis["Num" + "ber"] = function () { return "n"; };
globalThis["Str" + "ing"] = function () { return 7; };
globalThis["parse" + "Int"] = function () { return "p"; };
print([number(), string(), integer()].join());
"#,
        "n1,8,p1",
    );
    run(
        "Promise.resolve().then(function () { print(Math.imul(2, 3) + 1); });\n\
         globalThis[\"Ma\" + \"th\"] = { imul() { return \"x\"; } };",
        "x1",
    );
}
