//! A function body is compiled before the statements that run ahead of its
//! first call. Any of those statements can replace a builtin global or one
//! of its methods (ECMA-262 19.1-19.4 make them writable and configurable),
//! so the body must observe the replacement whichever write path made it: a
//! computed key, an accessor, a reflective or bulk builtin write, an alias, a
//! helper function, an indirect eval or a named method store. Each program
//! uses a replacement returning a kind the intrinsic never returns, so a
//! result kind kept from the intrinsic shows up as a wrong sum. Expectations
//! are Node 22's.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedRunOutcome, RealmBuilder, RunOptions,
};

fn options(prelude: Option<&str>) -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        module_prelude: prelude.map(str::to_owned),
        ..CompileOptions::default()
    }
}

fn run_options() -> RunOptions {
    RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    }
}

fn assert_printed(observed: ObservedRunOutcome, expected: &[&str], source: &str) {
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}\n{source}",
        observed.completion
    );
    let expected: Vec<_> = expected
        .iter()
        .map(|line| HostOutputEvent::PrintLine((*line).into()))
        .collect();
    assert_eq!(observed.output_events, expected, "{source}");
}

fn run_lines(source: &str, expected: &[&str]) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_script(source, options(None), run_options())
        .expect("the script executes compiled Wasm");
    assert_printed(observed, expected, source);
}

fn run(source: &str, expected: &str) {
    run_lines(source, &[expected]);
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
        "Object.defineProperty(Math, \"imul\", { value() { return \"x\"; } });",
        "Reflect.set(Math, \"imul\", function () { return \"x\"; });",
        "function set(o, v) { o.imul = v; } function replace() { set(Math, function () { return \"x\"; }); } replace();",
        "(0, eval)(\"Math.imul = function () { return 'x'; }\");",
        "var o = { valueOf() { Math.imul = function () { return \"x\"; }; return 1; } }; +o;",
    ] {
        run(&format!("{RUN}\n{write}\nprint(run());"), "x1");
    }
    for write in [
        "String.prototype[\"charCode\" + \"At\"] = function () { return \"c\"; };",
        "String.prototype.charCodeAt = function () { return \"c\"; };",
        "Object.defineProperty(String.prototype, \"charCodeAt\", { value() { return \"c\"; } });",
    ] {
        run(
            &format!(
                "function run() {{ return \"abc\".charCodeAt(0) + 1; }}\n{write}\nprint(run());"
            ),
            "c1",
        );
    }
}

#[test]
fn dormant_conversions_and_jobs_observe_replaced_globals() {
    run(
        r#"
function number() { return Number("3") + 1; }
function string() { return String(5) + 1; }
function boolean() { return Boolean(0) + 1; }
function integer() { return parseInt("3") + 1; }
globalThis["Num" + "ber"] = function () { return "n"; };
globalThis["Str" + "ing"] = function () { return 7; };
globalThis["Bool" + "ean"] = function () { return "b"; };
globalThis["parse" + "Int"] = function () { return "p"; };
print([number(), string(), boolean(), integer()].join());
"#,
        "n1,8,b1,p1",
    );
    run(
        "function integer() { return parseInt(\"3\") + 1; }\n\
         parseInt = function () { return \"p\"; };\n\
         print(integer());",
        "p1",
    );
    run(
        "Promise.resolve().then(function () { print(Math.imul(2, 3) + 1); });\n\
         globalThis[\"Ma\" + \"th\"] = { imul() { return \"x\"; } };",
        "x1",
    );
}

/// The entry guard holds again once the binding holds the intrinsic again,
/// and never for an edited intrinsic, which keeps its own storage for good.
#[test]
fn a_body_follows_its_builtins_through_replacement_and_restoration() {
    run_lines(
        r#"
function run() { return Math.imul(2, 3) + 1; }
var math = Math;
print(run());
globalThis["Ma" + "th"] = { imul() { return "x"; } };
print(run());
globalThis["Ma" + "th"] = math;
print(run());
math["im" + "ul"] = function () { return "y"; };
print(run());
delete math["im" + "ul"];
try { print(run()); } catch (error) { print(error.constructor.name); }
"#,
        &["7", "x1", "7", "y1", "TypeError"],
    );
    // An eval's lexical declaration is the eval's own and shadows nothing.
    run_lines(
        r#"
function run() { return Math.imul(2, 3) + 1; }
print(run());
(0, eval)("let Math = { imul() { return 'e'; } }; print(run());");
print(run());
"#,
        &["7", "7", "7"],
    );
}

