//! A guarded body keeps the whole proof of its builtin facts for one builtin
//! fact epoch and re-proves them only after the epoch advanced. Each program
//! here calls its body twice first, so that the second call ran on the kept
//! proof, and then changes what the proof was read from through one writer:
//! the next call must observe the change. A writer that did not advance the
//! epoch would leave the body on the intrinsic and print a number.
//! Expectations are Node 22's.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run_lines(source: &str, expected: &[&str]) {
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
    let expected: Vec<_> = expected
        .iter()
        .map(|line| HostOutputEvent::PrintLine((*line).into()))
        .collect();
    assert_eq!(observed.output_events, expected, "{source}");
}

/// Declares `body`, prints `call` twice, runs `write`, prints `call` again.
fn warm_then(body: &str, call: &str, warm: &str, write: &str, after: &str) {
    run_lines(
        &format!("{body}\nprint({call});\nprint({call});\n{write}\nprint({call});"),
        &[warm, warm, after],
    );
}

const RUN: &str = "function run() { try { return Math.imul(2, 3) + 1; } catch (e) { return e.constructor.name; } }";
const REPLACEMENT: &str = "{ imul() { return \"x\"; } }";

#[test]
fn a_warm_body_observes_every_write_of_its_global_binding() {
    for (write, after) in [
        (format!("globalThis[\"Ma\" + \"th\"] = {REPLACEMENT};"), "x1"),
        (format!("Math = {REPLACEMENT};"), "x1"),
        (format!("this.Math = {REPLACEMENT};"), "x1"),
        (format!("var g = globalThis; g.Math = {REPLACEMENT};"), "x1"),
        (
            format!("function set(o, k, v) {{ o[k] = v; }} set(globalThis, \"Math\", {REPLACEMENT});"),
            "x1",
        ),
        (format!("Reflect.set(globalThis, \"Math\", {REPLACEMENT});"), "x1"),
        (format!("Object.assign(globalThis, {{ Math: {REPLACEMENT} }});"), "x1"),
        (
            format!("Object.defineProperty(globalThis, \"Math\", {{ value: {REPLACEMENT} }});"),
            "x1",
        ),
        (
            format!("Reflect.defineProperty(globalThis, \"Math\", {{ value: {REPLACEMENT} }});"),
            "x1",
        ),
        (
            format!(
                "Object.defineProperties(globalThis, {{ Math: {{ value: {REPLACEMENT} }} }});"
            ),
            "x1",
        ),
        (
            "Object.defineProperty(globalThis, \"Math\", { get() { return { imul() { return \"y\"; } }; } });"
                .to_string(),
            "y1",
        ),
        (format!("(0, eval)(\"var Math = {{ imul() {{ return 'x'; }} }};\");"), "x1"),
        (
            "(0, eval)(\"function Math() {} Math.imul = function () { return 'f'; };\");".to_string(),
            "f1",
        ),
        ("delete globalThis[\"Ma\" + \"th\"];".to_string(), "ReferenceError"),
        ("delete globalThis.Math; globalThis.Math = { imul() { return \"d\"; } };".to_string(), "d1"),
        (
            "var o = { valueOf() { globalThis.Math = { imul() { return \"v\"; } }; return 1; } }; +o;"
                .to_string(),
            "v1",
        ),
    ] {
        warm_then(RUN, "run()", "7", &write, after);
    }
}

#[test]
fn a_warm_body_observes_every_edit_of_an_intrinsic_it_reads_through() {
    for (write, after) in [
        ("Math.imul = function () { return \"x\"; };", "x1"),
        ("Math[\"im\" + \"ul\"] = function () { return \"x\"; };", "x1"),
        ("var m = Math; m.imul = function () { return \"x\"; };", "x1"),
        ("Object.assign(Math, { imul() { return \"x\"; } });", "x1"),
        ("Object.defineProperty(Math, \"imul\", { value() { return \"x\"; } });", "x1"),
        (
            "Object.defineProperty(Math, \"imul\", { get() { return function () { return \"g\"; }; } });",
            "g1",
        ),
        ("Reflect.set(Math, \"imul\", function () { return \"x\"; });", "x1"),
        ("(0, eval)(\"Math.imul = function () { return 'x'; }\");", "x1"),
        ("delete Math.imul;", "TypeError"),
    ] {
        warm_then(RUN, "run()", "7", write, after);
    }
    // A constructor's own rows, and the prototypes a function fact or a
    // primitive receiver reads through.
    warm_then(
        "function run(o) { return Object.keys(o).length + 1; }",
        "run({ a: 1 })",
        "2",
        "Object[\"ke\" + \"ys\"] = function () { return { length: \"k\" }; };",
        "k1",
    );
    warm_then(
        "function run() { return \"abc\".charCodeAt(0) + 1; }",
        "run()",
        "98",
        "String.prototype[\"charCode\" + \"At\"] = function () { return \"c\"; };",
        "c1",
    );
    warm_then(
        "function run() { return Math.max.call(null, 1, 2) + 1; }",
        "run()",
        "3",
        "Function.prototype[\"ca\" + \"ll\"] = function () { return \"c\"; };",
        "c1",
    );
    warm_then(
        "function run() { return [1, 2].indexOf(2) + 1; }",
        "run()",
        "2",
        "Array.prototype[\"index\" + \"Of\"] = function () { return \"i\"; };",
        "i1",
    );
    warm_then(
        "function run() { return parseInt(\"3\") + 1; }",
        "run()",
        "4",
        "parseInt = function () { return \"p\"; };",
        "p1",
    );
}

