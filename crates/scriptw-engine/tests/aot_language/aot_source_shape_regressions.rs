//! Valid programs that the front end or lowering used to reject, long operator
//! chains that must compile, and source nested beyond the implementation's
//! limit, which must be a SyntaxError instead of a native stack overflow.
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

// Each construct nests one grammar level per repetition; near the limit the
// whole pipeline compiles and runs, and far beyond it is a SyntaxError (the
// exact boundary is the front end's `source_nesting_limit_is_exact`).
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

fn nesting_error(source: &str) -> String {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let (compile, run) = options();
    let error = Engine::new(RealmBuilder::new().build())
        .run_script(source, compile, run)
        .expect_err("past the implementation limit");
    format!("{error:?}")
}

// Minifiers emit `+`, `||`, `&&`, `??` and comma chains with tens of
// thousands of operators (Node runs chains of a million terms). The parser
// builds them in loops, and Boa's visitors, the tree's Drop and lowering walk
// them without recursing per operator, so they cost nothing against the
// native stack; lowering splits them into bounded IR.
#[test]
fn hundred_thousand_term_chains_compile_and_run() {
    const TERMS: usize = 100_000;
    prints(
        &format!(
            "print((function () {{ var x = 1; return x{}; }})());",
            "+x".repeat(TERMS)
        ),
        "100001",
    );
    prints(&format!("print(({}1));", "0, ".repeat(TERMS)), "1");
    // Logical chains lower in the same loop; each right operand is lowered
    // as conditionally reached, which snapshots the flow facts, so they are
    // tested shorter (see the inventory's chain item).
    const LOGICAL_TERMS: usize = 500;
    prints(
        &format!(
            "print((function (t) {{ return t{} || 'end'; }})(0));",
            " || t".repeat(LOGICAL_TERMS)
        ),
        "end",
    );
    prints(
        &format!(
            "print((function (t) {{ return t{} && 'end'; }})(1));",
            " && t".repeat(LOGICAL_TERMS)
        ),
        "end",
    );
    prints(
        &format!(
            "print((function (t) {{ return t{} ?? 'end'; }})(null));",
            " ?? t".repeat(LOGICAL_TERMS)
        ),
        "end",
    );
    // Mixed precedence keeps `&&` inside `||`, left to right.
    prints(
        &format!(
            "var log = ''; function f(v) {{ log += v; return v; }} \
             var r = f(0){} || f(1) && f(2); print(log + ':' + r);",
            " || f(0) && f(9)".repeat(300)
        ),
        &format!("{}12:2", "0".repeat(301)),
    );
}

// Generator and async function bodies are lowered by resumable passes that
// still recurse per operator, so there an operator costs half a nesting level.
#[test]
fn operator_chains_in_resumable_bodies_stay_within_the_nesting_limit() {
    prints(
        &format!(
            "var g = (function* () {{ var x = 1; yield x{}; }})(); print(g.next().value);",
            "+x".repeat(1_900)
        ),
        "1901",
    );
    let error = nesting_error(&format!(
        "function* g() {{ var x = 1; yield x{}; }}",
        "+x".repeat(4_000)
    ));
    assert!(
        error.contains("nesting exceeds the implementation limit"),
        "{error}"
    );
}

// An `else if` costs two thirds of a nesting level, so more than 1,000 branches
// compile (about 1,500 fit; V8 runs about 3,000 and throws a RangeError at
// 10,000). A member or call link costs two levels, so about 500 fit: native
// compilation of both grows faster than their length.
#[test]
fn else_if_and_member_chains_are_bounded_by_their_compile_cost() {
    let else_ifs = |n: usize| {
        let mut source = String::from("var x = -1; if (x === 0) { print(0); }");
        for i in 1..n {
            source.push_str(&format!(" else if (x === {i}) {{ print({i}); }}"));
        }
        source.push_str(" else { print('end'); }");
        source
    };
    prints(&else_ifs(1_050), "end");
    let error = nesting_error(&else_ifs(1_600));
    assert!(
        error.contains("nesting exceeds the implementation limit"),
        "{error}"
    );

    let members = |n: usize| format!("var a = {{}}; a.b = a; print(a{} === a);", ".b".repeat(n));
    prints(&members(490), "true");
    let error = nesting_error(&members(600));
    assert!(
        error.contains("nesting exceeds the implementation limit"),
        "{error}"
    );
}

// A regular expression literal nests groups at most 1,000 deep; deeper is a
// SyntaxError (V8 compiles about 10,000 and rejects 20,000 with one).
#[test]
fn regular_expression_nesting_is_bounded() {
    let pattern = |n: usize| format!("{}a{}", "(".repeat(n), ")".repeat(n));
    prints(&format!("print(/{}/.test('a'));", pattern(990)), "true");
    let error = nesting_error(&format!("/{}/;", pattern(100_000)));
    assert!(
        error.contains("nest more than the implementation limit"),
        "{error}"
    );
}
