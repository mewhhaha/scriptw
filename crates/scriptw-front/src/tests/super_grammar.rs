use super::*;

#[test]
fn script_top_level_super_rejects_each_pinned_contains_boundary() {
    for source in SCRIPT_BODY_SUPER_SOURCES {
        let err = parse(source, ParseOptions::script())
            .expect_err("ScriptBody Contains super should fail");
        assert_eq!(
            err.diagnostic().phase(),
            ParseDiagnosticPhase::Early,
            "{source:?}: {err:?}"
        );
        assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
        assert_eq!(
            err.diagnostic().code,
            early(EarlyErrorCode::ScriptTopLevelSuper),
            "{source:?}: {err:?}"
        );
        let span = err
            .diagnostic()
            .span
            .expect("the fixed ScriptBody position must retain a source span");
        assert!(span.start < span.end, "{source:?}: {err:?}");
    }
}

#[test]
fn module_top_level_super_keeps_its_distinct_goal_owner() {
    for source in SCRIPT_BODY_SUPER_SOURCES {
        let err = parse(source, ParseOptions::module())
            .expect_err("ModuleItemList Contains super should fail");
        assert_eq!(
            err.diagnostic().phase(),
            ParseDiagnosticPhase::Early,
            "{source:?}: {err:?}"
        );
        assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
        assert_eq!(
            err.diagnostic().code,
            early(EarlyErrorCode::ModuleTopLevelSuper),
            "{source:?}: {err:?}"
        );
        let span = err
            .diagnostic()
            .span
            .expect("the fixed Module position must retain a source span");
        assert!(span.start < span.end, "{source:?}: {err:?}");
    }
}

#[test]
fn method_owned_super_forms_remain_valid_under_both_goals() {
    for source in [
        "class Base {}; class Derived extends Base { constructor() { super(); } }",
        "class Base {}; class Derived extends Base { method() { return super.value; } }",
        "class Base {}; class Derived extends Base { method() { return () => super.value; } }",
        "class Base {}; class Derived extends Base { field = super.value; }",
        "class Base {}; class Derived extends Base { static field = super.value; }",
        "class Base {}; class Derived extends Base { static { void super.value; } }",
        "({ method() { return super.value; } });",
        "({ get value() { return super.value; } });",
        "({ set value(input) { void super.value; } });",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("method-owned super references should remain parse-valid");
        }
    }
}

