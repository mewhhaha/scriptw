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
    // as conditionally reached, between two snapshots of the flow facts. A
    // sloppy function called with the global `this` once copied the global
    // object's whole shape into each (10,000 links took 177 s); it is shared
    // now (see the inventory's chain item).
    const LOGICAL_TERMS: usize = 10_000;
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

// A template literal's parts are folded like a chain's operands, so a flat
// template of any length lowers to bounded IR: each `${}` nested the IR one
// level deeper, and 30,000 substitutions overflowed the emitter's stack after
// a quadratic scan (100,000 ran for more than 15 minutes); Node runs 100,000.
// Each substitution still emits about 500 Wasm bytes into the function body,
// so 100,000 exceed the validator's body size limit, a compile error the
// inventory's compile memory item owns (split large bodies).
#[test]
fn hundred_thousand_substitution_templates_lower_to_bounded_ir() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let source = format!(
        "print((function (x) {{ return `{}`; }})('a').length);",
        "${x}".repeat(100_000)
    );
    let (compile, run) = options();
    let error = Engine::new(RealmBuilder::new().build())
        .run_script(&source, compile, run)
        .expect_err("one function body of 100,000 substitutions exceeds the Wasm body size limit");
    assert!(
        format!("{error:?}").contains("function body size count exceeds limit"),
        "{error:?}"
    );
}

// Each substitution is evaluated and ToString'd (hint String) before the next
// one, also across the temporaries a template longer than 256 parts is split
// with, and around an `await`; a tagged template's object is created once per
// site and holds every part.
#[test]
fn long_templates_keep_their_evaluation_order() {
    const PARTS: usize = 300;
    let parts = |range: std::ops::Range<usize>| -> String {
        range.map(|i| format!("${{f({i})}}")).collect()
    };
    let order =
        |range: std::ops::Range<usize>| -> String { range.map(|i| format!("f{i}t{i}")).collect() };
    // `valueOf` (hint Default) is never called.
    let helpers = "var log = ''; function f(v) { log += 'f' + v; \
                   return { toString() { log += 't' + v; return 'x'; }, valueOf() { return 'WRONG'; } }; }";
    prints(
        &format!(
            "{helpers} var s = `{}`; print(s.length + ':' + log);",
            parts(0..PARTS)
        ),
        &format!("{PARTS}:{}", order(0..PARTS)),
    );
    // An `await` after the first 150 parts: those are evaluated before it.
    prints(
        &format!(
            "{helpers} (async function () {{ var s = `{}${{(log += 'A', await 'w')}}{}`; \
             print(s.length + ':' + log); }})();",
            parts(0..PARTS / 2),
            parts(PARTS / 2..PARTS)
        ),
        &format!(
            "{}:{}A{}",
            PARTS + 1,
            order(0..PARTS / 2),
            order(PARTS / 2..PARTS)
        ),
    );
    // A class key is lowered in async expression lowering, which pins the
    // parts before the `await`.
    prints(
        &format!(
            "{helpers} (async function () {{ class C {{ static [`{}${{(log += 'A', await 'w')}}{}`]() {{}} }} \
             print(Object.getOwnPropertyNames(C).length + ':' + log); }})();",
            parts(0..PARTS / 2),
            parts(PARTS / 2..PARTS)
        ),
        &format!("4:{}A{}", order(0..PARTS / 2), order(PARTS / 2..PARTS)),
    );
    prints(
        &format!(
            "var first; function t(s, ...a) {{ first = first || s; \
             return s.length + ':' + s.raw.length + ':' + a.length + ':' + (s === first); }} \
             function g(x) {{ return t`{}`; }} g(1); print(g(2));",
            "${x}".repeat(PARTS)
        ),
        &format!("{0}:{0}:{PARTS}:true", PARTS + 1),
    );
    prints(
        &format!(
            "print((function (x) {{ return String.raw`{}`; }})('a').length);",
            "${x}\\n".repeat(PARTS)
        ),
        &(3 * PARTS).to_string(),
    );
}

