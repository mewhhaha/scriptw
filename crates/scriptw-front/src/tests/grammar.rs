use super::*;

#[test]
fn script_rejects_module_syntax() {
    let err = parse("export const value = 1;", ParseOptions::script())
        .expect_err("script goal should reject export");
    assert!(err.message().contains("parse error"));
    assert_eq!(
        err.diagnostic().kind(),
        ParseDiagnosticKind::MalformedJavaScript
    );
    assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Parse);
    assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
    assert_eq!(err.diagnostic().code, ParseCode::Malformed);
}

#[test]
fn parser_rejects_obvious_function_syntax_error() {
    let err = parse("function {", ParseOptions::script())
        .expect_err("broken function syntax should fail");
    assert!(err.message().contains("parse error"));
    assert_eq!(
        err.diagnostic().kind(),
        ParseDiagnosticKind::MalformedJavaScript
    );
}

#[test]
fn for_in_using_conflicts_keep_their_existing_early_error_owner() {
    for source in [
        "for (using x in {}) { var x; }",
        "async function f() { for (await using x in {}) { var x; } }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("for-in using syntax must reject before body conflict analysis");
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::ForInUsingDeclaration),
                "{source:?}: {err:?}"
            );
        }
    }
}

#[test]
fn callable_use_strict_message_inventory_stays_at_sixteen_reviewed_sites() {
    const MESSAGE: &str =
        "Illegal 'use strict' directive in function with non-simple parameter list";
    const SOURCES: [(&str, usize); 10] = [
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/../../../vendor/boa_parser-0.21.1/src/parser/statement/declaration/hoistable/mod.rs"
            )),
            1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/../../../vendor/boa_parser-0.21.1/src/parser/statement/declaration/hoistable/class_decl/mod.rs"
            )),
            4,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/primary/object_initializer/mod.rs"
            )),
            5,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/primary/function_expression/mod.rs"
            )),
            1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/primary/generator_expression/mod.rs"
            )),
            1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/primary/async_function_expression/mod.rs"
            )),
            1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/primary/async_generator_expression/mod.rs"
            )),
            1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/assignment/mod.rs"
            )),
            1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/assignment/async_arrow_function.rs"
            )),
            1,
        ),
        (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/../../../vendor/boa_parser-0.21.1/src/parser/expression/assignment/arrow_function.rs"
            )),
            0,
        ),
    ];

    let mut total = 0;
    for (source, expected) in SOURCES {
        let count = source.matches(MESSAGE).count();
        assert_eq!(count, expected);
        total += count;
    }
    assert_eq!(total, 16);
}

#[test]
fn callable_use_strict_conjunction_and_containment_boundaries_remain_valid() {
    for source in [
        "function simple(a) { 'use strict'; }",
        "function non_simple(a = 0) {}",
        "a => { 'use strict'; };",
        "(a = 0) => 0;",
        "function nested(a = 0) { function inner() { 'use strict'; } }",
        "function after_prologue(a = 0) { 0; 'use strict'; }",
        "class C { method(a = 0) {} }",
        "class C { get #x() { 'use strict'; } }",
        "class C { set #x(a = 0) {} }",
        "class C { set x({ a }) {} }",
        "class C { set x(a) { 'use strict'; } }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options).expect(
                "ambient strictness, one false conjunct, or a nested directive must remain valid",
            );
        }
    }
}

#[test]
fn private_getter_and_class_setter_grammar_errors_stay_unclassified() {
    for source in [
        "class C { get #x(a = 0) { 'use strict'; } }",
        "class C { set #x() {} }",
        "class C { set #x(a, b) {} }",
        "class C { set #x(a,) {} }",
        "class C { set #x(...a) { 'use strict'; } }",
        "class C { set x() {} }",
        "class C { set x(a, b) {} }",
        "class C { set x(a,) {} }",
        "class C { set x(...a) { 'use strict'; } }",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("getter/setter parameter grammar must reject before body analysis");
            assert_eq!(
                err.diagnostic().code,
                ParseCode::Malformed,
                "{source:?}: {err:?}"
            );
        }
    }
}

