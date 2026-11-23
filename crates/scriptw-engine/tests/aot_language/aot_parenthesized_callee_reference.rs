use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, RealmBuilder, RunOptions,
};

fn run_boolean(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let engine = Engine::new(RealmBuilder::new().build());
    let outcome = engine
        .run_script(
            source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("parenthesized callees should execute through Wasm");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

// ECMA-262 13.2.1.1: a ParenthesizedExpression evaluates to its operand's
// Reference, so EvaluateCall takes thisValue from the property reference's
// base and `(eval)(src)` is a direct eval. An optional chain keeps its own
// parentheses: `(a?.b)()` with nullish `a` throws instead of short-circuiting.
#[test]
fn parenthesized_member_callees_keep_their_this_and_direct_eval() {
    run_boolean(
        r#"
var obj = { m() { return this === obj; } };
var arr = [function () { return this === arr; }];
var deep = { m() { return this; } };
var local = "outer";
function evalScope() { var local = "inner"; return (eval)("local"); }
var nullish = null;
var threw = false;
try { (nullish?.m)(); } catch (error) { threw = error instanceof TypeError; }
var comma = (0, obj.m)();
(obj.m)() && (obj["m"])() && (arr[0])() && ((obj.m))() && (deep.m)() === deep
  && evalScope() === "inner" && threw && comma === false
"#,
    );
}