/// A Script's own global lexical declarations leave its builtin facts
/// provable: the guard compares the declared names once.
#[test]
fn a_script_with_global_lexical_declarations_observes_replacements() {
    run_lines(
        r#"
let calls = 0;
const run = () => { calls += 1; return Math.imul(2, 3) + 1; };
class Holder { static twice(x) { return Math.floor(x) * 2; } }
print(run());
print(Holder.twice(2.5));
Math["im" + "ul"] = function () { return "x"; };
Math["flo" + "or"] = function () { return "3"; };
print(run());
print(Holder.twice(2.5));
print(calls);
"#,
        &["7", "4", "x1", "6", "2"],
    );
}

/// The inherited facts a function fact reads through (`.call`, `.apply`,
/// %Object.prototype% methods) are part of what the entry proves.
#[test]
fn a_dormant_function_observes_edited_shared_prototypes() {
    run(
        "function run() { return Math.max.call(null, 1, 2) + 1; }\n\
         Function.prototype[\"ca\" + \"ll\"] = function () { return \"c\"; };\n\
         print(run());",
        "c1",
    );
    run(
        "function run(o) { return Object.keys(o).length + 1; }\n\
         Object[\"ke\" + \"ys\"] = function () { return { length: \"k\" }; };\n\
         print(run({ a: 1 }));",
        "k1",
    );
    run(
        "function run(o) { return [1, 2].indexOf(2) + 1; }\n\
         Array.prototype[\"index\" + \"Of\"] = function () { return \"i\"; };\n\
         print(run());",
        "i1",
    );
}

/// A summary a caller reads of a callee holds whichever of the callee's
/// bodies runs.
#[test]
fn callers_of_a_guarded_body_observe_its_fallback_results() {
    run(
        r#"
class A { static f(x) { return Math.floor(x); } m() { return A.f(1.5) + 1; } }
function helper(x) { return Math.floor(x); }
function caller() { var s = 0; for (var i = 0; i < 3; i++) { s = s + helper(i + 0.5) * 2; } return s; }
function make(y) { const c = Math.floor(y); return function () { return c + 1; }; }
Promise.resolve().then(function () { print([new A().m(), caller(), make(2.5)()].join()); });
globalThis["Ma" + "th"] = { floor() { return "7"; } };
"#,
        "71,42,71",
    );
}

#[test]
fn a_prelude_a_module_and_a_created_realm_start_from_their_own_builtin_state() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    // The prelude Script replaces a builtin before the entry Script's
    // declarations are instantiated.
    let source = "function run() { return Math.imul(2, 3) + 1; }\nprint(run());\n\
                  Math[\"im\" + \"ul\"] = function () { return \"y\"; };\nprint(run());";
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            options(Some(
                "globalThis[\"Ma\" + \"th\"] = { imul() { return \"x\"; } };",
            )),
            run_options(),
        )
        .expect("the script executes compiled Wasm");
    assert_printed(observed, &["x1", "y1"], source);
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_script(source, options(Some("var unrelated = 1;")), run_options())
        .expect("the script executes compiled Wasm");
    assert_printed(observed, &["7", "y1"], source);

    let module = "function run() { return Math.imul(2, 3) + 1; }\n\
                  const before = run();\n\
                  globalThis[\"Ma\" + \"th\"] = { imul() { return \"x\"; } };\n\
                  print([before, run(), Math.imul(2, 3) + 1].join());";
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_module(module, options(None), run_options())
        .expect("the module executes compiled Wasm");
    assert_printed(observed, &["7,x1,x1"], module);
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_module(
            module,
            options(Some(
                "Math[\"im\" + \"ul\"] = function () { return \"p\"; };",
            )),
            run_options(),
        )
        .expect("the module executes compiled Wasm");
    assert_printed(observed, &["p1,x1,x1"], module);

    // Another Realm's Script starts from that Realm's own intrinsics and
    // leaves this Realm's alone.
    let agent = r#"
var other = __scriptwCreateRealm();
other.evalScript(`
  function run() { return Math.imul(2, 3) + 1; }
  var before = run();
  globalThis["Ma" + "th"] = { imul() { return "x"; } };
  var after = run();
`);
print([other.evalScript("before;"), other.evalScript("after;")].join());
function run() { return Math.imul(2, 3) + 1; }
print(run());
"#;
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_script(agent, options(None), run_options())
        .expect("the script executes compiled Wasm");
    assert_printed(observed, &["7,x1", "7"], agent);
}