#[test]
fn parser_label_static_semantics_errors_report_early_phase() {
    let err = parse("break;", ParseOptions::script())
        .expect_err("unlabelled break outside breakable statement should fail");
    assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
    assert_eq!(err.diagnostic().code, early(EarlyErrorCode::IllegalBreak));

    let err = parse("continue missing;", ParseOptions::script())
        .expect_err("labelled continue outside iteration should fail");
    assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::IllegalContinue)
    );

    let err = parse(
        "while (false) { continue missing; }",
        ParseOptions::script(),
    )
    .expect_err("continue to undefined label should fail");
    assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::UndefinedContinueTarget)
    );
}

#[test]
fn duplicate_class_constructors_report_one_early_error_for_both_forms_and_goals() {
    for source in [
        "class C { constructor() {} constructor() {} }",
        "let C = class { constructor() {} constructor() {} };",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("a class may not contain two ordinary constructors");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::DuplicateClassConstructor),
                "{source:?}: {err:?}"
            );
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn duplicate_class_constructor_boundaries_preserve_static_and_computed_methods() {
    for source in [
        r#"class C {
                constructor() {}
                static constructor() {}
                ["constructor"]() {}
            }"#,
        r#"let C = class {
                constructor() {}
                static constructor() {}
                ["constructor"]() {}
            };"#,
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("static and computed constructor methods are not constructor definitions");
        }
    }
}

#[test]
fn class_constructor_generator_methods_report_one_early_error_for_both_forms_and_goals() {
    for source in [
        "class C { *constructor() {} }",
        "let C = class { async *constructor() {} };",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("a non-static class constructor may not be a generator method");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::ClassConstructorGeneratorMethod),
                "{source:?}: {err:?}"
            );
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn remaining_class_constructor_restrictions_report_specific_early_errors_in_both_goals() {
    for (source, code) in [
        (
            "class C { async constructor() {} }",
            EarlyErrorCode::ClassConstructorAsyncMethod,
        ),
        (
            "let C = class { async constructor() {} };",
            EarlyErrorCode::ClassConstructorAsyncMethod,
        ),
        (
            "class C { get constructor() {} }",
            EarlyErrorCode::ClassConstructorGetter,
        ),
        (
            "let C = class { get constructor() {} };",
            EarlyErrorCode::ClassConstructorGetter,
        ),
        (
            "class C { set constructor(value) {} }",
            EarlyErrorCode::ClassConstructorSetter,
        ),
        (
            "let C = class { set constructor(value) {} };",
            EarlyErrorCode::ClassConstructorSetter,
        ),
        (
            "class C { #constructor; }",
            EarlyErrorCode::ClassPrivateConstructorName,
        ),
        (
            "let C = class { static async *#constructor() {} };",
            EarlyErrorCode::ClassPrivateConstructorName,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("a forbidden class constructor form should fail before evaluation");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn duplicate_class_private_names_report_one_early_error_for_both_forms_and_goals() {
    for source in [
        "class C { #x; #x; }",
        "let C = class { #x() {} static #x; };",
        "class C { get #x() {} get #x() {} }",
        "let C = class { set #x(value) {} #x; };",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("a class may not declare the same private name twice");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::ClassDuplicatePrivateName),
                "{source:?}: {err:?}"
            );
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn duplicate_class_private_name_boundaries_preserve_accessor_pairs_and_nested_classes() {
    for source in [
        "class C { get #x() {} set #x(value) {} }",
        "let C = class { #x() {} method() { return class { #x() {} }; } };",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("a getter/setter pair and a nested class have valid private-name domains");
        }
    }
}

#[test]
fn class_field_literal_name_restrictions_cover_fields_accessors_forms_and_goals() {
    for (source, code) in [
        (
            "class C { constructor = 1; }",
            EarlyErrorCode::ClassFieldConstructorName,
        ),
        (
            r#"let C = class { "constructor"; };"#,
            EarlyErrorCode::ClassFieldConstructorName,
        ),
        (
            "class C { accessor constructor = 1; }",
            EarlyErrorCode::ClassFieldConstructorName,
        ),
        (
            r#"let C = class { accessor "constructor"; };"#,
            EarlyErrorCode::ClassFieldConstructorName,
        ),
        (
            "class C { static constructor = 1; }",
            EarlyErrorCode::ClassStaticFieldConstructorOrPrototypeName,
        ),
        (
            r#"let C = class { static "prototype"; };"#,
            EarlyErrorCode::ClassStaticFieldConstructorOrPrototypeName,
        ),
        (
            "class C { static accessor constructor = 1; }",
            EarlyErrorCode::ClassStaticFieldConstructorOrPrototypeName,
        ),
        (
            r#"let C = class { static accessor "prototype"; };"#,
            EarlyErrorCode::ClassStaticFieldConstructorOrPrototypeName,
        ),
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("a forbidden literal class-field name should fail before evaluation");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(
                err.diagnostic().error_type(),
                Some("SyntaxError"),
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }
}

#[test]
fn class_field_name_restrictions_preserve_computed_names_and_constructor_methods() {
    for source in [
        r#"class C {
                constructor() {}
                prototype;
                accessor "prototype";
                ["constructor"];
                static ["constructor"];
                static ["prototype"] = 1;
            }"#,
        r#"let C = class {
                accessor ["constructor"] = 1;
                static accessor ["constructor"];
                static accessor ["prototype"] = 1;
            };"#,
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options).expect(
                "computed names, non-static prototype fields and constructor methods remain valid",
            );
        }
    }
}