fn module_prints(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let (compile, run) = options();
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_module(source, compile, run)
        .unwrap_or_else(|error| panic!("{error:?}"));
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

// In generator and async function bodies, the resumable lowering and state
// planners recurse per operator once a chain's operand suspends, so there such
// a chain's operators cost half a nesting level each. A chain that does not
// suspend costs what it costs anywhere: an ES module's top-level code, which
// module lowering compiles inside an async function, compiled only about 2,000
// operators per chain when every chain there paid.
#[test]
fn operator_chains_in_resumable_bodies_pay_only_when_they_suspend() {
    prints(
        &format!(
            "var g = (function* () {{ var x = 1; yield x{}; }})(); print(g.next().value);",
            "+x".repeat(1_900)
        ),
        "1901",
    );
    // `yield`'s operand chain does not suspend; one whose operand does pays.
    prints(
        &format!(
            "var g = (function* () {{ var x = 1; yield x{}; }})(); print(g.next().value);",
            "+x".repeat(4_000)
        ),
        "4001",
    );
    let error = nesting_error(&format!(
        "function* g() {{ var x = 1; (yield x){}; }}",
        "+x".repeat(4_000)
    ));
    assert!(
        error.contains("with an `await` or `yield` operand"),
        "{error}"
    );
    // Comma chains of literals keep the native compile cheap (a long `+`
    // chain of top-level bindings compiles slowly; see the inventory).
    prints(
        &format!(
            "var g = (function* () {{ var y = (0{}, 5); yield y; }})(); print(g.next().value);",
            ", 0".repeat(20_000)
        ),
        "5",
    );
    module_prints(
        &format!("export const n = (0{}, 7); print(n);", ", 0".repeat(20_000)),
        "7",
    );
    module_prints(
        &format!(
            "const y = await Promise.resolve(7); print((0{}, y));",
            ", 0".repeat(20_000)
        ),
        "7",
    );
}

// Class declarations lower their heritage and computed keys in async
// expression lowering, which pins operands before a suspension. A chain there
// that does not suspend is lowered by the chain loop like any other.
#[test]
fn long_chains_in_class_keys_and_heritage_compile() {
    // Comma chains of literals keep the native compile cheap.
    prints(
        &format!(
            "class C {{ [(0{}, 'm')]() {{ return 7; }} }} print(new C().m());",
            ", 0".repeat(100_000)
        ),
        "7",
    );
    prints(
        &format!(
            "class B {{}} class C extends (0{}, B) {{}} print(new C() instanceof B);",
            ", 0".repeat(100_000)
        ),
        "true",
    );
    prints(
        &format!(
            "var k = 'a'; var o = {{ [k]: class {{ static [(0{}, k)]() {{ return 3; }} }} }}; print(o.a.a());",
            ", 0".repeat(50_000)
        ),
        "3",
    );
}

// Lowering binds a chain's value to a temporary every 256 operators and right
// after an operand whose own chains are deep, and arranges the temporaries as
// a tree; evaluation order and values are those of the plain left-deep chain.
#[test]
fn nested_chains_split_into_temporaries_keep_their_evaluation_order() {
    for (levels, links) in [(12usize, 80usize), (3, 600), (40, 70)] {
        let mut expression = String::from("f(0)");
        for level in 1..=levels {
            expression = format!(
                "(f({level}) + {expression}{})",
                format!(" + f({level})").repeat(links - 1)
            );
        }
        // f(levels) … f(1), f(0), then each level's remaining operands from
        // the innermost out.
        let mut order: Vec<usize> = (0..=levels).rev().collect();
        for level in 1..=levels {
            order.extend(std::iter::repeat_n(level, links - 1));
        }
        let hash = order
            .iter()
            .fold(0u64, |hash, value| (hash * 31 + *value as u64) % 1_000_003);
        let sum = links * levels * (levels + 1) / 2;
        prints(
            &format!(
                "var h = 0; function f(v) {{ h = (h * 31 + v) % 1000003; return v; }} \
                 var r = {expression}; print(r + ':' + h);"
            ),
            &format!("{sum}:{hash}"),
        );
    }
}

// An `else if` costs two thirds of a nesting level, so more than 1,000 branches
// compile (about 1,500 fit; V8 runs about 3,000 and throws a RangeError at
// 10,000). A property access, call or optional link costs two levels, so about
// 500 fit: native compilation of a property access chain grows quadratically.
// A method call's two links cost half a level each, so about 1,000 fit, as
// builder and `Map#set` chains need (Node runs 3,000; native compilation of
// such a chain grows superlinearly, see the inventory).
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
    // The deepest branch's `print(` call may be what crosses the limit.
    let error = nesting_error(&else_ifs(1_600));
    assert!(
        error.contains("nesting exceeds the implementation limit"),
        "{error}"
    );

    let members = |n: usize| format!("var a = {{}}; a.b = a; print(a{} === a);", ".b".repeat(n));
    prints(&members(490), "true");
    let error = nesting_error(&members(600));
    assert!(error.contains("property access"), "{error}");

    prints(
        &format!(
            "var o = {{ m() {{ return this; }} }}; print(o{} === o);",
            ".m()".repeat(900)
        ),
        "true",
    );
    prints(
        &format!(
            "print(new Map(){}.size);",
            (0..500)
                .map(|i| format!(".set({i}, {i})"))
                .collect::<String>()
        ),
        "500",
    );
    let error = nesting_error(&format!("var o = {{}}; o{};", ".m()".repeat(1_100)));
    assert!(error.contains("method call"), "{error}");
}

// `(a0, a1, …) =>` is covered by a comma chain, converted to parameters in a
// loop; more than 65,535 parameters is a SyntaxError (Node 24's V8 accepts at
// most 65,525).
#[test]
fn arrow_parameter_lists_are_bounded() {
    let parameters = |count: usize| {
        (0..count)
            .map(|index| format!("a{index}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let (compile, run) = options();
    let error = Engine::new(RealmBuilder::new().build())
        .run_script(
            &format!("var f = ({}) => 0;", parameters(300_000)),
            compile,
            run,
        )
        .expect_err("too many parameters");
    assert!(
        format!("{error:?}").contains("too many formal parameters"),
        "{error:?}"
    );
    prints(
        &format!(
            "var f = ({}) => a1; print(f.length + ':' + f(0, 5));",
            parameters(5_000)
        ),
        "5000:5",
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
    // A pattern string reaches the compile-time pattern compiler through the
    // `RegExp` constructor; one nested past the limit is left to the runtime
    // compiler, whose resource limit is a RangeError.
    let classes = |n: usize| format!("{}a{}", "[".repeat(n), "]".repeat(n));
    prints(
        &format!("print(new RegExp('{}', 'v').test('a'));", classes(900)),
        "true",
    );
    prints(
        &format!(
            "try {{ new RegExp('{}', 'v'); print('compiled'); }} catch (e) {{ print(e.name); }}",
            classes(100_000)
        ),
        "RangeError",
    );
}