#[test]
fn base_constructor_direct_super_rejections_cover_both_forms_inputs_arrows_and_goals() {
    for source in [
        "class C { constructor() { super(); } }",
        "(class { constructor() { super(); } });",
        "class C { constructor(value = super()) {} }",
        "class C { constructor() { (() => super())(); } }",
        "class C { constructor(value = (() => super())()) {} }",
        "class C { constructor() { (() => () => super())()(); } }",
        "class C { constructor() { (async () => super())(); } }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err =
                parse(source, options).expect_err("a base constructor HasDirectSuper should fail");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::ClassBaseConstructorHasDirectSuper),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the class-owned rejection must retain a source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn class_static_block_super_call_rejections_cover_forms_heritage_arrows_and_goals() {
    for source in [
        "class C { static { super(); } }",
        "(class { static { super(); } });",
        "class B {}; class C extends B { static { super(); } }",
        "class C { static { { super(); } } }",
        "class C { static { (() => super())(); } }",
        "class C { static { ((value = super()) => value)(); } }",
        "class C { static { (() => () => super())()(); } }",
        "class C { static { (async () => super())(); } }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("a class static block Contains SuperCall should fail");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::ClassStaticBlockContainsSuperCall),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the class-owned rejection must retain a source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn class_field_initializer_super_call_rejections_cover_every_field_shape_and_goal() {
    for source in [
        "class C { field = super(); }",
        "class C { static field = super(); }",
        "class C { #field = super(); }",
        "class C { static #field = super(); }",
        "class C { accessor field = super(); }",
        "class C { static accessor field = super(); }",
        "(class { field = super(); });",
        "class B {}; class C extends B { field = super(); }",
        "class C { field = () => super(); }",
        "class C { static #field = async () => super(); }",
        "class C { field = () => () => super(); }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("a class field initializer Contains SuperCall should fail");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::ClassFieldInitializerContainsSuperCall),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the field-owned rejection must retain a source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn function_expression_super_rejections_cover_parameters_bodies_arrows_and_goals() {
    for source in [
        "(function() { super(); });",
        "(function() { void super.value; });",
        "(function(value = super()) {});",
        "(function(value = super.value) {});",
        "(function named() { super(); });",
        "(function named(value = super.value) {});",
        "(function() { (() => super())(); });",
        "(function(value = (() => super.value)()) {});",
        "(function() { (async () => super())(); });",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("FunctionExpression parameters and body may not contain super");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::FunctionExpressionContainsSuper),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the function-expression rejection must retain a source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn function_expression_super_positive_boundaries_remain_valid_under_both_goals() {
    for source in [
        "(function() {});",
        "(function named(value) { return value; });",
        "(function() { class D extends Object { constructor() { super(); } } });",
        "(function() { return \"super() and super.value\"; });",
        "class B {}; class C extends B { method() { return function() {}; } }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("nested class ownership and ordinary text must remain parse-valid");
        }
    }
}

#[test]
fn function_expression_super_precedence_preserves_prior_callable_checks() {
    for (source, code) in [
        (
            "(function(value = super(), value) {});",
            EarlyErrorCode::DuplicateFormalParameter,
        ),
        (
            "(function(value = super()) { \"use strict\"; });",
            EarlyErrorCode::CallableNonSimpleParametersContainUseStrict,
        ),
        (
            "(function(value = super()) { let value; });",
            EarlyErrorCode::DuplicateLexicalDeclaration,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("an earlier FunctionExpression check should own the rejection");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn function_expression_super_classifier_is_anchored_and_injection_safe() {
    assert_eq!(
        classify_parse_failure("function expression cannot contain super at line 1, col 11")
            .map(ParseClassified::code),
        Some(EarlyErrorCode::FunctionExpressionContainsSuper)
    );
    for message in [
        "invalid super usage at line 1, col 11",
        "invalid super call usage at line 1, col 11",
    ] {
        assert_eq!(classify_parse_failure(message), None);
    }

    let exported_name = "function expression cannot contain super at line";
    let source = format!(
        "const value = 0; export {{ value as \"{exported_name}\" }}; export {{ value as \"{exported_name}\" }};"
    );
    let err = parse(&source, ParseOptions::module())
        .expect_err("the user-chosen exported name is duplicated");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ModuleDuplicateExport),
        "{err:?}"
    );
}

#[test]
fn function_declaration_super_rejections_cover_parameters_bodies_arrows_and_goals() {
    for source in [
        "function invalid() { super(); }",
        "function invalid() { void super.value; }",
        "function invalid(value = super()) {}",
        "function invalid(value = super.value) {}",
        "function invalid() { (() => super())(); }",
        "function invalid(value = (() => super.value)()) {}",
        "function invalid() { (async () => super())(); }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("FunctionDeclaration parameters and body may not contain super");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::FunctionDeclarationContainsSuper),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the function-declaration rejection must retain a source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn anonymous_default_function_declaration_super_rejects_under_module_goal() {
    let source = "export default function(value = super()) {}";
    let err = parse(source, ParseOptions::module())
        .expect_err("an anonymous default FunctionDeclaration may not contain super");
    assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
    assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::FunctionDeclarationContainsSuper),
        "{err:?}"
    );
    let span = err
        .diagnostic()
        .span
        .expect("the default FunctionDeclaration rejection must retain a source span");
    assert!(span.start < span.end, "{source:?}: {err:?}");
}

#[test]
fn function_declaration_super_positive_boundaries_remain_valid_under_both_goals() {
    for source in [
        "function valid() {}",
        "function valid(value) { return value; }",
        "function valid() { function nested() {} }",
        "function valid() { class D extends Object { constructor() { super(); } } }",
        "function valid() { return \"super() and super.value\"; }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("nested ownership and ordinary text must remain parse-valid");
        }
    }
}

#[test]
fn function_declaration_super_precedence_preserves_prior_callable_checks() {
    for (source, code) in [
        (
            "function invalid(value = super(), value) {}",
            EarlyErrorCode::DuplicateFormalParameter,
        ),
        (
            "function invalid(value = super()) { \"use strict\"; }",
            EarlyErrorCode::CallableNonSimpleParametersContainUseStrict,
        ),
        (
            "function invalid(value = super()) { let value; }",
            EarlyErrorCode::DuplicateLexicalDeclaration,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("an earlier FunctionDeclaration check should own the rejection");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn function_declaration_super_classifier_is_anchored_and_injection_safe() {
    for (message, expected) in [
        (
            "function declaration cannot contain super at line 1, col 12",
            Some(EarlyErrorCode::FunctionDeclarationContainsSuper),
        ),
        (
            "function expression cannot contain super at line 1, col 11",
            Some(EarlyErrorCode::FunctionExpressionContainsSuper),
        ),
        ("invalid super usage at line 1, col 12", None),
        ("invalid super call usage at line 1, col 12", None),
    ] {
        assert_eq!(
            classify_parse_failure(message).map(ParseClassified::code),
            expected
        );
    }

    let exported_name = "function declaration cannot contain super at line";
    let source = format!(
        "const value = 0; export {{ value as \"{exported_name}\" }}; export {{ value as \"{exported_name}\" }};"
    );
    let err = parse(&source, ParseOptions::module())
        .expect_err("the user-chosen exported name is duplicated");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ModuleDuplicateExport),
        "{err:?}"
    );
}

#[test]
fn async_function_declaration_super_rejections_cover_parameters_bodies_arrows_and_goals() {
    for source in [
        "async function invalid() { super(); }",
        "async function invalid() { void super.value; }",
        "async function invalid(value = super()) {}",
        "async function invalid(value = super.value) {}",
        "async function invalid() { (() => super())(); }",
        "async function invalid(value = (() => super.value)()) {}",
        "async function invalid() { (async () => super())(); }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("AsyncFunctionDeclaration parameters and body may not contain super");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::AsyncFunctionDeclarationContainsSuper),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the async-function-declaration rejection must retain a source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn anonymous_default_async_function_declaration_super_rejects_under_module_goal() {
    let source = "export default async function(value = super()) {}";
    let err = parse(source, ParseOptions::module())
        .expect_err("an anonymous default AsyncFunctionDeclaration may not contain super");
    assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
    assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::AsyncFunctionDeclarationContainsSuper),
        "{err:?}"
    );
    let span = err
        .diagnostic()
        .span
        .expect("the default AsyncFunctionDeclaration rejection must retain a source span");
    assert!(span.start < span.end, "{source:?}: {err:?}");
}

#[test]
fn async_function_declaration_super_positive_boundaries_remain_valid_under_both_goals() {
    for source in [
        "async function valid() {}",
        "async function valid(value) { return await value; }",
        "async function valid() { function nested() {} }",
        "async function valid() { class D extends Object { constructor() { super(); } } }",
        "async function valid() { return \"super() and super.value\"; }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("nested ownership and ordinary text must remain parse-valid");
        }
    }
}

#[test]
fn async_function_declaration_super_stops_at_nested_callable_and_class_owners() {
    for (source, code) in [
        (
            "async function outer() { function inner() { super(); } }",
            EarlyErrorCode::FunctionDeclarationContainsSuper,
        ),
        (
            "async function outer(value = function() { return super.value; }) {}",
            EarlyErrorCode::FunctionExpressionContainsSuper,
        ),
        (
            "async function outer() { class Inner { constructor() { super(); } } }",
            EarlyErrorCode::ClassBaseConstructorHasDirectSuper,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("the nested production must retain its own super restriction");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn async_function_declaration_super_precedence_preserves_shared_callable_check_order() {
    for (source, code) in [
        (
            "async function invalid(value = super(), value) {}",
            EarlyErrorCode::DuplicateFormalParameter,
        ),
        (
            "async function invalid(value = super()) { \"use strict\"; }",
            EarlyErrorCode::CallableNonSimpleParametersContainUseStrict,
        ),
        (
            "async function invalid(value = super()) { let value; }",
            EarlyErrorCode::DuplicateLexicalDeclaration,
        ),
        (
            "async function invalid(value = (await 0, super())) {}",
            EarlyErrorCode::AsyncFunctionDeclarationContainsSuper,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("the shared AsyncFunctionDeclaration check order must be retained");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn async_function_declaration_super_classifier_is_anchored_and_injection_safe() {
    for (message, expected) in [
        (
            "async function declaration cannot contain super at line 1, col 18",
            Some(EarlyErrorCode::AsyncFunctionDeclarationContainsSuper),
        ),
        (
            "function declaration cannot contain super at line 1, col 12",
            Some(EarlyErrorCode::FunctionDeclarationContainsSuper),
        ),
        (
            "async function expression cannot contain super at line 1, col 17",
            Some(EarlyErrorCode::AsyncFunctionExpressionContainsSuper),
        ),
        ("invalid super usage at line 1, col 18", None),
        ("invalid super call usage at line 1, col 18", None),
    ] {
        assert_eq!(
            classify_parse_failure(message).map(ParseClassified::code),
            expected
        );
    }

    let exported_name = "async function declaration cannot contain super at line";
    let source = format!(
        "const value = 0; export {{ value as \"{exported_name}\" }}; export {{ value as \"{exported_name}\" }};"
    );
    let err = parse(&source, ParseOptions::module())
        .expect_err("the user-chosen exported name is duplicated");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ModuleDuplicateExport),
        "{err:?}"
    );
}

#[test]
fn async_function_expression_super_rejections_cover_parameters_bodies_arrows_and_goals() {
    for source in [
        "(async function() { super(); });",
        "(async function() { void super.value; });",
        "(async function(value = super()) {});",
        "(async function(value = super.value) {});",
        "(async function named() { super(); });",
        "(async function named(value = super.value) {});",
        "(async function() { (() => super())(); });",
        "(async function(value = (() => super.value)()) {});",
        "(async function() { (async () => super())(); });",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("AsyncFunctionExpression parameters and body may not contain super");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::AsyncFunctionExpressionContainsSuper),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the async-function-expression rejection must retain a source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn async_function_expression_super_positive_boundaries_remain_valid_under_both_goals() {
    for source in [
        "(async function() {});",
        "(async function named(value) { return await value; });",
        "(async function() { function nested() {} });",
        "(async function() { class D extends Object { constructor() { super(); } } });",
        "(async function() { return \"super() and super.value\"; });",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("nested ownership and ordinary text must remain parse-valid");
        }
    }
}

#[test]
fn async_function_expression_super_precedence_preserves_prior_callable_checks() {
    for (source, code) in [
        (
            "(async function(value = (await 0, super())) {});",
            EarlyErrorCode::AsyncFunctionExpressionParametersContainAwait,
        ),
        (
            "(async function(value = super(), value) {});",
            EarlyErrorCode::DuplicateFormalParameter,
        ),
        (
            "(async function(value = super()) { \"use strict\"; });",
            EarlyErrorCode::CallableNonSimpleParametersContainUseStrict,
        ),
        (
            "(async function(value = super()) { let value; });",
            EarlyErrorCode::DuplicateLexicalDeclaration,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("an earlier AsyncFunctionExpression check should own the rejection");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn async_function_expression_super_classifier_is_anchored_and_injection_safe() {
    for (message, expected) in [
        (
            "async function expression cannot contain super at line 1, col 17",
            Some(EarlyErrorCode::AsyncFunctionExpressionContainsSuper),
        ),
        (
            "function expression cannot contain super at line 1, col 11",
            Some(EarlyErrorCode::FunctionExpressionContainsSuper),
        ),
        (
            "function declaration cannot contain super at line 1, col 12",
            Some(EarlyErrorCode::FunctionDeclarationContainsSuper),
        ),
        ("invalid super usage at line 1, col 17", None),
        ("invalid super call usage at line 1, col 17", None),
    ] {
        assert_eq!(
            classify_parse_failure(message).map(ParseClassified::code),
            expected
        );
    }

    let exported_name = "async function expression cannot contain super at line";
    let source = format!(
        "const value = 0; export {{ value as \"{exported_name}\" }}; export {{ value as \"{exported_name}\" }};"
    );
    let err = parse(&source, ParseOptions::module())
        .expect_err("the user-chosen exported name is duplicated");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ModuleDuplicateExport),
        "{err:?}"
    );
}

#[test]
fn generator_expression_super_rejections_cover_parameters_bodies_arrows_and_goals() {
    for source in [
        "(function*() { super(); });",
        "(function*() { void super.value; });",
        "(function*(value = super()) {});",
        "(function*(value = super.value) {});",
        "(function* named() { super(); });",
        "(function* named(value = super.value) {});",
        "(function*() { (() => super())(); });",
        "(function*(value = (() => super.value)()) {});",
        "(function*() { (async () => super())(); });",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("GeneratorExpression parameters and body may not contain super");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::GeneratorExpressionContainsSuper),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the generator-expression rejection must retain a source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn generator_expression_super_positive_boundaries_remain_valid_under_both_goals() {
    for source in [
        "(function*() {});",
        "(function* named(value) { yield value; });",
        "(function*() { function nested() {} });",
        "(function*() { class D extends Object { constructor() { super(); } } });",
        "(function*() { return \"super() and super.value\"; });",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("nested ownership and ordinary text must remain parse-valid");
        }
    }
}

#[test]
fn generator_expression_super_precedence_preserves_prior_callable_checks() {
    for (source, code) in [
        (
            "(function*(value = yield) { super(); });",
            EarlyErrorCode::GeneratorExpressionParametersContainYield,
        ),
        (
            "(function*(value = super(), value) {});",
            EarlyErrorCode::DuplicateFormalParameter,
        ),
        (
            "(function*(value = super()) { \"use strict\"; });",
            EarlyErrorCode::CallableNonSimpleParametersContainUseStrict,
        ),
        (
            "(function*(value = super()) { let value; });",
            EarlyErrorCode::DuplicateLexicalDeclaration,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("an earlier GeneratorExpression check should own the rejection");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn generator_expression_super_classifier_is_anchored_and_injection_safe() {
    for (message, expected) in [
        (
            "generator expression cannot contain super at line 1, col 12",
            Some(EarlyErrorCode::GeneratorExpressionContainsSuper),
        ),
        (
            "async function expression cannot contain super at line 1, col 17",
            Some(EarlyErrorCode::AsyncFunctionExpressionContainsSuper),
        ),
        (
            "function expression cannot contain super at line 1, col 11",
            Some(EarlyErrorCode::FunctionExpressionContainsSuper),
        ),
        ("invalid super usage at line 1, col 12", None),
        ("invalid super call usage at line 1, col 12", None),
    ] {
        assert_eq!(
            classify_parse_failure(message).map(ParseClassified::code),
            expected
        );
    }

    let exported_name = "generator expression cannot contain super at line";
    let source = format!(
        "const value = 0; export {{ value as \"{exported_name}\" }}; export {{ value as \"{exported_name}\" }};"
    );
    let err = parse(&source, ParseOptions::module())
        .expect_err("the user-chosen exported name is duplicated");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ModuleDuplicateExport),
        "{err:?}"
    );
}

#[test]
fn generator_declaration_super_rejections_cover_parameters_bodies_arrows_and_goals() {
    for source in [
        "function* invalid() { super(); }",
        "function* invalid() { void super.value; }",
        "function* invalid(value = super()) {}",
        "function* invalid(value = super.value) {}",
        "function* invalid() { (() => super())(); }",
        "function* invalid(value = (() => super.value)()) {}",
        "function* invalid() { (async () => super())(); }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("GeneratorDeclaration parameters and body may not contain super");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::GeneratorDeclarationContainsSuper),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the generator-declaration rejection must retain a source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn anonymous_default_generator_declaration_super_rejects_under_module_goal() {
    let source = "export default function*(value = super()) {}";
    let err = parse(source, ParseOptions::module())
        .expect_err("an anonymous default GeneratorDeclaration may not contain super");
    assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
    assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::GeneratorDeclarationContainsSuper),
        "{err:?}"
    );
    let span = err
        .diagnostic()
        .span
        .expect("the default GeneratorDeclaration rejection must retain a source span");
    assert!(span.start < span.end, "{source:?}: {err:?}");
}

#[test]
fn generator_declaration_super_positive_boundaries_remain_valid_under_both_goals() {
    for source in [
        "function* valid() {}",
        "function* valid(value) { yield value; }",
        "function* valid() { function nested() {} }",
        "function* valid() { class D extends Object { constructor() { super(); } } }",
        "function* valid() { return \"super() and super.value\"; }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("nested ownership and ordinary text must remain parse-valid");
        }
    }
}

#[test]
fn generator_declaration_super_stops_at_nested_callable_and_class_owners() {
    for (source, code) in [
        (
            "function* outer() { function inner() { super(); } }",
            EarlyErrorCode::FunctionDeclarationContainsSuper,
        ),
        (
            "function* outer(value = function() { return super.value; }) {}",
            EarlyErrorCode::FunctionExpressionContainsSuper,
        ),
        (
            "function* outer() { class Inner { constructor() { super(); } } }",
            EarlyErrorCode::ClassBaseConstructorHasDirectSuper,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("the nested production must retain its own super restriction");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn generator_declaration_super_precedence_preserves_shared_callable_check_order() {
    for (source, code) in [
        (
            "function* invalid(value = super(), value) {}",
            EarlyErrorCode::DuplicateFormalParameter,
        ),
        (
            "function* invalid(value = super()) { \"use strict\"; }",
            EarlyErrorCode::CallableNonSimpleParametersContainUseStrict,
        ),
        (
            "function* invalid(value = super()) { let value; }",
            EarlyErrorCode::DuplicateLexicalDeclaration,
        ),
        (
            "function* invalid(value = (yield, super())) {}",
            EarlyErrorCode::GeneratorDeclarationContainsSuper,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("the shared GeneratorDeclaration check order must be retained");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn generator_declaration_super_classifier_is_anchored_and_injection_safe() {
    for (message, expected) in [
        (
            "generator declaration cannot contain super at line 1, col 13",
            Some(EarlyErrorCode::GeneratorDeclarationContainsSuper),
        ),
        (
            "generator expression cannot contain super at line 1, col 12",
            Some(EarlyErrorCode::GeneratorExpressionContainsSuper),
        ),
        (
            "function declaration cannot contain super at line 1, col 12",
            Some(EarlyErrorCode::FunctionDeclarationContainsSuper),
        ),
        ("invalid super usage at line 1, col 13", None),
        ("invalid super call usage at line 1, col 13", None),
    ] {
        assert_eq!(
            classify_parse_failure(message).map(ParseClassified::code),
            expected
        );
    }

    let exported_name = "generator declaration cannot contain super at line";
    let source = format!(
        "const value = 0; export {{ value as \"{exported_name}\" }}; export {{ value as \"{exported_name}\" }};"
    );
    let err = parse(&source, ParseOptions::module())
        .expect_err("the user-chosen exported name is duplicated");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ModuleDuplicateExport),
        "{err:?}"
    );
}

#[test]
fn async_generator_declaration_super_rejections_cover_parameters_bodies_arrows_and_goals() {
    for source in [
        "async function* invalid() { super(); }",
        "async function* invalid() { void super.value; }",
        "async function* invalid(value = super()) {}",
        "async function* invalid(value = super.value) {}",
        "async function* invalid() { (() => super())(); }",
        "async function* invalid(value = (() => super.value)()) {}",
        "async function* invalid() { (async () => super())(); }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("AsyncGeneratorDeclaration parameters and body may not contain super");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::AsyncGeneratorDeclarationContainsSuper),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the async-generator-declaration rejection must retain a source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn anonymous_default_async_generator_declaration_super_rejects_under_module_goal() {
    let source = "export default async function*(value = super()) {}";
    let err = parse(source, ParseOptions::module())
        .expect_err("an anonymous default AsyncGeneratorDeclaration may not contain super");
    assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
    assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::AsyncGeneratorDeclarationContainsSuper),
        "{err:?}"
    );
    let span = err
        .diagnostic()
        .span
        .expect("the default AsyncGeneratorDeclaration rejection must retain a source span");
    assert!(span.start < span.end, "{source:?}: {err:?}");
}

#[test]
fn async_generator_declaration_super_positive_boundaries_remain_valid_under_both_goals() {
    for source in [
        "async function* valid() {}",
        "async function* valid(value) { await value; yield value; }",
        "async function* valid() { function nested() {} }",
        "async function* valid() { class D extends Object { constructor() { super(); } } }",
        "async function* valid() { return \"super() and super.value\"; }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("nested ownership and ordinary text must remain parse-valid");
        }
    }
}

#[test]
fn async_generator_declaration_super_stops_at_nested_callable_and_class_owners() {
    for (source, code) in [
        (
            "async function* outer() { function inner() { super(); } }",
            EarlyErrorCode::FunctionDeclarationContainsSuper,
        ),
        (
            "async function* outer(value = function() { return super.value; }) {}",
            EarlyErrorCode::FunctionExpressionContainsSuper,
        ),
        (
            "async function* outer() { class Inner { constructor() { super(); } } }",
            EarlyErrorCode::ClassBaseConstructorHasDirectSuper,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("the nested production must retain its own super restriction");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn async_generator_declaration_super_precedence_preserves_shared_callable_check_order() {
    for (source, code) in [
        (
            "async function* invalid(value = super(), value) {}",
            EarlyErrorCode::DuplicateFormalParameter,
        ),
        (
            "async function* invalid(value = super()) { \"use strict\"; }",
            EarlyErrorCode::CallableNonSimpleParametersContainUseStrict,
        ),
        (
            "async function* invalid(value = super()) { let value; }",
            EarlyErrorCode::DuplicateLexicalDeclaration,
        ),
        (
            "async function* invalid(value = yield) { super(); }",
            EarlyErrorCode::AsyncGeneratorDeclarationContainsSuper,
        ),
        (
            "async function* invalid(value = await 0) { super(); }",
            EarlyErrorCode::AsyncGeneratorDeclarationContainsSuper,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("the shared AsyncGeneratorDeclaration check order must be retained");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn async_generator_declaration_super_classifier_is_anchored_and_injection_safe() {
    for (message, expected) in [
        (
            "async generator declaration cannot contain super at line 1, col 19",
            Some(EarlyErrorCode::AsyncGeneratorDeclarationContainsSuper),
        ),
        (
            "async generator expression cannot contain super at line 1, col 18",
            Some(EarlyErrorCode::AsyncGeneratorExpressionContainsSuper),
        ),
        (
            "generator declaration cannot contain super at line 1, col 13",
            Some(EarlyErrorCode::GeneratorDeclarationContainsSuper),
        ),
        (
            "async function declaration cannot contain super at line 1, col 18",
            Some(EarlyErrorCode::AsyncFunctionDeclarationContainsSuper),
        ),
        ("invalid super usage at line 1, col 19", None),
        ("invalid super call usage at line 1, col 19", None),
    ] {
        assert_eq!(
            classify_parse_failure(message).map(ParseClassified::code),
            expected
        );
    }

    let exported_name = "async generator declaration cannot contain super at line";
    let source = format!(
        "const value = 0; export {{ value as \"{exported_name}\" }}; export {{ value as \"{exported_name}\" }};"
    );
    let err = parse(&source, ParseOptions::module())
        .expect_err("the user-chosen exported name is duplicated");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ModuleDuplicateExport),
        "{err:?}"
    );
}

#[test]
fn async_generator_expression_super_rejections_cover_parameters_bodies_arrows_and_goals() {
    for source in [
        "(async function*() { super(); });",
        "(async function*() { void super.value; });",
        "(async function*(value = super()) {});",
        "(async function*(value = super.value) {});",
        "(async function* named() { super(); });",
        "(async function* named(value = super.value) {});",
        "(async function*() { (() => super())(); });",
        "(async function*(value = (() => super.value)()) {});",
        "(async function*() { (async () => super())(); });",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("AsyncGeneratorExpression parameters and body may not contain super");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::AsyncGeneratorExpressionContainsSuper),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the async-generator-expression rejection must retain a source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn async_generator_expression_super_positive_boundaries_remain_valid_under_both_goals() {
    for source in [
        "(async function*() {});",
        "(async function* named(value) { await value; yield value; });",
        "(async function*() { function nested() {} });",
        "(async function*() { class D extends Object { constructor() { super(); } } });",
        "(async function*() { return \"super() and super.value\"; });",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("nested ownership and ordinary text must remain parse-valid");
        }
    }
}

#[test]
fn async_generator_expression_super_precedence_preserves_prior_callable_checks() {
    for (source, code) in [
        (
            "(async function*(value = yield) { super(); });",
            EarlyErrorCode::AsyncGeneratorExpressionParametersContainYield,
        ),
        (
            "(async function*(value = await 0) { super(); });",
            EarlyErrorCode::AsyncGeneratorExpressionParametersContainAwait,
        ),
        (
            "(async function*(value = super(), value) {});",
            EarlyErrorCode::DuplicateFormalParameter,
        ),
        (
            "(async function*(value = super()) { \"use strict\"; });",
            EarlyErrorCode::CallableNonSimpleParametersContainUseStrict,
        ),
        (
            "(async function*(value = super()) { let value; });",
            EarlyErrorCode::DuplicateLexicalDeclaration,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("an earlier AsyncGeneratorExpression check should own the rejection");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn async_generator_expression_super_classifier_is_anchored_and_injection_safe() {
    for (message, expected) in [
        (
            "async generator expression cannot contain super at line 1, col 18",
            Some(EarlyErrorCode::AsyncGeneratorExpressionContainsSuper),
        ),
        (
            "generator expression cannot contain super at line 1, col 12",
            Some(EarlyErrorCode::GeneratorExpressionContainsSuper),
        ),
        (
            "async function expression cannot contain super at line 1, col 17",
            Some(EarlyErrorCode::AsyncFunctionExpressionContainsSuper),
        ),
        ("invalid super usage at line 1, col 18", None),
        ("invalid super call usage at line 1, col 18", None),
    ] {
        assert_eq!(
            classify_parse_failure(message).map(ParseClassified::code),
            expected
        );
    }

    let exported_name = "async generator expression cannot contain super at line";
    let source = format!(
        "const value = 0; export {{ value as \"{exported_name}\" }}; export {{ value as \"{exported_name}\" }};"
    );
    let err = parse(&source, ParseOptions::module())
        .expect_err("the user-chosen exported name is duplicated");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ModuleDuplicateExport),
        "{err:?}"
    );
}

#[test]
fn class_super_call_positive_boundaries_remain_valid_under_both_goals() {
    for source in [
        "class C {}",
        "class B {}; class C extends B { constructor() { super(); } }",
        "class C extends null { constructor() { super(); } }",
        "class C { constructor() { void super.value; } }",
        "class C { constructor() { class D extends C { constructor() { super(); } } } }",
        "class C { static {} }",
        "class B {}; class C extends B { static { void super.value; } }",
        "class C { static { class D extends C { constructor() { super(); } } } }",
        "class C { static { const text = \"super()\"; } }",
        "class B {}; class C extends B { field = super.value; static other = super.value; }",
        "class C { field = class D extends C { constructor() { super(); } }; }",
        "class C { field = \"super()\"; }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("heritage, SuperProperty and nested class ownership must stay distinct");
        }
    }
}

#[test]
fn class_super_call_precedence_preserves_pinned_boa_check_order() {
    for (source, code) in [
        (
            "class C { constructor() { super(); } static { super(); } }",
            EarlyErrorCode::ClassStaticBlockContainsSuperCall,
        ),
        (
            "class C { static { super(); } constructor() { super(); } }",
            EarlyErrorCode::ClassStaticBlockContainsSuperCall,
        ),
        (
            "class C { static { arguments; super(); } }",
            EarlyErrorCode::ClassStaticBlockContainsArguments,
        ),
        (
            "class C { static { super(); await 0; } }",
            EarlyErrorCode::ClassStaticBlockContainsSuperCall,
        ),
        (
            "class C { constructor() { super(); } constructor() {} }",
            EarlyErrorCode::DuplicateClassConstructor,
        ),
        (
            "class C { field = super(); constructor() { super(); } }",
            EarlyErrorCode::ClassFieldInitializerContainsSuperCall,
        ),
        (
            "class C { field = arguments && super(); }",
            EarlyErrorCode::ClassFieldContainsArguments,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("the earlier pinned class check should own the rejection");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn class_super_call_classifier_rows_are_distinct_and_injection_safe() {
    for (message, code) in [
        (
            "base class constructor cannot contain direct super call at line 2, col 1",
            EarlyErrorCode::ClassBaseConstructorHasDirectSuper,
        ),
        (
            "class static block cannot contain super call at line 2, col 1",
            EarlyErrorCode::ClassStaticBlockContainsSuperCall,
        ),
        (
            "class field initializer cannot contain super call at line 2, col 1",
            EarlyErrorCode::ClassFieldInitializerContainsSuperCall,
        ),
    ] {
        assert_eq!(
            classify_parse_failure(message).map(ParseClassified::code),
            Some(code)
        );
    }
    for message in [
        "invalid super usage at line 1, col 2",
        "invalid super call usage at line 1, col 1",
    ] {
        assert_eq!(classify_parse_failure(message), None);
    }

    for exported_name in [
        "base class constructor cannot contain direct super call at line",
        "class static block cannot contain super call at line",
        "class field initializer cannot contain super call at line",
    ] {
        let source = format!(
            "const value = 0; export {{ value as \"{exported_name}\" }}; export {{ value as \"{exported_name}\" }};"
        );
        let err = parse(&source, ParseOptions::module())
            .expect_err("the user-chosen exported name is duplicated");
        assert_eq!(
            err.diagnostic().code,
            early(EarlyErrorCode::ModuleDuplicateExport),
            "{err:?}"
        );
    }
}

#[test]
fn class_method_direct_super_call_has_exact_early_identity() {
    for source in [
        "class C { method() { super(); } }",
        "class C extends B { method() { super(); } }",
        "(class { method(value = super()) {} });",
        "class C { method(value = () => super()) {} }",
        "class C { method() { (() => super())(); } }",
        "class C { method() { (async () => super())(); } }",
        "class C { get value() { super(); } }",
        "class C { set value(value) { super(); } }",
        "class C { set value(value = super()) {} }",
        "class C { #method() { super(); } }",
        "class C { get #value() { super(); } }",
        "class C { set #value(value) { super(); } }",
        "class C { static method() { super(); } }",
        "class C { static constructor() { super(); } }",
        "class C { static #method() { super(); } }",
        "class C { static get value() { super(); } }",
        "class C { static set value(value) { super(); } }",
        "class C { [name]() { super(); } }",
        "class C { ['constructor']() { super(); } }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options).expect_err("the method owns HasDirectSuper");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::ClassMethodHasDirectSuper),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the method producer carries a source position");
            assert!(
                span.start < span.end && span.end <= source.len(),
                "{source:?}: {span:?}"
            );
        }
    }
    assert!(EarlyErrorCode::ALL.contains(&EarlyErrorCode::ClassMethodHasDirectSuper));
}

#[test]
fn class_method_direct_super_call_span_is_method_owned() {
    let source = "class C {\n  method() {\n    super();\n  }\n}";
    for options in [ParseOptions::script(), ParseOptions::module()] {
        let err = parse(source, options).expect_err("the method owns the source position");
        let start = source
            .find("method")
            .expect("the witness has a method name");
        assert_eq!(
            err.diagnostic().span,
            Some(SourceSpan {
                start,
                end: start + 1
            })
        );
        assert_eq!(
            err.diagnostic().code,
            early(EarlyErrorCode::ClassMethodHasDirectSuper)
        );
    }
}

#[test]
fn class_method_direct_super_call_respects_lexical_and_property_boundaries() {
    for source in [
        "class C { method(value = super.value) { return super.value; } }",
        "class C { method() { return () => super.value; } }",
        "class C { get value() { return super.value; } set value(value) { super.value = value; } }",
        "class C { static method() { return super.value; } }",
        "class C extends B { constructor() { super(); } }",
        "class C extends B { constructor() { (() => super())(); } }",
        "class C { method() { return class extends B { constructor() { super(); } }; } }",
        "class C { method() { return function() { return class extends B { constructor() { super(); } }; }; } }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options).unwrap_or_else(|err| {
                panic!("a different owner must remain valid: {source:?}: {err:?}")
            });
        }
    }
}

#[test]
fn class_method_direct_super_call_preserves_other_rejection_owners() {
    for (source, code) in [
        (
            "class C { constructor() { super(); } }",
            EarlyErrorCode::ClassBaseConstructorHasDirectSuper,
        ),
        (
            "class C { get constructor() { super(); } }",
            EarlyErrorCode::ClassConstructorGetter,
        ),
        (
            "class C { set constructor(value) { super(); } }",
            EarlyErrorCode::ClassConstructorSetter,
        ),
        (
            "class C { method() { function nested() { super(); } } }",
            EarlyErrorCode::FunctionDeclarationContainsSuper,
        ),
        (
            "class C { method() { (function() { super(); }); } }",
            EarlyErrorCode::FunctionExpressionContainsSuper,
        ),
        (
            "class C { method(value, value) { super(); } }",
            EarlyErrorCode::DuplicateFormalParameter,
        ),
        (
            "class C { method(value = 0) { 'use strict'; super(); } }",
            EarlyErrorCode::CallableNonSimpleParametersContainUseStrict,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options).expect_err("the earlier owner rejects this source");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
        }
    }
    // These distinct producers are intentionally outside this diagnostic repair.
    for source in [
        "({ method() { super(); } });",
        "({ get value() { super(); } });",
        "class C { *method() { super(); } }",
        "class C { async method() { super(); } }",
        "class C { async *method() { super(); } }",
        "class C { [super()]() {} }",
        "class C { method() { super(; } }",
        "class C { method() { super.; } }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err =
                parse(source, options).expect_err("the distinct parser owner rejects this source");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Parse,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                ParseCode::Malformed,
                "{source:?}: {err:?}"
            );
        }
    }
}

#[test]
fn class_method_direct_super_call_typed_carrier_owns_phase() {
    let source = "class C {\n  method() {} }";
    let position = boa_ast::Position::new(2, 3);
    let typed = boa_parser::Error::ClassMethodHasDirectSuper { position };
    assert_eq!(
        ParseClassified::from_boa_error(&typed).map(ParseClassified::code),
        Some(EarlyErrorCode::ClassMethodHasDirectSuper),
    );
    let err = parse_with_boundary::<()>(source, || Err(typed))
        .expect_err("the carrier owns Early independently of its display text");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ClassMethodHasDirectSuper)
    );
    assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
    assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
    let start = source
        .find("method")
        .expect("the witness has a method name");
    assert_eq!(
        err.diagnostic().span,
        Some(SourceSpan {
            start,
            end: start + 1
        })
    );

    for untyped in [
        boa_parser::Error::General {
            message: "class method cannot contain direct super call".into(),
            position,
        },
        boa_parser::Error::Lex {
            err: boa_parser::lexer::Error::Syntax(
                "class method cannot contain direct super call".into(),
                position,
            ),
        },
    ] {
        assert_eq!(ParseClassified::from_boa_error(&untyped), None);
        let err = parse_with_boundary::<()>(source, || Err(untyped))
            .expect_err("untyped text cannot manufacture the typed condition");
        assert_eq!(err.diagnostic().code, ParseCode::Malformed);
        assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Parse);
    }
    for message in [
        "class method cannot contain direct super call at line 2, col 3",
        "invalid super call usage at line 1, col 1",
        "invalid super call usage at line 2, col 3",
        "unexpected token 'class method cannot contain direct super call at line', expression at line 1, col 1",
    ] {
        assert_eq!(classify_parse_failure(message), None);
    }
    let source = "const value = 0; export { value as 'class method cannot contain direct super call at line', value as 'class method cannot contain direct super call at line' };";
    let err = parse(source, ParseOptions::module())
        .expect_err("the user-chosen export name is duplicated");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ModuleDuplicateExport)
    );
}

#[test]
fn script_top_level_super_precedes_new_target_and_resists_message_injection() {
    let err = parse("super.value; new.target;", ParseOptions::script())
        .expect_err("the ScriptBody super check should run before NewTarget");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ScriptTopLevelSuper),
        "{err:?}"
    );

    for message in [
        "invalid super usage at line 1, col 2",
        "invalid super usage at line 1, col 10",
    ] {
        assert_eq!(
            classify_parse_failure(message),
            None,
            "a distinct source position must not acquire the ScriptBody code"
        );
    }

    let err = parse(
        concat!(
            "const value = 0;\n",
            "export { value as \"invalid super usage at line 1, col 1\" };\n",
            "export { value as \"invalid super usage at line 1, col 1\" };",
        ),
        ParseOptions::module(),
    )
    .expect_err("the user-chosen exported name is duplicated");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ModuleDuplicateExport),
        "{err:?}"
    );
}

