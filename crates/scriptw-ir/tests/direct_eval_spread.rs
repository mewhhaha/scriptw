use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{
    lower, EnvironmentIdentifierOperationIr, ExprIr, PreparedScriptKind, PreparedScriptOutcome,
    ProgramIr, StatementIr,
};

fn prepare(source: &str) -> ProgramIr {
    let program = lower(&parse(source, ParseOptions::script()).unwrap());
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    program
}

fn assert_prepared(program: &ProgramIr, text: &str) {
    let script = program.script.as_ref().unwrap();
    for direct in [false, true] {
        assert!(
            script.prepared_scripts.iter().any(|source| {
                source.source == text
                    && matches!(source.kind, PreparedScriptKind::DirectEval(_)) == direct
                    && matches!(source.outcome, PreparedScriptOutcome::Executable(_))
            }),
            "missing source {text:?}, direct={direct}"
        );
    }
}

#[test]
fn an_empty_leading_spread_keeps_the_following_source_and_runtime_spread() {
    let program = prepare(
        "var iterable = { [Symbol.iterator]() { return { next() { return { done: true }; } }; } };\n\
         eval(...iterable, '23;', 'not a source argument');",
    );
    assert_prepared(&program, "23;");
    let script = program.script.as_ref().unwrap();
    assert!(!script
        .prepared_scripts
        .iter()
        .any(|source| source.source == "not a source argument"));
    let Some(StatementIr::Expression(expression)) = script.body.statements.last() else {
        panic!("call remains a statement");
    };
    let ExprIr::EnvironmentIdentifier(identifier) = &expression.expr else {
        panic!("callee Reference remains runtime-owned");
    };
    let EnvironmentIdentifierOperationIr::Call {
        args,
        direct_eval: Some(_),
        ..
    } = &identifier.operation
    else {
        panic!("direct eval context remains on the original call");
    };
    assert_eq!(args.len(), 3);
    assert!(matches!(args[0].expr, ExprIr::SpreadArgument(_)));
}

#[test]
fn assigned_iterator_methods_follow_finite_indexed_source_values() {
    for strict in ["", "'use strict';"] {
        let program = prepare(&format!(
            r#"
{strict}
var texts = ['value = 1;', 'value = 2;'];
var index = 0;
var iterable = {{}};
iterable[Symbol.iterator] = function() {{
    return {{ next: function() {{
        var i = index++;
        if (i < texts.length) return {{ done: false, value: texts[i] }};
        return {{ done: true }};
    }} }};
}};
function caller() {{ var value = 0; return eval(...iterable); }}
caller();
"#
        ));
        assert_prepared(&program, "value = 1;");
        assert_prepared(&program, "value = 2;");
    }
}

#[test]
fn getters_contribute_candidates_without_replacing_their_runtime_calls() {
    let program = prepare(
        r#"
var texts = ['31;'];
var iterable = {
    get [Symbol.iterator]() { return function() {
        return { get next() { return function() {
            return { done: false, get value() { return texts[0]; } };
        }; } };
    }; }
};
eval(...iterable);
"#,
    );
    assert_prepared(&program, "31;");
}

#[test]
fn iterator_source_candidates_survive_observable_global_gets() {
    for iterator in [
        "var iterable = { get [Symbol.iterator]() { return iteratorMethod; } };",
        "var iterable = {}; iterable[Symbol.iterator] = iteratorMethod;",
    ] {
        let program = prepare(&format!(
            r#"
var original = eval;
Object.defineProperty(globalThis, 'eval', {{
    configurable: true, get() {{ return original; }}
}});
var texts = ['value = 1;', 'value = 2;'];
var index = 0;
var iteratorMethod = function() {{
    return {{ get next() {{ return function() {{
        var i = index++;
        return {{
            get done() {{ return i >= texts.length; }},
            get value() {{ return texts[i]; }}
        }};
    }}; }} }};
}};
{iterator}
function caller() {{ var value = 0; eval(...iterable); return value; }}
caller();
"#
        ));
        assert_prepared(&program, "value = 1;");
        assert_prepared(&program, "value = 2;");
    }
}

#[test]
fn runtime_generated_iterator_source_does_not_gain_a_prepared_unit() {
    let program = prepare(
        r#"
var iterable = { [Symbol.iterator]() {
    return { next() { return { done: false, value: String.fromCharCode(52, 50) }; } };
} };
eval(...iterable);
"#,
    );
    assert!(!program
        .script
        .unwrap()
        .prepared_scripts
        .iter()
        .any(|source| source.source == "42"));
}

#[test]
fn finite_spread_discovery_obeys_the_candidate_limit() {
    let texts = (0..257)
        .map(|index| format!("'{index};'"))
        .collect::<Vec<_>>()
        .join(",");
    let program = prepare(&format!("var texts = [{texts}]; eval(...texts);"));
    assert!(program.script.unwrap().prepared_scripts.is_empty());
}