#[test]
fn strict_mode_with_statements_report_one_early_error_across_strict_contexts() {
    for source in [
        r#""use strict"; with ({}) {}"#,
        r#"function f() { "use strict"; with ({}) {} }"#,
        "class C { method() { with ({}) {} } }",
    ] {
        let err = parse(source, ParseOptions::script())
            .expect_err("with statements in strict Script code should fail before evaluation");
        assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
        assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
        assert_eq!(
            err.diagnostic().code,
            early(EarlyErrorCode::StrictModeWithStatement),
            "{source:?}: {err:?}"
        );
        assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
    }

    let err = parse("with ({}) {}", ParseOptions::module())
        .expect_err("Module code is strict without a directive");
    assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
    assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::StrictModeWithStatement),
        "{err:?}"
    );
    assert!(err.diagnostic().span.is_some(), "{err:?}");
}

#[test]
fn sloppy_with_statements_remain_valid_without_a_strict_context() {
    for source in ["with ({}) {}", "function f() { with ({}) {} }"] {
        parse(source, ParseOptions::script()).expect("sloppy Script code permits WithStatement");
    }
}

#[test]
fn delete_reference_early_errors_keep_distinct_typed_conditions() {
    for (source, options, code) in [
        (
            r#""use strict"; delete identifier;"#,
            ParseOptions::script(),
            EarlyErrorCode::StrictModeDeleteIdentifierReference,
        ),
        (
            r#""use strict"; delete (identifier);"#,
            ParseOptions::script(),
            EarlyErrorCode::StrictModeDeleteIdentifierReference,
        ),
        (
            r#"function f() { "use strict"; delete ((identifier)); }"#,
            ParseOptions::script(),
            EarlyErrorCode::StrictModeDeleteIdentifierReference,
        ),
        (
            "delete identifier;",
            ParseOptions::module(),
            EarlyErrorCode::StrictModeDeleteIdentifierReference,
        ),
        (
            "class C { #x; m(o) { delete o.#x; } }",
            ParseOptions::script(),
            EarlyErrorCode::StrictModeDeletePrivateReference,
        ),
        (
            "const C = class { #x; field = delete ((this.#x)); };",
            ParseOptions::module(),
            EarlyErrorCode::StrictModeDeletePrivateReference,
        ),
        (
            "class C { #m() {} m(o) { delete o().#m; } }",
            ParseOptions::script(),
            EarlyErrorCode::StrictModeDeletePrivateReference,
        ),
        (
            "const C = class { get #x() {} m(o) { delete (o.#x); } };",
            ParseOptions::module(),
            EarlyErrorCode::StrictModeDeletePrivateReference,
        ),
        (
            "class C { m(o) { delete o.#missing; } }",
            ParseOptions::script(),
            EarlyErrorCode::StrictModeDeletePrivateReference,
        ),
        (
            "class C { #x; m(o) { delete o?.#x; } }",
            ParseOptions::script(),
            EarlyErrorCode::StrictModeDeletePrivateReference,
        ),
        (
            "const C = class { #x; m(o) { delete ((o?.c.#x)); } };",
            ParseOptions::module(),
            EarlyErrorCode::StrictModeDeletePrivateReference,
        ),
    ] {
        let err = parse(source, options)
            .expect_err("a forbidden delete-reference operand should fail before evaluation");
        assert_eq!(
            err.diagnostic().phase(),
            ParseDiagnosticPhase::Early,
            "{source:?}: {err:?}"
        );
        assert_eq!(
            err.diagnostic().error_type(),
            Some("SyntaxError"),
            "{source:?}: {err:?}"
        );
        assert_eq!(err.diagnostic().code, early(code), "{source:?}: {err:?}");
        assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
    }
}

