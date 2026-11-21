//! A coercion of an object (ToString, ToNumeric, ToPrimitive, ToPropertyKey)
//! runs its `@@toPrimitive`, `toString` or `valueOf`, which is user code. The
//! lowering's facts about captured bindings and heap shapes must not survive
//! it: each case below kept `label` Number-kinded and `a.p` at its old shape
//! past the coercion and printed `1 1`. Expected output is Node 24's.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn prints(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let compile = CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    };
    let run = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(60_000),
        ..RunOptions::default()
    };
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

/// `label` is a captured `let` and `a.p` a property; `hit` changes both. A
/// statement wrapper (`function`, `async function`, `function*`) runs
/// `statement` with `o` coercible, then reads both.
fn facts_after(wrapper: &str, statement: &str) -> String {
    let body = format!(
        "let label = 1; const a = {{ p: 1 }}; \
         function hit() {{ label = 'changed'; a.p = 'x'; }} \
         const o = {{ toString() {{ hit(); return ''; }}, valueOf() {{ hit(); return 0; }} }}; \
         {statement}; const r = (label + 1) + ' ' + (a.p + 1);"
    );
    match wrapper {
        "function" => format!("function outer() {{ {body} return r; }} print(outer());"),
        "async" => format!("async function outer() {{ {body} return r; }} outer().then(print);"),
        "generator" => format!(
            "function* outer() {{ {body} return r; }} \
             const it = outer(); it.next(); print(it.next('y').value);"
        ),
        "script" => format!("{body} print(r);"),
        _ => unreachable!("{wrapper}"),
    }
}

const CHANGED: &str = "changed1 x1";

#[test]
fn template_substitutions_invalidate_flow_facts() {
    // The review's reproducer, at a script's top level.
    prints(
        "let label = 1; const o = { toString() { label = 'changed'; return ''; } }; \
         `${o}`; print(label + 1);",
        "changed1",
    );
    let one = "${1}";
    for (wrapper, statement) in [
        ("function", "const s = `${o}`".to_owned()),
        ("function", "const s = `${o}x`".to_owned()),
        ("script", "`${o}`".to_owned()),
        // A nested template's ToString and the outer one's.
        ("function", "const s = `a${`b${o}`}c`".to_owned()),
        ("function", "const s = true ? `${o}` : 0".to_owned()),
        // Past the first 256-part segment, which the chain binds to a
        // temporary.
        (
            "function",
            format!("const s = `{}${{o}}{}`", one.repeat(600), one.repeat(300)),
        ),
        // Facts re-established after the suspension, then a substitution.
        (
            "async",
            "const s = `${await 1}${(label = 1, a.p = 1, o)}`".to_owned(),
        ),
        (
            "async",
            format!(
                "const s = `{}${{await 1}}{}${{(label = 1, a.p = 1, o)}}{}`",
                one.repeat(300),
                one.repeat(300),
                one.repeat(300)
            ),
        ),
        // A generator's template assignment folds into statements.
        (
            "generator",
            "let s; s = `${yield 1}${(label = 1, a.p = 1, o)}`".to_owned(),
        ),
    ] {
        prints(&facts_after(wrapper, &statement), CHANGED);
    }
}

#[test]
fn identifier_compound_additions_and_updates_invalidate_flow_facts() {
    for statement in [
        "let p = o; p += (label = 1, a.p = 1, 1)",
        "var p = o; p += 1",
        "let p = o; p++",
        "let p = o; --p",
        // The update's TypeError follows ToNumeric's `valueOf`.
        "const p = o; try { p++; } catch (e) {}",
    ] {
        prints(&facts_after("function", statement), CHANGED);
    }
}

#[test]
fn computed_object_literal_keys_invalidate_flow_facts() {
    for statement in [
        "const t = { [o]: 1 }",
        "const t = { [o]() {} }",
        "const t = { get [o]() { return 1; } }",
    ] {
        prints(&facts_after("function", statement), CHANGED);
    }
}

// Builtins whose argument or receiver coercion runs user code, which the
// builtin catalog did not mark.
#[test]
fn coercing_builtin_calls_invalidate_flow_facts() {
    for statement in [
        "Symbol.for(o)",
        "Date.UTC(o)",
        "Date.parse(o)",
        "String.fromCodePoint(o)",
        "/a/.exec(o)",
        "/a/.test(o)",
        "/a/[Symbol.split](o)",
        "/a/.compile(o)",
    ] {
        prints(&facts_after("function", statement), CHANGED);
    }
}
