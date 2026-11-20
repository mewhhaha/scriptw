//! Valid programs that the front end or lowering used to reject, and source
//! nested beyond the implementation's limit, which must be a SyntaxError
//! instead of a native stack overflow (target/review/correctness.md 4 and 8).
//! Expected output is Node 24's.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn options() -> (CompileOptions, RunOptions) {
    (
        CompileOptions {
            host_surface_policy: HostSurfacePolicy::Test262,
            ..CompileOptions::default()
        },
        RunOptions {
            backend: ExecutionBackend::WasmAot,
            timeout_ms: Some(60_000),
            ..RunOptions::default()
        },
    )
}

fn prints(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let (compile, run) = options();
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_script(source, compile, run)
        .unwrap_or_else(|error| panic!("{error:?}\n{source}"));
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

// Exact-context clones of one function created in different specialization
// passes once both took the suffix `$exact_helper_context$1`.
#[test]
fn exact_context_clones_from_later_passes_get_distinct_identities() {
    prints(
        "var r; r = (function () { function f(o) { return o } var c = Object.create({}); return [f(c), f('s')] })(); print(typeof r[0] + ',' + r[1]);",
        "object,s",
    );
    prints(
        "var r; try { r = (function () { function f(o) { return o } var c = Object.create({}); return [f(c), f('s')] })() } catch (e) {} print(typeof r[0] + ',' + r[1]);",
        "object,s",
    );
}

// `let` and `async` are ordinary identifiers in sloppy code, also as the
// last token of the source; the observable result is the completion value.
#[test]
fn a_sloppy_script_may_end_with_let_or_async() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for (source, expected) in [
        ("var let = 1; let", "number(1)"),
        ("var let = 2; let // trailing comment", "number(2)"),
        ("var async = 3; async", "number(3)"),
        ("var async = function () { return 4 }; async()", "number(4)"),
        ("var async = 5; if (true) async", "number(5)"),
    ] {
        let (compile, run) = options();
        let outcome = Engine::new(RealmBuilder::new().build())
            .run_script(source, compile, run)
            .unwrap_or_else(|error| panic!("{error:?}\n{source}"));
        assert!(
            outcome.note.contains(expected),
            "{}\n{source}",
            outcome.note
        );
    }
}

// Each construct nests one grammar level per repetition; at the limit the
// whole pipeline compiles and runs, one level beyond is a SyntaxError.
#[test]
fn source_nesting_beyond_the_limit_is_a_syntax_error_not_a_stack_overflow() {
    const LIMIT: usize = 1000;
    let constructs: [(&str, fn(usize) -> String); 6] = [
        ("parentheses", |n| {
            format!("var x = {}1{};", "(".repeat(n), ")".repeat(n))
        }),
        ("arrays", |n| {
            format!("var x = {}{};", "[".repeat(n), "]".repeat(n))
        }),
        ("blocks", |n| format!("{}{}", "{".repeat(n), "}".repeat(n))),
        ("unary", |n| format!("var x = {}1;", "!".repeat(n))),
        ("exponent", |n| format!("var x = {}1;", "1 ** ".repeat(n))),
        ("functions", |n| {
            format!("{}{}", "function f(){".repeat(n), "}".repeat(n))
        }),
    ];
    for (name, source) in constructs {
        // The enclosing statement or expression is one level of its own.
        let fits = source(LIMIT - 2);
        let deep = source(LIMIT * 4);
        let (compile, run) = options();
        Engine::new(RealmBuilder::new().build())
            .run_script(&fits, compile, run)
            .unwrap_or_else(|error| panic!("{name} at the limit: {error:?}"));
        let (compile, run) = options();
        let error = Engine::new(RealmBuilder::new().build())
            .run_script(&deep, compile, run)
            .expect_err(name);
        assert!(
            format!("{error:?}").contains("nesting exceeds the implementation limit"),
            "{name}: {error:?}"
        );
    }
}