#[test]
fn delete_reference_early_error_boundaries_remain_valid() {
    for (source, options) in [
        ("delete identifier;", ParseOptions::script()),
        ("delete ((identifier));", ParseOptions::script()),
        (
            r#""use strict"; delete object.property;"#,
            ParseOptions::script(),
        ),
        ("delete object.property;", ParseOptions::module()),
        (
            r#""use strict"; delete (0, identifier);"#,
            ParseOptions::script(),
        ),
        ("delete object[key];", ParseOptions::module()),
        (
            r#""use strict"; delete object?.property;"#,
            ParseOptions::script(),
        ),
        ("class C { #x; m(o) { o.#x; } }", ParseOptions::script()),
        (
            "class C { #x; m(o) { return o?.#x; } }",
            ParseOptions::script(),
        ),
        (
            "const C = class { #x; m(o) { return o?.c.#x; } };",
            ParseOptions::module(),
        ),
        (
            "class C { #x; m(o) { delete o?.#x.property; } }",
            ParseOptions::script(),
        ),
        (
            "class C { #m() {} m(o) { delete o?.#m(); } }",
            ParseOptions::script(),
        ),
    ] {
        parse(source, options).expect(
            "sloppy identifiers, public properties, values and private reads remain valid syntax",
        );
    }
}

#[test]
fn optional_chain_tagged_templates_report_one_early_error_in_both_goals() {
    for source in [
        "const value = null; value?.`x`;",
        "const value = null; value?.`x${1}`;",
        "const value = null; value?.\n`x`;",
        "const value = { tag() {} }; value?.tag`x`;",
        "const value = { tag() {} }; value?.tag`x${1}`;",
        "const value = { tag() {} }; value?.tag\n`x`;",
    ] {
        for options in [ParseOptions::script(), ParseOptions::module()] {
            let err = parse(source, options)
                .expect_err("a TemplateLiteral directly on an optional chain should fail");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::OptionalChainTaggedTemplate),
                "{source:?}: {err:?}"
            );
            let span = err
                .diagnostic()
                .span
                .expect("the rejected TemplateLiteral must retain its source span");
            assert!(span.start < span.end, "{source:?}: {err:?}");
        }
    }
}