/// Several bodies share one epoch: one write voids every proof, and a body
/// proves again once its builtins hold again.
#[test]
fn warm_bodies_follow_their_builtins_through_replacement_and_restoration() {
    run_lines(
        r#"
function imul() { return Math.imul(2, 3) + 1; }
function number() { return Number("3") + 1; }
function both() { return Math.floor(2.5) + Number("1"); }
function show() { print([imul(), number(), both()].join()); }
var math = Math, number0 = Number;
show(); show();
globalThis["Ma" + "th"] = { imul() { return "x"; }, floor() { return "f"; } };
show(); show();
globalThis["Num" + "ber"] = function () { return "n"; };
show(); show();
globalThis["Ma" + "th"] = math;
show(); show();
globalThis["Num" + "ber"] = number0;
show(); show();
var unrelated = 1; unrelated = 2; globalThis.other = 3; delete globalThis.other;
show();
"#,
        &[
            "7,4,3", "7,4,3", "x1,4,f1", "x1,4,f1", "x1,n1,fn", "x1,n1,fn", "7,n1,2n", "7,n1,2n",
            "7,4,3", "7,4,3", "7,4,3",
        ],
    );
}

/// A later Script's global lexical declaration shadows the global object's
/// property: the Environment's declarative name table is replaced.
#[test]
fn a_warm_body_observes_a_later_script_shadowing_its_builtin() {
    run_lines(
        r#"
function run() { return Math.imul(2, 3) + 1; }
print(run()); print(run());
__scriptwRealmEvalScript("let unrelated = 1;");
print(run()); print(run());
__scriptwRealmEvalScript("let Math = { imul() { return 'l'; } };");
print(run()); print(run());
"#,
        &["7", "7", "7", "7", "l1", "l1"],
    );
}

/// The epoch is one for every Realm, so a writer running in another Realm
/// voids this Realm's proofs, and each Realm's bodies prove their own
/// Realm's builtins.
#[test]
fn a_warm_body_observes_writes_made_by_another_realm() {
    run_lines(
        r#"
var other = __scriptwCreateRealm();
function run() { return Math.imul(2, 3) + 1; }
print(run()); print(run());
other.evalScript("(function (math) { math.imul = function () { return 'o'; }; })")(Math);
print(run()); print(run());
"#,
        &["7", "7", "o1", "o1"],
    );
    run_lines(
        r#"
var other = __scriptwCreateRealm();
function run() { return Math.imul(2, 3) + 1; }
print(run()); print(run());
other.evalScript("(function (global, value) { global.Math = value; })")(globalThis, { imul() { return "g"; } });
print(run()); print(run());
"#,
        &["7", "7", "g1", "g1"],
    );
    run_lines(
        r#"
var other = __scriptwCreateRealm();
other.evalScript("function run() { return Math.imul(2, 3) + 1; } var first = [run(), run()].join();");
function run() { return Math.imul(2, 3) + 1; }
print(run()); print(run());
other.evalScript("globalThis['Ma' + 'th'] = { imul() { return 'x'; } }; var second = [run(), run()].join();");
print([other.evalScript("first;"), other.evalScript("second;"), run(), run()].join());
globalThis["Ma" + "th"] = other.evalScript("Math");
print([run(), other.evalScript("run();")].join());
"#,
        &["7", "7", "7,7,x1,x1,7,7", "x1,x1"],
    );
}