#[test]
fn known_script_and_class_super_producers_stay_structurally_reviewed() {
    fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        source
            .split_once(start)
            .unwrap_or_else(|| panic!("missing start marker: {start}"))
            .1
            .split_once(end)
            .unwrap_or_else(|| panic!("missing end marker after: {start}"))
            .0
    }

    fn count_in_rust_sources(root: &std::path::Path, fragment: &str) -> usize {
        let entries = std::fs::read_dir(root)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));
        let mut count = 0;
        for entry in entries {
            let path = entry.expect("failed to read vendored Boa entry").path();
            if path.is_dir() {
                count += count_in_rust_sources(&path, fragment);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
                count += source.matches(fragment).count();
            }
        }
        count
    }

    fn collect_rust_sources(
        root: &std::path::Path,
        sources: &mut Vec<(std::path::PathBuf, String)>,
    ) {
        let entries = std::fs::read_dir(root)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));
        for entry in entries {
            let path = entry.expect("failed to read workspace source entry").path();
            if path.is_dir() {
                collect_rust_sources(&path, sources);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
                sources.push((path, source));
            }
        }
    }

    let compact = |source: &str| {
        source
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
    };

    const RAW_MESSAGE: &str = "invalid super usage";
    const METHOD_MESSAGE: &str = "invalid super call usage";
    const CLASS_METHOD_MESSAGE: &str = "class method cannot contain direct super call";
    const BASE_CONSTRUCTOR_MESSAGE: &str =
        "base class constructor cannot contain direct super call";
    const STATIC_BLOCK_MESSAGE: &str = "class static block cannot contain super call";
    const FIELD_INITIALIZER_MESSAGE: &str = "class field initializer cannot contain super call";
    const FUNCTION_EXPRESSION_MESSAGE: &str = "function expression cannot contain super";
    const FUNCTION_DECLARATION_MESSAGE: &str = "function declaration cannot contain super";
    const ASYNC_FUNCTION_DECLARATION_MESSAGE: &str =
        "async function declaration cannot contain super";
    const ASYNC_FUNCTION_EXPRESSION_MESSAGE: &str =
        "async function expression cannot contain super";
    const GENERATOR_EXPRESSION_MESSAGE: &str = "generator expression cannot contain super";
    const GENERATOR_DECLARATION_MESSAGE: &str = "generator declaration cannot contain super";
    const ASYNC_GENERATOR_EXPRESSION_MESSAGE: &str =
        "async generator expression cannot contain super";
    const ASYNC_GENERATOR_DECLARATION_MESSAGE: &str =
        "async generator declaration cannot contain super";
    const MODULE_MESSAGE: &str = "module cannot contain `super` on the top-level";
    const SCRIPT_BRANCH: &str = r#"if contains(&body, ContainsSymbol::Super) {
                return Err(Error::general("invalid super usage", Position::new(1, 1)));
            }"#;
    const MODULE_BRANCH: &str = r#"if contains(&module, ContainsSymbol::Super) {
            return Err(Error::general(
                "module cannot contain `super` on the top-level",
                Position::new(1, 1),
            ));
        }"#;
    const FUNCTION_EXPRESSION_SUPER_BRANCH: &str = r#"if contains(&function, ContainsSymbol::Super) {
            return Err(Error::lex(LexError::Syntax(
                "function expression cannot contain super".into(),
                params_start_position,
            )));
        }"#;
    const ASYNC_FUNCTION_EXPRESSION_SUPER_BRANCH: &str = r#"if contains(&function, ContainsSymbol::Super) {
            return Err(Error::lex(LexError::Syntax(
                "async function expression cannot contain super".into(),
                params_start_position,
            )));
        }"#;
    const GENERATOR_EXPRESSION_SUPER_BRANCH: &str = r#"if contains(&function, ContainsSymbol::Super) {
            return Err(Error::lex(LexError::Syntax(
                "generator expression cannot contain super".into(),
                params_start_position,
            )));
        }"#;
    const ASYNC_GENERATOR_EXPRESSION_SUPER_BRANCH: &str = r#"if contains(&function, ContainsSymbol::Super) {
            return Err(Error::lex(LexError::Syntax(
                "async generator expression cannot contain super".into(),
                params_start_position,
            )));
        }"#;
    const CALLABLE_DECLARATION_REQUIRED_SUPER_MESSAGE: &str =
        "fn contains_super_error_message(&self) -> &'static str;";
    const CALLABLE_DECLARATION_SUPER_BRANCH: &str = r#"if contains(&body, ContainsSymbol::Super) || contains(&params, ContainsSymbol::Super) {
        return Err(Error::lex(LexError::Syntax(
            c.contains_super_error_message().into(),
            params_start_position,
        )));
    }"#;
    const FUNCTION_DECLARATION_SUPER_MESSAGE_METHOD: &str = r#"fn contains_super_error_message(&self) -> &'static str {
        "function declaration cannot contain super"
    }"#;
    const ASYNC_FUNCTION_DECLARATION_SUPER_MESSAGE_METHOD: &str = r#"fn contains_super_error_message(&self) -> &'static str {
        "async function declaration cannot contain super"
    }"#;
    const GENERATOR_DECLARATION_SUPER_MESSAGE_METHOD: &str = r#"fn contains_super_error_message(&self) -> &'static str {
        "generator declaration cannot contain super"
    }"#;
    const ASYNC_GENERATOR_DECLARATION_SUPER_MESSAGE_METHOD: &str = r#"fn contains_super_error_message(&self) -> &'static str {
        "async generator declaration cannot contain super"
    }"#;
    const BASE_CONSTRUCTOR_BRANCH: &str = r#"if super_ref.is_none()
                && let Some(constructor) = &constructor
                && contains(constructor, ContainsSymbol::SuperCall)
            {
                return Err(Error::lex(LexError::Syntax(
                    "base class constructor cannot contain direct super call".into(),
                    body_start,"#;
    const FIELD_INITIALIZER_POSITION: &str = r#""class field initializer cannot contain super call".into(),
                            position,"#;
    const FIELD_NODE_SUPER_BRANCH: &str = r#"if let Some(node) = field.initializer()
                        && contains(node, ContainsSymbol::SuperCall)
                    {
                        return Err(Error::lex(LexError::Syntax(
                            "class field initializer cannot contain super call".into(),
                            position,
                        )));
                    }"#;
    const FIELD_SUPER_BRANCH: &str = r#"if let Some(field) = field.initializer()
                        && contains(field, ContainsSymbol::SuperCall)
                    {
                        return Err(Error::lex(LexError::Syntax(
                            "class field initializer cannot contain super call".into(),
                            position,
                        )));
                    }"#;
    const CLASS_METHOD_NAME_SUPER_BRANCH: &str = r#"if let ClassElementName::PropertyName(name) = m.name()
                        && contains(name, ContainsSymbol::SuperCall)
                    {
                        return Err(Error::lex(LexError::Syntax(
                            "invalid super call usage".into(),
                            position,
                        )));
                    }"#;
    const CLASS_METHOD_BODY_SUPER_BRANCH: &str = r#"if contains(m.parameters(), ContainsSymbol::SuperCall)
                        || contains(m.body(), ContainsSymbol::SuperCall)
                    {
                        return Err(Error::ClassMethodHasDirectSuper { position });
                    }"#;
    const OBJECT_METHOD_POSITION_SUPER_BRANCH: &str = r#"if has_direct_super_new(&params, &body) {
                    return Err(Error::lex(LexError::Syntax(
                        "invalid super call usage".into(),
                        position,
                    )));
                }"#;
    const OBJECT_GETTER_SUPER_BRANCH: &str = r#"if has_direct_super_new(&FormalParameterList::default(), &body) {
                    return Err(Error::lex(LexError::Syntax(
                        "invalid super call usage".into(),
                        position,
                    )));
                }"#;
    const OBJECT_METHOD_PARAMS_SUPER_BRANCH: &str = r#"if has_direct_super_new(&params, &body) {
                    return Err(Error::lex(LexError::Syntax(
                        "invalid super call usage".into(),
                        params_start_position,
                    )));
                }"#;
    const OBJECT_METHOD_BODY_SUPER_BRANCH: &str = r#"if has_direct_super_new(&params, &body) {
            return Err(Error::lex(LexError::Syntax(
                "invalid super call usage".into(),
                body_start,
            )));
        }"#;
    const STATIC_BLOCK_BRANCH: &str = r#"if contains(&statement_list, ContainsSymbol::SuperCall) {
                        return Err(Error::general(
                            "class static block cannot contain super call",
                            position,"#;
    const PARSER_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/mod.rs"
    ));
    const HOISTABLE_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/statement/declaration/hoistable/mod.rs"
    ));
    const FUNCTION_DECLARATION_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/statement/declaration/hoistable/function_decl/mod.rs"
    ));
    const GENERATOR_DECLARATION_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/statement/declaration/hoistable/generator_decl/mod.rs"
    ));
    const ASYNC_FUNCTION_DECLARATION_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/statement/declaration/hoistable/async_function_decl/mod.rs"
    ));
    const ASYNC_GENERATOR_DECLARATION_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/statement/declaration/hoistable/async_generator_decl/mod.rs"
    ));
    const FUNCTION_EXPRESSION_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/primary/function_expression/mod.rs"
    ));
    const GENERATOR_EXPRESSION_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/primary/generator_expression/mod.rs"
    ));
    const ASYNC_FUNCTION_EXPRESSION_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/primary/async_function_expression/mod.rs"
    ));
    const ASYNC_GENERATOR_EXPRESSION_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/primary/async_generator_expression/mod.rs"
    ));
    const OBJECT_INITIALIZER_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/primary/object_initializer/mod.rs"
    ));
    const CLASS_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/statement/declaration/hoistable/class_decl/mod.rs"
    ));
    const PARSER_ERROR_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/error/mod.rs"
    ));
    const FRONT_SOURCE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));

    let boa_package_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/boa_parser-0.21.1");
    assert_eq!(count_in_rust_sources(&boa_package_root, RAW_MESSAGE), 1);
    assert_eq!(
        count_in_rust_sources(
            &boa_package_root,
            r#""function expression cannot contain super""#,
        ),
        1
    );
    assert_eq!(
        count_in_rust_sources(
            &boa_package_root,
            r#""function declaration cannot contain super""#,
        ),
        1
    );
    assert_eq!(
        count_in_rust_sources(
            &boa_package_root,
            r#""async function declaration cannot contain super""#,
        ),
        1
    );
    assert_eq!(
        count_in_rust_sources(&boa_package_root, ASYNC_FUNCTION_EXPRESSION_MESSAGE),
        1
    );
    assert_eq!(
        count_in_rust_sources(
            &boa_package_root,
            r#""generator expression cannot contain super""#,
        ),
        1
    );
    assert_eq!(
        count_in_rust_sources(
            &boa_package_root,
            r#""generator declaration cannot contain super""#,
        ),
        1
    );
    assert_eq!(
        count_in_rust_sources(
            &boa_package_root,
            r#""async generator expression cannot contain super""#,
        ),
        1
    );
    assert_eq!(
        count_in_rust_sources(
            &boa_package_root,
            r#""async generator declaration cannot contain super""#,
        ),
        1
    );
    assert_eq!(count_in_rust_sources(&boa_package_root, METHOD_MESSAGE), 10);
    assert_eq!(
        count_in_rust_sources(&boa_package_root, BASE_CONSTRUCTOR_MESSAGE),
        1
    );
    assert_eq!(
        count_in_rust_sources(&boa_package_root, STATIC_BLOCK_MESSAGE),
        1
    );
    assert_eq!(
        count_in_rust_sources(&boa_package_root, FIELD_INITIALIZER_MESSAGE),
        4
    );
    assert_eq!(count_in_rust_sources(&boa_package_root, MODULE_MESSAGE), 1);

    let script_start = PARSER_SOURCE
        .find("impl<R> TokenParser<R> for ScriptBody")
        .expect("the ScriptBody parser remains present");
    let module_start = PARSER_SOURCE
        .find("/// Parses a full module.")
        .expect("the Module parser remains after ScriptBody");
    let script_body = &PARSER_SOURCE[script_start..module_start];
    assert_eq!(script_body.matches(RAW_MESSAGE).count(), 1);
    assert_eq!(script_body.matches(SCRIPT_BRANCH).count(), 1);
    assert_eq!(script_body.matches("if !self.direct_eval {").count(), 1);

    let super_check = script_body
        .find(SCRIPT_BRANCH)
        .expect("the fixed ScriptBody super branch remains present");
    let direct_eval_guard = script_body
        .find("if !self.direct_eval {")
        .expect("the direct-eval exclusion remains present");
    let direct_eval_end = script_body[direct_eval_guard..]
        .find("\n        }\n\n        if let Err(error) = check_labels(&body)")
        .map(|offset| direct_eval_guard + offset)
        .expect("the direct-eval guard must still enclose the Script checks");
    assert_eq!(
        script_body[direct_eval_guard..direct_eval_end]
            .matches(SCRIPT_BRANCH)
            .count(),
        1,
        "the Script super producer must remain inside the direct-eval exclusion"
    );
    let new_target_check = script_body
        .find("if contains(&body, ContainsSymbol::NewTarget)")
        .expect("the adjacent NewTarget check remains present");
    let private_name_check = script_body
        .find("if !all_private_identifiers_valid(&body, Vec::new())")
        .expect("the adjacent private-name check remains present");
    let label_check = script_body
        .find("if let Err(error) = check_labels(&body)")
        .expect("the adjacent label check remains present");
    let cover_check = script_body
        .find("if contains_invalid_object_literal(&body)")
        .expect("the adjacent cover-grammar check remains present");
    assert!(
        direct_eval_guard < super_check
            && super_check < new_target_check
            && new_target_check < private_name_check
            && private_name_check < label_check
            && label_check < cover_check,
        "the ScriptBody semantic-check order or direct-eval boundary changed"
    );

    let module_body = &PARSER_SOURCE[module_start..];
    assert_eq!(module_body.matches(MODULE_MESSAGE).count(), 1);
    assert_eq!(module_body.matches(MODULE_BRANCH).count(), 1);
    assert!(
        module_body
            .find(MODULE_BRANCH)
            .expect("the Module super branch remains present")
            < module_body
                .find("if contains(&module, ContainsSymbol::NewTarget)")
                .expect("the Module NewTarget branch remains present"),
        "Module must retain its separate super owner and ordering"
    );

    assert_eq!(
        ASYNC_GENERATOR_EXPRESSION_SOURCE
            .matches(RAW_MESSAGE)
            .count(),
        0
    );
    assert_eq!(
        ASYNC_GENERATOR_EXPRESSION_SOURCE
            .matches(ASYNC_GENERATOR_EXPRESSION_MESSAGE)
            .count(),
        1
    );
    assert_eq!(
        compact(ASYNC_GENERATOR_EXPRESSION_SOURCE)
            .matches(compact(ASYNC_GENERATOR_EXPRESSION_SUPER_BRANCH).as_str())
            .count(),
        1,
        "the unique AsyncGeneratorExpression message must remain on the completed-node Contains Super branch"
    );
    assert_eq!(
        ASYNC_GENERATOR_EXPRESSION_SOURCE
            .matches(r#"Error::general("invalid super usage", Position::new(1, 1))"#,)
            .count(),
        0
    );
    let async_generator_parameter_yield_check = ASYNC_GENERATOR_EXPRESSION_SOURCE
        .find("if contains(&params, ContainsSymbol::YieldExpression)")
        .expect("the AsyncGeneratorExpression parameter Yield check remains present");
    let async_generator_parameter_await_check = ASYNC_GENERATOR_EXPRESSION_SOURCE
        .find("if contains(&params, ContainsSymbol::AwaitExpression)")
        .expect("the AsyncGeneratorExpression parameter Await check remains present");
    let async_generator_node_construction = ASYNC_GENERATOR_EXPRESSION_SOURCE
        .find("let function = AsyncGeneratorExpressionNode::new(")
        .expect("the complete AsyncGeneratorExpression node remains present");
    let async_generator_super_check = ASYNC_GENERATOR_EXPRESSION_SOURCE
        .find(ASYNC_GENERATOR_EXPRESSION_SUPER_BRANCH)
        .expect("the AsyncGeneratorExpression Contains Super branch remains present");
    assert!(
        async_generator_parameter_yield_check < async_generator_parameter_await_check
            && async_generator_parameter_await_check < async_generator_node_construction
            && async_generator_node_construction < async_generator_super_check,
        "the AsyncGeneratorExpression parameter checks / completed-node-super order changed"
    );
    assert_eq!(GENERATOR_EXPRESSION_SOURCE.matches(RAW_MESSAGE).count(), 0);
    assert_eq!(
        GENERATOR_EXPRESSION_SOURCE
            .matches(GENERATOR_EXPRESSION_MESSAGE)
            .count(),
        1
    );
    assert_eq!(
        compact(GENERATOR_EXPRESSION_SOURCE)
            .matches(compact(GENERATOR_EXPRESSION_SUPER_BRANCH).as_str())
            .count(),
        1,
        "the unique GeneratorExpression message must remain on the completed-node Contains Super branch"
    );
    let generator_parameter_yield_check = GENERATOR_EXPRESSION_SOURCE
        .find("if contains(&params, ContainsSymbol::YieldExpression)")
        .expect("the GeneratorExpression parameter Yield check remains present");
    let generator_node_construction = GENERATOR_EXPRESSION_SOURCE
        .find("let function = GeneratorExpressionNode::new(")
        .expect("the complete GeneratorExpression node remains present");
    let generator_super_check = GENERATOR_EXPRESSION_SOURCE
        .find(GENERATOR_EXPRESSION_SUPER_BRANCH)
        .expect("the GeneratorExpression Contains Super branch remains present");
    assert!(
        generator_parameter_yield_check < generator_node_construction
            && generator_node_construction < generator_super_check,
        "the GeneratorExpression parameter-Yield / completed-node-super order changed"
    );
    assert_eq!(
        ASYNC_FUNCTION_EXPRESSION_SOURCE
            .matches(RAW_MESSAGE)
            .count(),
        0
    );
    assert_eq!(
        ASYNC_FUNCTION_EXPRESSION_SOURCE
            .matches(ASYNC_FUNCTION_EXPRESSION_MESSAGE)
            .count(),
        1
    );
    assert_eq!(
        compact(ASYNC_FUNCTION_EXPRESSION_SOURCE)
            .matches(compact(ASYNC_FUNCTION_EXPRESSION_SUPER_BRANCH).as_str())
            .count(),
        1,
        "the unique AsyncFunctionExpression message must remain on the completed-node Contains Super branch"
    );
    let async_parameter_await_check = ASYNC_FUNCTION_EXPRESSION_SOURCE
        .find("if contains(&params, ContainsSymbol::AwaitExpression)")
        .expect("the AsyncFunctionExpression parameter Await check remains present");
    let async_node_construction = ASYNC_FUNCTION_EXPRESSION_SOURCE
        .find("let function = AsyncFunctionExpressionNode::new(")
        .expect("the complete AsyncFunctionExpression node remains present");
    let async_super_check = ASYNC_FUNCTION_EXPRESSION_SOURCE
        .find(ASYNC_FUNCTION_EXPRESSION_SUPER_BRANCH)
        .expect("the AsyncFunctionExpression Contains Super branch remains present");
    assert!(
        async_parameter_await_check < async_node_construction
            && async_node_construction < async_super_check,
        "the AsyncFunctionExpression parameter-Await / completed-node-super order changed"
    );
    assert_eq!(HOISTABLE_SOURCE.matches(RAW_MESSAGE).count(), 0);
    assert_eq!(
        HOISTABLE_SOURCE
            .matches(FUNCTION_DECLARATION_MESSAGE)
            .count(),
        0
    );
    assert_eq!(
        HOISTABLE_SOURCE
            .matches(ASYNC_FUNCTION_DECLARATION_MESSAGE)
            .count(),
        0
    );
    assert_eq!(
        HOISTABLE_SOURCE
            .matches(GENERATOR_DECLARATION_MESSAGE)
            .count(),
        0
    );
    assert_eq!(
        HOISTABLE_SOURCE
            .matches(ASYNC_GENERATOR_DECLARATION_MESSAGE)
            .count(),
        0
    );
    assert_eq!(
        compact(HOISTABLE_SOURCE)
            .matches(compact(CALLABLE_DECLARATION_REQUIRED_SUPER_MESSAGE).as_str())
            .count(),
        1,
        "every CallableDeclaration implementation must select its production-owned Contains Super diagnostic"
    );
    assert_eq!(
        compact(HOISTABLE_SOURCE)
            .matches(compact(CALLABLE_DECLARATION_SUPER_BRANCH).as_str())
            .count(),
        1,
        "the shared declaration predicate must select a production-owned message and retain the parameter-start position"
    );
    let declaration_lexical_name_check = HOISTABLE_SOURCE
        .find("name_in_lexically_declared_names(")
        .expect("the declaration parameter/body lexical-name check remains present");
    let declaration_super_check = HOISTABLE_SOURCE
        .find(CALLABLE_DECLARATION_SUPER_BRANCH)
        .expect("the shared declaration Contains Super branch remains present");
    let declaration_yield_check = HOISTABLE_SOURCE
        .find("if c.parameters_yield_is_early_error()")
        .expect("the adjacent generator parameter check remains present");
    let declaration_await_check = HOISTABLE_SOURCE
        .find("if c.parameters_await_is_early_error()")
        .expect("the adjacent async parameter check remains present");
    assert!(
        declaration_lexical_name_check < declaration_super_check
            && declaration_super_check < declaration_yield_check
            && declaration_yield_check < declaration_await_check,
        "the shared callable-declaration early-error order changed"
    );

    assert_eq!(FUNCTION_DECLARATION_SOURCE.matches(RAW_MESSAGE).count(), 0);
    assert_eq!(
        FUNCTION_DECLARATION_SOURCE
            .matches(FUNCTION_DECLARATION_MESSAGE)
            .count(),
        1
    );
    assert_eq!(
        compact(FUNCTION_DECLARATION_SOURCE)
            .matches(compact(FUNCTION_DECLARATION_SUPER_MESSAGE_METHOD).as_str())
            .count(),
        1,
        "ordinary FunctionDeclaration must retain its production-owned Contains Super diagnostic"
    );
    assert_eq!(
        FUNCTION_DECLARATION_SOURCE
            .matches("parse_callable_declaration(&self")
            .count(),
        1
    );

    assert_eq!(
        ASYNC_FUNCTION_DECLARATION_SOURCE
            .matches(RAW_MESSAGE)
            .count(),
        0
    );
    assert_eq!(
        ASYNC_FUNCTION_DECLARATION_SOURCE
            .matches(ASYNC_FUNCTION_DECLARATION_MESSAGE)
            .count(),
        1
    );
    assert_eq!(
        compact(ASYNC_FUNCTION_DECLARATION_SOURCE)
            .matches(compact(ASYNC_FUNCTION_DECLARATION_SUPER_MESSAGE_METHOD).as_str())
            .count(),
        1,
        "AsyncFunctionDeclaration must select its production-owned Contains Super diagnostic"
    );
    assert_eq!(
        ASYNC_FUNCTION_DECLARATION_SOURCE
            .matches("parameters_await_is_early_error")
            .count(),
        1
    );
    assert_eq!(
        ASYNC_FUNCTION_DECLARATION_SOURCE
            .matches("parse_callable_declaration(&self")
            .count(),
        1
    );

    assert_eq!(GENERATOR_DECLARATION_SOURCE.matches(RAW_MESSAGE).count(), 0);
    assert_eq!(
        GENERATOR_DECLARATION_SOURCE
            .matches(GENERATOR_DECLARATION_MESSAGE)
            .count(),
        1
    );
    assert_eq!(
        compact(GENERATOR_DECLARATION_SOURCE)
            .matches(compact(GENERATOR_DECLARATION_SUPER_MESSAGE_METHOD).as_str())
            .count(),
        1,
        "GeneratorDeclaration must select its production-owned Contains Super diagnostic"
    );
    assert_eq!(
        GENERATOR_DECLARATION_SOURCE
            .matches("parameters_yield_is_early_error")
            .count(),
        1
    );
    assert_eq!(
        GENERATOR_DECLARATION_SOURCE
            .matches("parse_callable_declaration(&self")
            .count(),
        1
    );

    assert_eq!(
        ASYNC_GENERATOR_DECLARATION_SOURCE
            .matches(RAW_MESSAGE)
            .count(),
        0
    );
    assert_eq!(
        ASYNC_GENERATOR_DECLARATION_SOURCE
            .matches(ASYNC_GENERATOR_DECLARATION_MESSAGE)
            .count(),
        1
    );
    assert_eq!(
        compact(ASYNC_GENERATOR_DECLARATION_SOURCE)
            .matches(compact(ASYNC_GENERATOR_DECLARATION_SUPER_MESSAGE_METHOD).as_str())
            .count(),
        1,
        "AsyncGeneratorDeclaration must select its production-owned Contains Super diagnostic"
    );
    assert_eq!(
        ASYNC_GENERATOR_DECLARATION_SOURCE
            .matches("parameters_yield_is_early_error")
            .count(),
        1
    );
    assert_eq!(
        ASYNC_GENERATOR_DECLARATION_SOURCE
            .matches("parameters_await_is_early_error")
            .count(),
        1
    );
    assert_eq!(
        ASYNC_GENERATOR_DECLARATION_SOURCE
            .matches("parse_callable_declaration(&self")
            .count(),
        1
    );
    assert_eq!(FUNCTION_EXPRESSION_SOURCE.matches(RAW_MESSAGE).count(), 0);
    assert_eq!(
        FUNCTION_EXPRESSION_SOURCE
            .matches(FUNCTION_EXPRESSION_MESSAGE)
            .count(),
        1
    );
    assert_eq!(
        compact(FUNCTION_EXPRESSION_SOURCE)
            .matches(compact(FUNCTION_EXPRESSION_SUPER_BRANCH).as_str())
            .count(),
        1,
        "the unique FunctionExpression message must remain on the completed-node Contains Super branch"
    );

    let compact_class = compact(CLASS_SOURCE);
    assert_eq!(CLASS_SOURCE.matches(RAW_MESSAGE).count(), 0);
    assert_eq!(CLASS_SOURCE.matches(METHOD_MESSAGE).count(), 1);
    assert_eq!(CLASS_SOURCE.matches(CLASS_METHOD_MESSAGE).count(), 0);
    assert_eq!(PARSER_ERROR_SOURCE.matches(CLASS_METHOD_MESSAGE).count(), 1);
    assert_eq!(
        count_in_rust_sources(&boa_package_root, "Error::ClassMethodHasDirectSuper"),
        1,
        "only the reviewed class-method parameters/body predicate may produce the typed error"
    );
    assert_eq!(
        count_in_rust_sources(&boa_package_root, CLASS_METHOD_MESSAGE),
        1
    );
    assert_eq!(OBJECT_INITIALIZER_SOURCE.matches(METHOD_MESSAGE).count(), 9);
    assert_eq!(CLASS_SOURCE.matches(BASE_CONSTRUCTOR_MESSAGE).count(), 1);
    assert_eq!(CLASS_SOURCE.matches(STATIC_BLOCK_MESSAGE).count(), 1);
    assert_eq!(CLASS_SOURCE.matches(FIELD_INITIALIZER_MESSAGE).count(), 4);
    assert_eq!(
        compact_class
            .matches(compact(BASE_CONSTRUCTOR_BRANCH).as_str())
            .count(),
        1
    );
    assert_eq!(
        compact_class
            .matches(compact(FIELD_INITIALIZER_POSITION).as_str())
            .count(),
        4
    );

    let class_elements = bounded(
        CLASS_SOURCE,
        "match &element {",
        "            elements.push(element);",
    );
    let private_field = bounded(
        class_elements,
        "function::ClassElement::PrivateFieldDefinition(field) => {",
        "function::ClassElement::PrivateStaticFieldDefinition(field) => {",
    );
    let private_static_field = bounded(
        class_elements,
        "function::ClassElement::PrivateStaticFieldDefinition(field) => {",
        "function::ClassElement::FieldDefinition(field)",
    );
    let grouped_public_fields = bounded(
        class_elements,
        "function::ClassElement::FieldDefinition(field)",
        "function::ClassElement::StaticAccessorFieldDefinition(field) => {",
    );
    let static_accessor_field = bounded(
        class_elements,
        "function::ClassElement::StaticAccessorFieldDefinition(field) => {",
        "function::ClassElement::StaticBlock(_) => {}",
    );
    let compact_node_field_branch = compact(FIELD_NODE_SUPER_BRANCH);
    let compact_field_branch = compact(FIELD_SUPER_BRANCH);
    for (owner, branch) in [
        (private_field, compact_node_field_branch.as_str()),
        (private_static_field, compact_node_field_branch.as_str()),
        (grouped_public_fields, compact_field_branch.as_str()),
        (static_accessor_field, compact_field_branch.as_str()),
    ] {
        assert_eq!(owner.matches(FIELD_INITIALIZER_MESSAGE).count(), 1);
        assert_eq!(compact(owner).matches(branch).count(), 1);
        assert_eq!(owner.matches(RAW_MESSAGE).count(), 0);
        assert_eq!(owner.matches(METHOD_MESSAGE).count(), 0);
    }

    assert_eq!(
        compact_class
            .matches(compact(CLASS_METHOD_NAME_SUPER_BRANCH).as_str())
            .count(),
        1
    );
    assert_eq!(
        compact_class
            .matches(compact(CLASS_METHOD_BODY_SUPER_BRANCH).as_str())
            .count(),
        1
    );
    let compact_object_initializer = compact(OBJECT_INITIALIZER_SOURCE);
    for (branch, count) in [
        (OBJECT_METHOD_POSITION_SUPER_BRANCH, 3),
        (OBJECT_GETTER_SUPER_BRANCH, 1),
        (OBJECT_METHOD_PARAMS_SUPER_BRANCH, 2),
        (OBJECT_METHOD_BODY_SUPER_BRANCH, 3),
    ] {
        assert_eq!(
            compact_object_initializer
                .matches(compact(branch).as_str())
                .count(),
            count
        );
    }
    assert_eq!(
        CLASS_SOURCE.matches(METHOD_MESSAGE).count()
            + OBJECT_INITIALIZER_SOURCE.matches(METHOD_MESSAGE).count(),
        count_in_rust_sources(&boa_package_root, METHOD_MESSAGE),
        "all ten generic method-message producers must remain in the two reviewed parser owners"
    );
    assert_eq!(
        compact_class
            .matches(compact(STATIC_BLOCK_BRANCH).as_str())
            .count(),
        1
    );
    assert_eq!(
        CLASS_SOURCE
            .matches(r#"Error::general("invalid super usage", Position::new(1, 1))"#)
            .count(),
        0
    );

    let class_tail_start = CLASS_SOURCE
        .find("impl<R> TokenParser<R> for ClassTail")
        .expect("the ClassTail parser remains present");
    let class_heritage_start = CLASS_SOURCE[class_tail_start..]
        .find("/// `ClassHeritage` parsing.")
        .map(|offset| class_tail_start + offset)
        .expect("ClassHeritage remains after ClassTail");
    let class_tail = &CLASS_SOURCE[class_tail_start..class_heritage_start];
    let class_body_parse = class_tail
        .find("ClassBody::new(self.name, self.allow_yield, self.allow_await)")
        .expect("ClassTail must parse one complete ClassBody");
    let base_constructor_check = class_tail
        .find("if super_ref.is_none()")
        .expect("the absent-heritage constructor check remains present");
    assert!(class_body_parse < base_constructor_check);
    assert_eq!(class_tail.matches(BASE_CONSTRUCTOR_MESSAGE).count(), 1);
    assert_eq!(class_tail.matches("body_start").count(), 2);

    let static_block_start = CLASS_SOURCE
        .find("TokenKind::Punctuator(Punctuator::OpenBlock) if r#static => {")
        .expect("the class static-block parser remains present");
    let static_block_end = CLASS_SOURCE[static_block_start..]
        .find("function::ClassElement::StaticBlock(")
        .map(|offset| static_block_start + offset)
        .expect("the class static-block AST construction remains after validation");
    let static_block = &CLASS_SOURCE[static_block_start..static_block_end];
    let arguments_check = static_block
        .find("if contains_arguments(&statement_list)")
        .expect("ContainsArguments remains the first adjacent static-block check");
    let super_call_check = static_block
        .find("if contains(&statement_list, ContainsSymbol::SuperCall)")
        .expect("the static-block Contains SuperCall check remains present");
    let await_check = static_block
        .find("if contains(&statement_list, ContainsSymbol::AwaitExpression)")
        .expect("the static-block Contains AwaitExpression check remains present");
    let object_literal_check = static_block
        .find("if contains_invalid_object_literal(&statement_list)")
        .expect("the static-block cover-grammar check remains present");
    assert!(
        arguments_check < super_call_check
            && super_call_check < await_check
            && await_check < object_literal_check,
        "the class static-block early-error ordering changed"
    );
    assert_eq!(static_block.matches(STATIC_BLOCK_MESSAGE).count(), 1);

    let (front_product, front_tests) = FRONT_SOURCE
        .split_once("#[cfg(test)]\nmod tests {")
        .expect("the product/test boundary remains explicit");
    let front_test_children = [
        "crates/scriptw-front/src/tests/declarations.rs",
        "crates/scriptw-front/src/tests/grammar.rs",
        "crates/scriptw-front/src/tests/parameters.rs",
        "crates/scriptw-front/src/tests/source_text.rs",
        "crates/scriptw-front/src/tests/super_grammar.rs",
    ];
    for child in front_test_children {
        let name = std::path::Path::new(child)
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap();
        let declaration = format!("#[path = \"{name}.rs\"]\n    mod {name};");
        assert_eq!(front_tests.matches(&declaration).count(), 1);
        assert!(!front_product.contains(&declaration));
    }
    // Host UTF-8 and prepared UTF-16 source retain independent input
    // routes. Both retain Boa's collected source; eval shares one grammar
    // operation after selecting its input representation.
    let parser_new = ["Parser", "::new("].concat();
    let parse_script = [".parse_", "script("].concat();
    let parse_owned_script = [".parse_", "script_with_source("].concat();
    let parse_module = [".parse_", "module("].concat();
    let parse_owned_module = [".parse_", "module_with_source("].concat();
    let classifier = ["classify_parse_", "failure(&err)"].concat();
    assert_eq!(front_product.matches(parser_new.as_str()).count(), 3);
    assert_eq!(front_product.matches(parse_script.as_str()).count(), 0);
    assert_eq!(front_product.matches(parse_module.as_str()).count(), 0);
    assert_eq!(
        front_product.matches(parse_owned_script.as_str()).count(),
        2
    );
    assert_eq!(
        front_product.matches(parse_owned_module.as_str()).count(),
        1
    );
    assert_eq!(front_product.matches(classifier.as_str()).count(), 1);

    let front_source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        count_in_rust_sources(&front_source_root, parser_new.as_str()),
        15,
        "all nine product parser constructions and six test-only probes require review"
    );
    for (method, routes) in [("formal_parameters", 2), ("function_body", 2), ("eval", 1)] {
        let parse_goal = [".parse_", method, "("].concat();
        assert_eq!(
            count_in_rust_sources(&front_source_root, &parse_goal),
            routes,
            "isolated Function fragments have UTF-8/UTF-16 input routes; eval shares its goal"
        );
    }
    assert_eq!(
        count_in_rust_sources(&front_source_root, parse_script.as_str()),
        6,
        "only the six test-only Script probes discard collected source"
    );
    assert_eq!(
        count_in_rust_sources(&front_source_root, parse_owned_script.as_str()),
        2,
        "host and prepared Script routes retain exact parser source ownership"
    );
    assert_eq!(
        count_in_rust_sources(&front_source_root, parse_module.as_str()),
        0,
        "no Module parse may discard collected source"
    );
    assert_eq!(
        count_in_rust_sources(&front_source_root, parse_owned_module.as_str()),
        1,
        "only the UTF-8 host Module route invokes Boa directly"
    );

    fn product_dependency_section(header: &str) -> bool {
        header == "[dependencies]"
            || header.starts_with("[dependencies.")
            || (header.starts_with("[target.")
                && (header.contains(".dependencies]") || header.contains(".dependencies.")))
    }

    fn has_product_dependency(manifest: &str, dependency: &str) -> bool {
        let mut in_product_dependencies = false;
        for line in manifest.lines() {
            let line = line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                in_product_dependencies = product_dependency_section(line);
                if in_product_dependencies && line.contains(dependency) {
                    return true;
                }
            } else if in_product_dependencies && line.contains(dependency) {
                return true;
            }
        }
        false
    }

    for manifest in [
        "[dependencies]\nboa_parser = \"0.21.1\"",
        "[dependencies.boa_parser]\nversion = \"0.21.1\"",
        "[target.'cfg(unix)'.dependencies.parser]\npackage = \"boa_parser\"",
    ] {
        assert!(has_product_dependency(manifest, "boa_parser"));
    }
    assert!(!has_product_dependency(
        "[dev-dependencies.boa_parser]\nversion = \"0.21.1\"",
        "boa_parser"
    ));

    let crates_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("scriptw-front remains under the workspace crates directory");
    let mut parser_dependency_owners = Vec::new();
    for entry in std::fs::read_dir(crates_root).expect("failed to read workspace crates") {
        let path = entry.expect("failed to read workspace crate entry").path();
        let manifest_path = path.join("Cargo.toml");
        if !manifest_path.is_file() {
            continue;
        }
        let manifest = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
        if has_product_dependency(&manifest, "boa_parser") {
            parser_dependency_owners.push(
                path.file_name()
                    .expect("workspace crate has a directory name")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    parser_dependency_owners.sort();
    assert_eq!(parser_dependency_owners, vec!["scriptw-front".to_string()]);

    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("scriptw-front remains two levels below the workspace root");
    let mut workspace_sources = Vec::new();
    collect_rust_sources(&workspace_root.join("crates"), &mut workspace_sources);

    fn identifier_mentions(source: &str, identifier: &str) -> usize {
        let is_identifier_continue = |character: Option<char>| {
            character.is_some_and(|character| character.is_alphanumeric() || character == '_')
        };

        source
            .match_indices(identifier)
            .filter(|(start, _)| {
                let end = *start + identifier.len();
                !is_identifier_continue(source[..*start].chars().next_back())
                    && !is_identifier_continue(source[end..].chars().next())
            })
            .count()
    }

    let classifier_identifier = ["classify_parse_", "failure"].concat();
    let classifier_call = ["classify_parse_", "failure("].concat();
    let classifier_definition = ["pub const fn classify_parse_", "failure("].concat();
    let product_parse_call = ["classify_parse_", "failure(&err)"].concat();
    let dependency_test_helper_call =
        ["scriptw_front::classify_parse_", "failure(message)"].concat();
    let classifier_reexport_prefix = "pubuseearly_error_code::";
    let mut classifier_reexport_members = vec![
        classifier_identifier.clone(),
        "EarlyErrorCode".to_string(),
        "ParseClassified".to_string(),
        "NO_EARLY_ERROR_CODE".to_string(),
    ];
    classifier_reexport_members.sort();
    let front_glob_import = ["scriptw_front", "::*"].concat();
    let classifier_module_glob_import = ["early_error_code", "::*"].concat();
    let mut all_classifier_identifier_owners = Vec::new();
    let mut product_classifier_definitions = Vec::new();
    let mut product_classifier_calls = Vec::new();
    let mut product_parse_call_owners = Vec::new();
    let mut dependency_test_helper_owners = Vec::new();
    let mut classifier_reexport_owners = Vec::new();
    let mut classifier_glob_import_owners = Vec::new();
    for (path, source) in workspace_sources {
        let relative = path
            .strip_prefix(workspace_root)
            .expect("workspace Rust source must remain below the workspace root")
            .to_string_lossy()
            .into_owned();
        let all_count = identifier_mentions(&source, &classifier_identifier);
        if all_count != 0 {
            all_classifier_identifier_owners.push((relative.clone(), all_count));
        }

        let (product, tests) = if front_test_children.contains(&relative.as_str()) {
            ("", source.as_str())
        } else {
            source
                .split_once("#[cfg(test)]")
                .map_or((source.as_str(), ""), |(product, tests)| (product, tests))
        };
        let definition_count = product.matches(classifier_definition.as_str()).count();
        if definition_count != 0 {
            product_classifier_definitions.push((relative.clone(), definition_count));
        }
        let call_count = product.matches(classifier_call.as_str()).count() - definition_count;
        if call_count != 0 {
            product_classifier_calls.push((relative.clone(), call_count));
        }
        let product_parse_call_count = product.matches(product_parse_call.as_str()).count();
        if product_parse_call_count != 0 {
            product_parse_call_owners.push((relative.clone(), product_parse_call_count));
        }
        let dependency_test_helper_count =
            tests.matches(dependency_test_helper_call.as_str()).count();
        if dependency_test_helper_count != 0 {
            dependency_test_helper_owners.push((relative.clone(), dependency_test_helper_count));
        }
        let compact_product = compact(product);
        let reexports = compact_product
            .split(classifier_reexport_prefix)
            .skip(1)
            .map(|suffix| {
                let statement = suffix.split_once(';').expect("complete public re-export").0;
                let members = statement
                    .strip_prefix('{')
                    .and_then(|members| members.strip_suffix('}'))
                    .unwrap_or(statement);
                let mut members = members
                    .split(',')
                    .filter(|member| !member.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                members.sort();
                members
            })
            .collect::<Vec<_>>();
        if !reexports.is_empty() {
            assert_eq!(
                reexports,
                vec![classifier_reexport_members.clone()],
                "{relative} must retain the exact reviewed exports without aliases or extra declarations"
            );
            classifier_reexport_owners.push((relative.clone(), reexports.len()));
        }
        let compact_source = compact(&source);
        let glob_count = compact_source.matches(front_glob_import.as_str()).count()
            + compact_source
                .matches(classifier_module_glob_import.as_str())
                .count();
        if glob_count != 0 {
            classifier_glob_import_owners.push((relative, glob_count));
        }
    }
    all_classifier_identifier_owners.sort();
    product_classifier_definitions.sort();
    product_classifier_calls.sort();
    product_parse_call_owners.sort();
    dependency_test_helper_owners.sort();
    classifier_reexport_owners.sort();
    classifier_glob_import_owners.sort();
    assert_eq!(
        all_classifier_identifier_owners,
        vec![
            (
                "crates/scriptw-front/src/early_error_code.rs".to_string(),
                66,
            ),
            ("crates/scriptw-front/src/lib.rs".to_string(), 4),
            ("crates/scriptw-front/src/tests/grammar.rs".to_string(), 1),
            (
                "crates/scriptw-front/src/tests/parameters.rs".to_string(),
                1
            ),
            (
                "crates/scriptw-front/src/tests/source_text.rs".to_string(),
                1
            ),
            (
                "crates/scriptw-front/src/tests/super_grammar.rs".to_string(),
                13
            ),
            ("crates/scriptw-ir/src/modules/early.rs".to_string(), 2),
        ],
        "every classifier identifier, including imports, re-exports and aliases, requires review"
    );
    assert_eq!(
        product_classifier_definitions,
        vec![(
            "crates/scriptw-front/src/early_error_code.rs".to_string(),
            1,
        )],
        "the workspace must retain one classifier definition"
    );
    assert_eq!(
        product_classifier_calls,
        vec![
            (
                "crates/scriptw-front/src/early_error_code.rs".to_string(),
                61,
            ),
            ("crates/scriptw-front/src/lib.rs".to_string(), 1),
        ],
        "only classifier self-proofs and the product parse boundary may call the classifier"
    );
    assert_eq!(
        classifier_reexport_owners,
        vec![("crates/scriptw-front/src/lib.rs".to_string(), 1)],
        "the public classifier surface must remain the one reviewed scriptw-front re-export"
    );
    assert_eq!(
        classifier_glob_import_owners,
        Vec::<(String, usize)>::new(),
        "workspace glob imports must not expose or alias the classifier without naming it"
    );
    assert_eq!(
        product_parse_call_owners,
        vec![("crates/scriptw-front/src/lib.rs".to_string(), 1)],
        "the sole product classifier consumer must remain scriptw-front's direct Boa parse boundary"
    );
    assert_eq!(
        dependency_test_helper_owners,
        vec![("crates/scriptw-ir/src/modules/early.rs".to_string(), 1)],
        "only the retained scriptw-ir test helper may call the exported classifier directly"
    );
}

#[test]
fn pinned_contains_super_traversal_stays_structurally_reviewed() {
    fn visitor_method<'a>(source: &'a str, signature: &str) -> &'a str {
        let start = source
            .find(signature)
            .unwrap_or_else(|| panic!("missing visitor method {signature}"));
        let remaining_source = &source[start + signature.len()..];
        let next_method = remaining_source.find("\n        fn ");
        let next_operation_comment = remaining_source.find("\n        // `");
        let end_offset = match (next_method, next_operation_comment) {
            (Some(method), Some(comment)) => method.min(comment),
            (Some(method), None) => method,
            (None, Some(comment)) => comment,
            (None, None) => remaining_source.len(),
        };
        let end = start + signature.len() + end_offset;
        &source[start..end]
    }

    fn visit_with_method<'a>(source: &'a str, implementation: &str) -> &'a str {
        let implementation_start = source
            .find(implementation)
            .unwrap_or_else(|| panic!("missing VisitWith implementation {implementation}"));
        let method_start = source[implementation_start..]
            .find("fn visit_with<'a, V>")
            .map(|offset| implementation_start + offset)
            .expect("VisitWith implementation must retain its shared visitor method");
        let method_end = source[method_start..]
            .find("\n    fn visit_with_mut")
            .map(|offset| method_start + offset)
            .expect("shared visitor method must remain before its mutable counterpart");
        &source[method_start..method_end]
    }

    let compact = |source: &str| {
        source
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
            .replace(",)", ")")
    };

    const OPERATIONS_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_ast-0.21.1/src/operations/mod.rs"
    ));
    const PROPERTY_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_ast-0.21.1/src/property.rs"
    ));
    const CLASS_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_ast-0.21.1/src/function/class.rs"
    ));

    for signature in [
        "fn visit_function_expression(",
        "fn visit_function_declaration(",
        "fn visit_async_function_expression(",
        "fn visit_async_function_declaration(",
        "fn visit_generator_expression(",
        "fn visit_generator_declaration(",
        "fn visit_async_generator_expression(",
        "fn visit_async_generator_declaration(",
    ] {
        let method = visitor_method(OPERATIONS_SOURCE, signature);
        assert!(method.contains("self.visit_contains_eval(node.contains_direct_eval)?;"));
        assert!(method.contains("ControlFlow::Continue(())"));
        assert!(
            !method.contains("node.visit_with(self)"),
            "ordinary callable definitions must remain Contains boundaries: {signature}"
        );
    }

    const ARROW_VISITOR: &str = r#"fn visit_arrow_function(
            &mut self,
            node: &'ast ArrowFunction,
        ) -> ControlFlow<Self::BreakTy> {
            if ![
                ContainsSymbol::NewTarget,
                ContainsSymbol::SuperProperty,
                ContainsSymbol::SuperCall,
                ContainsSymbol::Super,
                ContainsSymbol::This,
                ContainsSymbol::DirectEval,
            ]
            .contains(&self.0)
            {
                return ControlFlow::Continue(());
            }

            node.visit_with(self)
        }"#;
    const ASYNC_ARROW_VISITOR: &str = r#"fn visit_async_arrow_function(
            &mut self,
            node: &'ast AsyncArrowFunction,
        ) -> ControlFlow<Self::BreakTy> {
            if ![
                ContainsSymbol::NewTarget,
                ContainsSymbol::SuperProperty,
                ContainsSymbol::SuperCall,
                ContainsSymbol::Super,
                ContainsSymbol::This,
                ContainsSymbol::DirectEval,
            ]
            .contains(&self.0)
            {
                return ControlFlow::Continue(());
            }

            node.visit_with(self)
        }"#;
    for (signature, expected) in [
        ("fn visit_arrow_function(", ARROW_VISITOR),
        ("fn visit_async_arrow_function(", ASYNC_ARROW_VISITOR),
    ] {
        let method = compact(visitor_method(OPERATIONS_SOURCE, signature));
        assert_eq!(
            method,
            compact(expected),
            "lexical arrow traversal changed: {signature}"
        );
        assert_eq!(method.matches("returnControlFlow::Continue(())").count(), 1);
        assert_eq!(method.matches("ContainsSymbol::SuperCall").count(), 1);
        assert!(method.ends_with("node.visit_with(self)}"));
    }

    const CLASS_EXPRESSION_VISITOR: &str = r#"fn visit_class_expression(
            &mut self,
            node: &'ast ClassExpression,
        ) -> ControlFlow<Self::BreakTy> {
            if !node.elements().is_empty() && self.0 == ContainsSymbol::ClassBody {
                return ControlFlow::Break(());
            }

            if node.super_ref().is_some() && self.0 == ContainsSymbol::ClassHeritage {
                return ControlFlow::Break(());
            }

            node.visit_with(self)
        }"#;
    const CLASS_DECLARATION_VISITOR: &str = r#"fn visit_class_declaration(
            &mut self,
            node: &'ast ClassDeclaration,
        ) -> ControlFlow<Self::BreakTy> {
            if !node.elements().is_empty() && self.0 == ContainsSymbol::ClassBody {
                return ControlFlow::Break(());
            }

            if node.super_ref().is_some() && self.0 == ContainsSymbol::ClassHeritage {
                return ControlFlow::Break(());
            }

            node.visit_with(self)
        }"#;
    for (signature, expected) in [
        ("fn visit_class_expression(", CLASS_EXPRESSION_VISITOR),
        ("fn visit_class_declaration(", CLASS_DECLARATION_VISITOR),
    ] {
        let method = compact(visitor_method(OPERATIONS_SOURCE, signature));
        assert_eq!(
            method,
            compact(expected),
            "class Contains boundary changed: {signature}"
        );
        assert!(
            !method.contains("ContainsSymbol::SuperCall"),
            "a nested class must not stop SuperCall before computed names are visited: {signature}"
        );
        assert!(method.ends_with("node.visit_with(self)}"));
    }

    const CLASS_ELEMENT_VISITOR: &str = r#"fn visit_class_element(
            &mut self,
            node: &'ast ClassElement
        ) -> ControlFlow<Self::BreakTy> {
            match node {
                ClassElement::MethodDefinition(m) => {
                    if self.0 == ContainsSymbol::DirectEval {
                        return ControlFlow::Continue(());
                    }

                    if let ClassElementName::PropertyName(name) = m.name() {
                        name.visit_with(self)
                    } else {
                        ControlFlow::Continue(())
                    }
                }
                ClassElement::FieldDefinition(field)
                | ClassElement::StaticFieldDefinition(field) => field.name.visit_with(self),
                _ => ControlFlow::Continue(()),
            }
        }"#;
    let class_element = compact(visitor_method(OPERATIONS_SOURCE, "fn visit_class_element("));
    assert_eq!(class_element, compact(CLASS_ELEMENT_VISITOR));
    assert!(!class_element.contains("StaticBlock"));
    assert!(!class_element.contains(".body"));
    assert!(!class_element.contains("initializer"));

    const CLASS_DECLARATION_VISIT_WITH: &str = r#"fn visit_with<'a, V>(
            &'a self,
            visitor: &mut V,
        ) -> ControlFlow<V::BreakTy>
        where
            V: Visitor<'a>,
        {
            visitor.visit_identifier(&self.name)?;
            for decorator in &*self.decorators {
                visitor.visit_expression(decorator)?;
            }
            if let Some(expr) = &self.super_ref {
                visitor.visit_expression(expr)?;
            }
            if let Some(func) = &self.constructor {
                visitor.visit_function_expression(func)?;
            }
            for elem in &*self.elements {
                visitor.visit_class_element(elem)?;
            }
            ControlFlow::Continue(())
        }"#;
    const CLASS_EXPRESSION_VISIT_WITH: &str = r#"fn visit_with<'a, V>(
            &'a self,
            visitor: &mut V,
        ) -> ControlFlow<V::BreakTy>
        where
            V: Visitor<'a>,
        {
            if let Some(ident) = &self.name {
                visitor.visit_identifier(ident)?;
            }
            for decorator in &*self.decorators {
                visitor.visit_expression(decorator)?;
            }
            if let Some(expr) = &self.super_ref {
                visitor.visit_expression(expr)?;
            }
            if let Some(func) = &self.constructor {
                visitor.visit_function_expression(func)?;
            }
            for elem in &*self.elements {
                visitor.visit_class_element(elem)?;
            }
            ControlFlow::Continue(())
        }"#;
    for (implementation, expected) in [
        (
            "impl VisitWith for ClassDeclaration",
            CLASS_DECLARATION_VISIT_WITH,
        ),
        (
            "impl VisitWith for ClassExpression",
            CLASS_EXPRESSION_VISIT_WITH,
        ),
    ] {
        assert_eq!(
            compact(visit_with_method(CLASS_SOURCE, implementation)),
            compact(expected),
            "nested-class traversal changed: {implementation}"
        );
    }

    let property_definition = compact(visitor_method(
        OPERATIONS_SOURCE,
        "fn visit_property_definition(",
    ));
    assert!(property_definition.contains("ifletPropertyDefinition::MethodDefinition(m)=node"));
    assert!(property_definition.contains("returnm.name().visit_with(self);"));
    assert!(property_definition.ends_with("node.visit_with(self)}"));

    const PROPERTY_NAME_VISIT_WITH: &str = r#"fn visit_with<'a, V>(
            &'a self,
            visitor: &mut V,
        ) -> ControlFlow<V::BreakTy>
        where
            V: Visitor<'a>,
        {
            match self {
                Self::Literal(ident) => visitor.visit_sym(ident.sym_ref()),
                Self::Computed(expr) => visitor.visit_expression(expr),
            }
        }"#;
    let property_visit = compact(visit_with_method(
        PROPERTY_SOURCE,
        "impl VisitWith for PropertyName",
    ));
    assert_eq!(property_visit, compact(PROPERTY_NAME_VISIT_WITH));
    assert!(
        class_element.contains("m.name()")
            && class_element.contains("field.name.visit_with(self)")
            && property_visit.contains("Self::Computed(expr)=>visitor.visit_expression(expr)"),
        "nested classes must traverse computed element names without traversing class bodies"
    );
}