#[test]
fn duplicate_import_attribute_keys_report_one_module_early_error() {
    for source in [
        r#"import "./dep.mjs" with { type: "json", "type": "css" };"#,
        r#"export * from "./dep.mjs" with { mode: "first", "mode": "second" };"#,
    ] {
        let err = parse(source, ParseOptions::module())
            .expect_err("duplicate static import-attribute keys should fail");
        assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
        assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
        assert_eq!(
            err.diagnostic().code,
            early(EarlyErrorCode::ModuleDuplicateImportAttributeKey),
            "{source:?}: {err:?}"
        );
        assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
    }
}

#[test]
fn user_export_names_cannot_forge_duplicate_import_attribute_classification() {
    let err = parse(
        r#"export { "duplicate import attribute key at line" };"#,
        ParseOptions::module(),
    )
    .expect_err("a string literal cannot be a local referenced binding");
    assert_eq!(err.diagnostic().code, ParseCode::Malformed, "{err}");

    let err = parse(
        concat!(
            "const value = 0;\n",
            "export { value as \"duplicate import attribute key at line\" };\n",
            "export { value as \"duplicate import attribute key at line\" };",
        ),
        ParseOptions::module(),
    )
    .expect_err("the user-chosen exported name is duplicated");
    assert_eq!(
        err.diagnostic().code,
        early(EarlyErrorCode::ModuleDuplicateExport),
        "{err}"
    );
}

#[test]
fn duplicate_import_attribute_message_has_one_shared_attribute_parser_owner() {
    const MESSAGE: &str = "duplicate import attribute key";
    const IMPORT_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/statement/declaration/import.rs"
    ));
    const EXPORT_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/../../../vendor/boa_parser-0.21.1/src/parser/statement/declaration/export.rs"
    ));

    assert_eq!(IMPORT_SOURCE.matches(MESSAGE).count(), 1);
    assert_eq!(EXPORT_SOURCE.matches(MESSAGE).count(), 0);
    assert!(IMPORT_SOURCE.contains("fn parse_module_request_attributes<R: ReadChar>("));
    assert!(IMPORT_SOURCE.contains("pub(super) fn parse_re_export_request<R: ReadChar>("));
    assert_eq!(EXPORT_SOURCE.matches("parse_re_export_request(").count(), 2);
    assert!(classify_parse_failure("duplicate import attribute key: type").is_none());
}

#[test]
fn parser_accepts_simple_module_syntax() {
    parse("export const value = 1;", ParseOptions::module())
        .expect("module goal should accept export");
}

#[test]
fn parser_rejects_labelled_functions_nested_under_if_and_loop() {
    for source in [
        "if (true) label: function nested_if() {}",
        "while (false) label: function nested_loop() {}",
    ] {
        parse(source, ParseOptions::script())
            .expect_err("labelled function nested under a control-flow statement should fail");
    }
}

#[test]
fn sloppy_let_and_async_identifiers_may_end_the_source() {
    // ExpressionStatement restricts only the lookaheads `let [` and
    // `async [no LineTerminator here] function`; end of input is neither,
    // and ASI supplies the final semicolon (ECMA-262 14.5, 12.10.1).
    for source in [
        "var let = 1; let",
        "var let = 1; let // comment",
        "var let = 1; let /* comment */",
        "var let = 1; if (true) let",
        "var let = 1; label: let",
        "var async = 1; async",
        "var async = 1; async /* comment */",
        "var async = 1; x = async",
        "var async = 1; if (true) async",
        "var async = function () {}; async(1)",
        "var async = function () {}; async()",
        "var async = function () {}; x = async(1)\n",
        "var async = function () {}; async(1)\n// trailing",
    ] {
        assert!(
            parse(source, ParseOptions::script()).is_ok(),
            "{source:?}: {:?}",
            parse(source, ParseOptions::script()).err()
        );
    }
    for source in [
        "\"use strict\"; let",
        "var let = 1; let [",
        "async function",
        "var async = 1; async x",
    ] {
        assert!(
            parse(source, ParseOptions::script()).is_err(),
            "{source:?} stays a SyntaxError"
        );
    }
    // A module is strict: `let` is reserved there.
    assert!(parse("let", ParseOptions::module()).is_err());
}

#[test]
fn source_nesting_past_the_parser_limit_is_a_syntax_error() {
    // The parser needs native stack in proportion to the nesting it admits;
    // product compilation runs on a 64 MiB stack, so this test does too.
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            for (open, close) in [("(", ")"), ("[", "]"), ("{a:", "}")] {
                let source = |depth: usize| {
                    format!("var x = {}1{};", open.repeat(depth), close.repeat(depth))
                };
                assert!(parse(&source(990), ParseOptions::script()).is_ok());
                let err = parse(&source(4000), ParseOptions::script())
                    .expect_err("nesting past the limit is rejected");
                assert!(
                    err.message()
                        .contains("nesting exceeds the implementation limit"),
                    "{err:?}"
                );
                assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn export_default_async_is_a_declaration_only_before_function_on_the_same_line() {
    use boa_ast::declaration::ExportDeclaration;
    use boa_ast::ModuleItem;

    // ECMA-262 16.2.3: `export default` takes a declaration only for the
    // lookaheads `function`, `async [no LineTerminator here] function` and
    // `class`; any other `async` begins an AssignmentExpression.
    let default_export_kinds = |source: &str| {
        let parsed = parse(source, ParseOptions::module())
            .unwrap_or_else(|err| panic!("{source:?}: {err:?}"));
        parsed
            .as_module()
            .expect("module goal")
            .with_compiler_session(|module, _| {
                module
                    .items()
                    .items()
                    .iter()
                    .filter_map(|item| match item {
                        ModuleItem::ExportDeclaration(export) => Some(match export.as_ref() {
                            ExportDeclaration::DefaultAssignmentExpression(_) => "expression",
                            ExportDeclaration::DefaultAsyncFunctionDeclaration(_) => {
                                "async function"
                            }
                            ExportDeclaration::DefaultAsyncGeneratorDeclaration(_) => {
                                "async generator"
                            }
                            _ => "other",
                        }),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
    };
    for (source, kind) in [
        ("export default async () => 1", "expression"),
        ("export default async x => x", "expression"),
        ("export default async\nx => x", "expression"),
        ("var async = 1; export default async", "expression"),
        ("var async = 1; export default async;", "expression"),
        (
            "var async = function () {}; export default async(1)",
            "expression",
        ),
        (
            "var async = 1; export default async\nfunction f() {}",
            "expression",
        ),
        ("export default async function f() {}", "async function"),
        ("export default async function () {}", "async function"),
        ("export default\nasync function f() {}", "async function"),
        ("export default async function* g() {}", "async generator"),
    ] {
        assert_eq!(default_export_kinds(source), [kind], "{source:?}");
    }
    assert!(parse("export default async x", ParseOptions::module()).is_err());
}

#[test]
fn source_nesting_limit_is_exact() {
    use boa_parser::parser::{MAX_NESTING_UNITS, NESTING_UNITS_PER_LEVEL};
    let limit = MAX_SOURCE_NESTING_DEPTH as usize;
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let rejected = |source: &str| {
                parse(source, ParseOptions::script())
                    .err()
                    .is_some_and(|err| {
                        err.message()
                            .contains("nesting exceeds the implementation limit")
                    })
            };
            // The statement and its initializer are two levels, each
            // parenthesis one more.
            let parentheses =
                |depth: usize| format!("var x = {}1{};", "(".repeat(depth), ")".repeat(depth));
            assert!(!rejected(&parentheses(limit - 2)));
            assert!(rejected(&parentheses(limit - 1)));
            // With one level of room left, a `+` chain inside may have exactly
            // one level's worth of operators.
            let room = (MAX_NESTING_UNITS / MAX_SOURCE_NESTING_DEPTH) as usize;
            assert_eq!(room, NESTING_UNITS_PER_LEVEL as usize);
            let chain = |operators: usize| {
                format!(
                    "var x = {}1{}{};",
                    "(".repeat(limit - 3),
                    "+1".repeat(operators),
                    ")".repeat(limit - 3)
                )
            };
            assert!(!rejected(&chain(room)));
            assert!(rejected(&chain(room + 1)));
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn long_operator_chains_parse_without_recursing_per_operator() {
    // A million operators (Node's measured scale): the parser builds them in
    // loops, the visitors that check early errors and the tree's Drop walk
    // them without recursion, on this test's default thread stack. About two
    // million fit the limit.
    for operator in ["+", "||", "&&", "??", ","] {
        let source = format!("x{};", format!(" {operator} x").repeat(1_000_000));
        assert!(parse(&source, ParseOptions::script()).is_ok(), "{operator}");
    }
    let source = format!("x{};", " + x".repeat(2_100_000));
    assert!(parse(&source, ParseOptions::script()).is_err());
}

fn on_compiler_stack(test: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(test)
        .unwrap()
        .join()
        .unwrap();
}

fn nesting_error(source: &str) -> Option<String> {
    parse(source, ParseOptions::script())
        .err()
        .map(|error| error.message().to_string())
        .filter(|message| message.contains("nesting exceeds the implementation limit"))
}

// In a generator or async function body a binary chain costs half a level per
// operator only when one of its operands suspends, and then for every one of
// its operators. Otherwise it costs what it costs anywhere, so an ES module's
// top-level code, which module lowering parses again inside an async arrow
// function, pays nothing extra.
#[test]
fn operator_chains_in_resumable_bodies_pay_only_when_they_suspend() {
    on_compiler_stack(|| {
        let chain = |operators: usize| " + x".repeat(operators);
        for source in [
            format!("async () => {{ \"use strict\"; x{}; }}", chain(100_000)),
            format!("async function f() {{ await x; x{}; }}", chain(100_000)),
            format!("function* g() {{ yield; x{}; }}", chain(100_000)),
            // A function inside the chain owns its suspensions.
            format!(
                "async () => {{ x + async function () {{ await x; }}{}; }}",
                chain(100_000)
            ),
            format!("async () => {{ await x{}; }}", chain(1_900)),
        ] {
            assert_eq!(nesting_error(&source), None, "{}", &source[..40]);
        }
        for source in [
            format!("async () => {{ await x{}; }}", chain(3_000)),
            format!("async () => {{ x{} + await x; }}", chain(3_000)),
            format!("function* g() {{ x{} + (yield); }}", chain(3_000)),
        ] {
            let error = nesting_error(&source).expect("past the limit");
            assert!(
                error.contains("with an `await` or `yield` operand"),
                "{error}"
            );
        }
    });
}

// The compiler parses a module's body again inside its module scaffolding, an
// async arrow function eight levels deep, so the Module parse charges the body
// as that reparse will: eight levels deeper than a Script, and as a resumable
// body, where a chain with an `await` operand costs half a level per operator.
// Either limit is then reported by the Module parse, at the module's own
// source position, instead of in the reparse's generated text.
#[test]
fn module_bodies_are_charged_as_their_reparse_inside_the_module_scaffolding() {
    use boa_parser::parser::MODULE_BODY_ENCLOSING_LEVELS;
    let limit = MAX_SOURCE_NESTING_DEPTH as usize - MODULE_BODY_ENCLOSING_LEVELS as usize;
    on_compiler_stack(move || {
        let module_error = |source: &str| {
            parse(source, ParseOptions::module())
                .err()
                .map(|error| error.message().to_string())
                .filter(|message| message.contains("nesting exceeds the implementation limit"))
        };
        let parentheses = |depth: usize| {
            format!(
                "export const x = {}1{};",
                "(".repeat(depth),
                ")".repeat(depth)
            )
        };
        // An exported declaration's initializer is one level, each
        // parenthesis one more.
        assert_eq!(module_error(&parentheses(limit - 1)), None);
        let error = module_error(&parentheses(limit)).expect("past the module limit");
        assert!(error.contains("at line 1, col"), "{error}");
        let chain = |operators: usize| " + \"a\"".repeat(operators);
        assert_eq!(
            module_error(&format!("export const s = (await x){};", chain(1_900))),
            None
        );
        assert_eq!(
            module_error(&format!("export const s = x{};", chain(100_000))),
            None
        );
        let error = module_error(&format!("export const s = (await x){};", chain(3_000)))
            .expect("past the limit");
        assert!(
            error.contains("with an `await` or `yield` operand"),
            "{error}"
        );
        assert!(error.contains("at line 1, col"), "{error}");
    });
}

// A method call's property access and call cost half a level each, so about
// 1,000 chained method calls fit, four times the other links. Other member, call
// and optional links cost two levels, as each item of an optional chain does;
// the error names what exceeded the limit.
#[test]
fn member_chains_are_charged_by_link_kind() {
    on_compiler_stack(|| {
        let ok = |source: &str| parse(source, ParseOptions::script()).is_ok();
        assert!(ok(&format!("o{};", ".m()".repeat(950))));
        assert!(ok(&format!("o{};", "[k](1)".repeat(950))));
        assert!(ok(&format!("new Map(){};", ".set(1, 2)".repeat(950))));
        assert!(ok(&format!("o?{};", ".m()".repeat(950))));
        let error = nesting_error(&format!("o{};", ".m()".repeat(1_100))).expect("too long");
        assert!(error.contains("method call"), "{error}");

        assert!(ok(&format!("o{};", ".a".repeat(490))));
        assert!(ok(&format!("o?{};", ".a".repeat(490))));
        for source in [
            format!("o{};", ".a".repeat(600)),
            format!("o?{};", ".a".repeat(600)),
            format!("f{};", "()".repeat(600)),
        ] {
            let error = nesting_error(&source).expect("too long");
            assert!(error.contains("property access, call"), "{error}");
        }
        let error = nesting_error(&format!(
            "var x = {}1{};",
            "(".repeat(1_000),
            ")".repeat(1_000)
        ))
        .expect("too deep");
        assert!(error.contains("of 1000 levels at line"), "{error}");
    });
}

// `(a0, a1, …) =>` is covered by a comma chain, converted to parameters
// without recursing per comma. ECMA-262 sets no limit on parameters; Node 24's
// V8 accepts at most 65,525 (its SyntaxError says 65,534), and Scriptw rejects
// more than 65,535 with a SyntaxError, for every function form.
#[test]
fn parameter_lists_are_bounded_and_converted_without_recursion() {
    let parameters = |count: usize| {
        (0..count)
            .map(|index| format!("a{index}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let error = |source: String| {
        parse(&source, ParseOptions::script())
            .err()
            .map(|error| error.message().to_string())
            .unwrap_or_default()
    };
    // Boa's scope analysis looks bindings up linearly, so one function at the
    // limit takes seconds (see the inventory); one form checks the boundary.
    assert!(parse(
        &format!("var f = ({}) => 0;", parameters(65_535)),
        ParseOptions::script()
    )
    .is_ok());
    let list = parameters(1_000);
    for source in [
        format!("var f = async ({list}) => 0;"),
        format!("function f({list}) {{}}"),
    ] {
        assert!(parse(&source, ParseOptions::script()).is_ok());
    }
    for count in [65_536, 300_000] {
        let list = parameters(count);
        for source in [
            format!("var f = ({list}) => 0;"),
            format!("var f = async ({list}) => 0;"),
            format!("function f({list}) {{}}"),
        ] {
            let message = error(source);
            assert!(message.contains("too many formal parameters"), "{message}");
        }
    }
}
