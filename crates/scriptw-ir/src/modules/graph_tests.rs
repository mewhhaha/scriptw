use super::*;

use super::super::graph_build::build_graph;
// The single authority for what a resolved binding reads as in the merged
// scope, now that this file mints no cell name of its own.
use super::super::namespace::namespace_target_reference;

/// Last path segment, which is what the test loader resolves on.
fn last_segment(text: &str) -> &str {
    text.rsplit('/').next().unwrap_or(text)
}

/// Stands in for a host loader: a specifier resolves to the file whose key
/// ends in the same segment, so `"./a.js"`, `"../dir/a.js"` and `"a.js"`
/// all normalize to the one key `"/root/a.js"` — which is the shape the
/// module map has to collapse. `files[0]` is the entry.
fn sources_of(files: &[(&str, &str)]) -> ModuleGraphSources {
    let modules: Vec<ModuleSourceIr> = files
        .iter()
        .map(|(key, source_text)| {
            ModuleSourceIr::new(
                ModuleKey::from_host(*key),
                (*source_text).to_string(),
                format!("file://{key}"),
            )
        })
        .collect();
    let mut resolutions = Vec::new();
    for (index, _) in files.iter().enumerate() {
        let requests = modules[index]
            .module_requests()
            .expect("test module parses");
        let referrer = u32::try_from(index).expect("test graph is small");
        for request in requests {
            let target = files
                .iter()
                .position(|(key, _)| last_segment(key) == last_segment(request.specifier()));
            if let Some(target) = target {
                let target = u32::try_from(target).expect("test graph is small");
                resolutions.push((referrer, request, target));
            }
        }
    }
    ModuleGraphSources {
        realm_requests: Default::default(),
        modules,
        entry: 0,
        resolutions,
    }
}

fn linked(files: &[(&str, &str)]) -> ModuleGraphIr {
    let mut graph = build_graph(&sources_of(files)).expect("test modules parse");
    link(&mut graph);
    graph
}

fn unit_of(graph: &ModuleGraphIr, key: &str) -> ModuleUnitId {
    *graph
        .keys
        .get(&crate::ModuleRecordKey::new(
            ModuleKey::from_host(key),
            crate::ModuleKindIr::SourceText,
        ))
        .expect("key is in the graph")
}

fn components(graph: &ModuleGraphIr) -> Vec<Vec<ModuleUnitId>> {
    crate::evaluation_components(graph)
}

#[test]
fn rejected_delete_reference_dependencies_keep_typed_diagnostics_through_graph_build() {
    for (index, source_text, expected) in [
        (
            0,
            "export const x = 0; delete x;",
            EarlyErrorCode::StrictModeDeleteIdentifierReference,
        ),
        (
            1,
            "export class C { #x; m(o) { delete o.#x; } }",
            EarlyErrorCode::StrictModeDeletePrivateReference,
        ),
    ] {
        let dependency_key = format!("/root/delete-{index}.js");
        let dependency = ModuleSourceIr::new(
            ModuleKey::from_host(dependency_key.clone()),
            source_text.to_string(),
            format!("file://{dependency_key}"),
        );
        assert_eq!(
            dependency.module_requests(),
            None,
            "the rejected parse must be retained rather than rescanned"
        );

        let diagnostics = build_graph(&ModuleGraphSources {
            realm_requests: Default::default(),
            modules: vec![
                ModuleSourceIr::new(
                    ModuleKey::from_host("/root/entry.js"),
                    format!("import './delete-{index}.js';"),
                    "file:///root/entry.js".to_string(),
                ),
                dependency,
            ],
            entry: 0,
            resolutions: vec![(
                0,
                ModuleRequestKeyIr::plain(format!("./delete-{index}.js")),
                1,
            )],
        })
        .expect_err("the retained rejected dependency must stop graph construction");
        let [diagnostic] = diagnostics.as_slice() else {
            panic!("expected one retained parse diagnostic, got {diagnostics:?}");
        };

        assert_eq!(
            diagnostic.kind(),
            IrDiagnosticKind::LinkError,
            "{source_text:?}"
        );
        assert_eq!(
            diagnostic.phase(),
            IrDiagnosticPhase::Resolution,
            "{source_text:?}"
        );
        assert_eq!(diagnostic.code(), Some(expected), "{source_text:?}");
        assert_eq!(
            diagnostic.error_type(),
            Some(NativeErrorKind::SyntaxError),
            "{source_text:?}"
        );
        assert!(diagnostic.span.is_some(), "{source_text:?}: {diagnostic:?}");
    }
}

#[test]
fn rejected_optional_chain_tagged_template_dependency_keeps_typed_diagnostic_through_graph_build() {
    let dependency_source = "export const value = null; value?.tag`x${1}`;";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/optional-template.js"),
        dependency_source.to_string(),
        "file:///root/optional-template.js".to_string(),
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "the rejected parse must be retained rather than rescanned"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './optional-template.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(0, ModuleRequestKeyIr::plain("./optional-template.js"), 1)],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::OptionalChainTaggedTemplate)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained TemplateLiteral must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn rejected_for_head_body_declaration_conflict_dependency_keeps_typed_diagnostic_through_graph_build(
) {
    let dependency_source = "for (let x of []) { var x; }";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/for-head-body-conflict.js"),
        dependency_source.to_string(),
        "file:///root/for-head-body-conflict.js".to_string(),
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "the rejected parse must be retained rather than rescanned"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './for-head-body-conflict.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./for-head-body-conflict.js"),
            1,
        )],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::ForHeadBodyDeclarationConflict)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained loop conflict must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn rejected_for_declaration_duplicate_bound_name_dependency_keeps_typed_diagnostic_through_graph_build(
) {
    let dependency_source = "for (let [x, x] of []) {}";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/for-declaration-duplicate.js"),
        dependency_source.to_string(),
        "file:///root/for-declaration-duplicate.js".to_string(),
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "the rejected parse must be retained rather than rescanned"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './for-declaration-duplicate.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./for-declaration-duplicate.js"),
            1,
        )],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::ForDeclarationDuplicateBoundName)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained duplicate loop binding must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn rejected_lexical_bound_name_let_dependency_keeps_typed_diagnostic_through_graph_build() {
    let dependency_source = "for (const { value: let } of []) {}";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/lexical-bound-name-let.js"),
        dependency_source.to_string(),
        "file:///root/lexical-bound-name-let.js".to_string(),
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "the rejected Module parse must be retained rather than rescanned"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './lexical-bound-name-let.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./lexical-bound-name-let.js"),
            1,
        )],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(diagnostic.code(), Some(EarlyErrorCode::LexicalBoundNameLet));
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained lexical binding must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn rejected_top_level_super_dependency_keeps_its_module_code_through_graph_build() {
    let dependency_source = "() => super.value;";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/top-level-super.js"),
        dependency_source.to_string(),
        "file:///root/top-level-super.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    assert_eq!(
        dependency.module_requests(),
        None,
        "the rejected Module parse must be retained rather than rescanned"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './top-level-super.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(0, ModuleRequestKeyIr::plain("./top-level-super.js"), 1)],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(diagnostic.code(), Some(EarlyErrorCode::ModuleTopLevelSuper));
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained Module failure must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn rejected_class_owned_super_call_dependencies_keep_distinct_codes_through_graph_build() {
    for (index, source_text, expected) in [
        (
            0,
            "export default class { constructor() { super(); } }",
            EarlyErrorCode::ClassBaseConstructorHasDirectSuper,
        ),
        (
            1,
            "export default class { static { super(); } }",
            EarlyErrorCode::ClassStaticBlockContainsSuperCall,
        ),
    ] {
        let dependency_key = format!("/root/class-super-{index}.js");
        let dependency = ModuleSourceIr::new(
            ModuleKey::from_host(dependency_key.clone()),
            source_text.to_string(),
            format!("file://{dependency_key}"),
        );
        assert_eq!(dependency.goal(), ParseGoal::Module);
        let ModuleParse::Rejected { error, .. } = &dependency.parse else {
            panic!("the class-owned early error must be retained as a rejected parse");
        };
        assert_eq!(
            crate::modules::early::module_parse_failure_diagnostic(error).code(),
            Some(expected),
            "{source_text:?}"
        );
        assert_eq!(
            dependency.module_requests(),
            None,
            "a rejected dependency must not be rescanned for requests"
        );

        let diagnostics = build_graph(&ModuleGraphSources {
            realm_requests: Default::default(),
            modules: vec![
                ModuleSourceIr::new(
                    ModuleKey::from_host("/root/entry.js"),
                    format!("import './class-super-{index}.js';"),
                    "file:///root/entry.js".to_string(),
                ),
                dependency,
            ],
            entry: 0,
            resolutions: vec![(
                0,
                ModuleRequestKeyIr::plain(format!("./class-super-{index}.js")),
                1,
            )],
        })
        .expect_err("the retained rejected dependency must stop graph construction");
        let [diagnostic] = diagnostics.as_slice() else {
            panic!("expected one retained parse diagnostic, got {diagnostics:?}");
        };

        assert_eq!(
            diagnostic.kind(),
            IrDiagnosticKind::LinkError,
            "{source_text:?}"
        );
        assert_eq!(
            diagnostic.phase(),
            IrDiagnosticPhase::Resolution,
            "{source_text:?}"
        );
        assert_eq!(diagnostic.code(), Some(expected), "{source_text:?}");
        assert_eq!(
            diagnostic.error_type(),
            Some(NativeErrorKind::SyntaxError),
            "{source_text:?}"
        );
        let span = diagnostic
            .span
            .expect("the retained class rejection must keep its source span");
        assert!(span.start < span.end, "{source_text:?}: {diagnostic:?}");
    }
}

#[test]
fn rejected_class_field_super_call_dependency_keeps_its_code_through_graph_build() {
    let dependency_source = "export default class { accessor field = super(); }";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/class-field-super.js"),
        dependency_source.to_string(),
        "file:///root/class-field-super.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    let ModuleParse::Rejected { error, .. } = &dependency.parse else {
        panic!("the class field early error must be retained as a rejected parse");
    };
    assert_eq!(
        crate::modules::early::module_parse_failure_diagnostic(error).code(),
        Some(EarlyErrorCode::ClassFieldInitializerContainsSuperCall)
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "a rejected dependency must not be rescanned for requests"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './class-field-super.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(0, ModuleRequestKeyIr::plain("./class-field-super.js"), 1)],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::ClassFieldInitializerContainsSuperCall)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained class field rejection must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn rejected_function_expression_super_dependency_keeps_its_code_through_graph_build() {
    let dependency_source = "export default (function(value = super()) {});";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/function-expression-super.js"),
        dependency_source.to_string(),
        "file:///root/function-expression-super.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    let ModuleParse::Rejected { error, .. } = &dependency.parse else {
        panic!("the FunctionExpression early error must be retained as a rejected parse");
    };
    assert_eq!(
        crate::modules::early::module_parse_failure_diagnostic(error).code(),
        Some(EarlyErrorCode::FunctionExpressionContainsSuper)
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "a rejected dependency must not be rescanned for requests"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './function-expression-super.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./function-expression-super.js"),
            1,
        )],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::FunctionExpressionContainsSuper)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained FunctionExpression rejection must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn retained_function_expression_without_super_builds_a_real_module_graph() {
    let dependency_source = "export default (function(value) { return value; });";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/function-expression.js"),
        dependency_source.to_string(),
        "file:///root/function-expression.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    assert_eq!(dependency.module_requests(), Some(Vec::new()));

    let graph = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './function-expression.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(0, ModuleRequestKeyIr::plain("./function-expression.js"), 1)],
    })
    .expect("a valid FunctionExpression dependency must build a Module graph");
    assert_eq!(graph.units.len(), 2);
}

#[test]
fn rejected_function_declaration_super_dependency_keeps_its_code_through_graph_build() {
    let dependency_source = "export function invalid(value = super()) {}";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/function-declaration-super.js"),
        dependency_source.to_string(),
        "file:///root/function-declaration-super.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    let ModuleParse::Rejected { error, .. } = &dependency.parse else {
        panic!("the FunctionDeclaration early error must be retained as a rejected parse");
    };
    assert_eq!(
        crate::modules::early::module_parse_failure_diagnostic(error).code(),
        Some(EarlyErrorCode::FunctionDeclarationContainsSuper)
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "a rejected dependency must not be rescanned for requests"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './function-declaration-super.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./function-declaration-super.js"),
            1,
        )],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::FunctionDeclarationContainsSuper)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained FunctionDeclaration rejection must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn retained_function_declaration_without_super_builds_a_real_module_graph() {
    let dependency_source = "export function valid(value) { return value; }";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/function-declaration.js"),
        dependency_source.to_string(),
        "file:///root/function-declaration.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    assert_eq!(dependency.module_requests(), Some(Vec::new()));

    let graph = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './function-declaration.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(0, ModuleRequestKeyIr::plain("./function-declaration.js"), 1)],
    })
    .expect("a valid FunctionDeclaration dependency must build a Module graph");
    assert_eq!(graph.units.len(), 2);
}

#[test]
fn rejected_async_function_declaration_super_dependency_keeps_its_code_through_graph_build() {
    let dependency_source = "export async function invalid(value = super()) {}";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/async-function-declaration-super.js"),
        dependency_source.to_string(),
        "file:///root/async-function-declaration-super.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    let ModuleParse::Rejected { error, .. } = &dependency.parse else {
        panic!("the AsyncFunctionDeclaration early error must be retained as a rejected parse");
    };
    assert_eq!(
        crate::modules::early::module_parse_failure_diagnostic(error).code(),
        Some(EarlyErrorCode::AsyncFunctionDeclarationContainsSuper)
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "a rejected dependency must not be rescanned for requests"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './async-function-declaration-super.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./async-function-declaration-super.js"),
            1,
        )],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::AsyncFunctionDeclarationContainsSuper)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained AsyncFunctionDeclaration rejection must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn retained_async_function_declaration_without_super_builds_a_real_module_graph() {
    let dependency_source = "export async function valid(value) { return await value; }";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/async-function-declaration.js"),
        dependency_source.to_string(),
        "file:///root/async-function-declaration.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    assert_eq!(dependency.module_requests(), Some(Vec::new()));

    let graph = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './async-function-declaration.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./async-function-declaration.js"),
            1,
        )],
    })
    .expect("a valid AsyncFunctionDeclaration dependency must build a Module graph");
    assert_eq!(graph.units.len(), 2);
}

#[test]
fn rejected_generator_declaration_super_dependency_keeps_its_code_through_graph_build() {
    let dependency_source = "export function* invalid(value = super()) {}";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/generator-declaration-super.js"),
        dependency_source.to_string(),
        "file:///root/generator-declaration-super.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    let ModuleParse::Rejected { error, .. } = &dependency.parse else {
        panic!("the GeneratorDeclaration early error must be retained as a rejected parse");
    };
    assert_eq!(
        crate::modules::early::module_parse_failure_diagnostic(error).code(),
        Some(EarlyErrorCode::GeneratorDeclarationContainsSuper)
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "a rejected dependency must not be rescanned for requests"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './generator-declaration-super.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./generator-declaration-super.js"),
            1,
        )],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::GeneratorDeclarationContainsSuper)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained GeneratorDeclaration rejection must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn retained_generator_declaration_without_super_builds_a_real_module_graph() {
    let dependency_source = "export function* valid(value) { yield value; }";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/generator-declaration.js"),
        dependency_source.to_string(),
        "file:///root/generator-declaration.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    assert_eq!(dependency.module_requests(), Some(Vec::new()));

    let graph = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './generator-declaration.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./generator-declaration.js"),
            1,
        )],
    })
    .expect("a valid GeneratorDeclaration dependency must build a Module graph");
    assert_eq!(graph.units.len(), 2);
}

#[test]
fn rejected_async_generator_declaration_super_dependency_keeps_its_code_through_graph_build() {
    let dependency_source = "export async function* invalid(value = super()) {}";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/async-generator-declaration-super.js"),
        dependency_source.to_string(),
        "file:///root/async-generator-declaration-super.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    let ModuleParse::Rejected { error, .. } = &dependency.parse else {
        panic!("the AsyncGeneratorDeclaration early error must be retained as a rejected parse");
    };
    assert_eq!(
        crate::modules::early::module_parse_failure_diagnostic(error).code(),
        Some(EarlyErrorCode::AsyncGeneratorDeclarationContainsSuper)
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "a rejected dependency must not be rescanned for requests"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './async-generator-declaration-super.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./async-generator-declaration-super.js"),
            1,
        )],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::AsyncGeneratorDeclarationContainsSuper)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained AsyncGeneratorDeclaration rejection must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn retained_async_generator_declaration_without_super_builds_a_real_module_graph() {
    let dependency_source = "export async function* valid(value) { await value; yield value; }";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/async-generator-declaration.js"),
        dependency_source.to_string(),
        "file:///root/async-generator-declaration.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    assert_eq!(dependency.module_requests(), Some(Vec::new()));

    let graph = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './async-generator-declaration.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./async-generator-declaration.js"),
            1,
        )],
    })
    .expect("a valid AsyncGeneratorDeclaration dependency must build a Module graph");
    assert_eq!(graph.units.len(), 2);
}

#[test]
fn rejected_async_function_expression_super_dependency_keeps_its_code_through_graph_build() {
    let dependency_source = "export default (async function(value = super()) {});";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/async-function-expression-super.js"),
        dependency_source.to_string(),
        "file:///root/async-function-expression-super.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    let ModuleParse::Rejected { error, .. } = &dependency.parse else {
        panic!("the AsyncFunctionExpression early error must be retained as a rejected parse");
    };
    assert_eq!(
        crate::modules::early::module_parse_failure_diagnostic(error).code(),
        Some(EarlyErrorCode::AsyncFunctionExpressionContainsSuper)
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "a rejected dependency must not be rescanned for requests"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './async-function-expression-super.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./async-function-expression-super.js"),
            1,
        )],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::AsyncFunctionExpressionContainsSuper)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained AsyncFunctionExpression rejection must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn retained_async_function_expression_without_super_builds_a_real_module_graph() {
    let dependency_source = "export default (async function(value) { return value; });";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/async-function-expression.js"),
        dependency_source.to_string(),
        "file:///root/async-function-expression.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    assert_eq!(dependency.module_requests(), Some(Vec::new()));

    let graph = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './async-function-expression.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./async-function-expression.js"),
            1,
        )],
    })
    .expect("a valid AsyncFunctionExpression dependency must build a Module graph");
    assert_eq!(graph.units.len(), 2);
}

#[test]
fn rejected_generator_expression_super_dependency_keeps_its_code_through_graph_build() {
    let dependency_source = "export default (function*(value = super()) {});";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/generator-expression-super.js"),
        dependency_source.to_string(),
        "file:///root/generator-expression-super.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    let ModuleParse::Rejected { error, .. } = &dependency.parse else {
        panic!("the GeneratorExpression early error must be retained as a rejected parse");
    };
    assert_eq!(
        crate::modules::early::module_parse_failure_diagnostic(error).code(),
        Some(EarlyErrorCode::GeneratorExpressionContainsSuper)
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "a rejected dependency must not be rescanned for requests"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './generator-expression-super.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./generator-expression-super.js"),
            1,
        )],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::GeneratorExpressionContainsSuper)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained GeneratorExpression rejection must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn retained_generator_expression_without_super_builds_a_real_module_graph() {
    let dependency_source = "export default (function*(value) { yield value; });";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/generator-expression.js"),
        dependency_source.to_string(),
        "file:///root/generator-expression.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    assert_eq!(dependency.module_requests(), Some(Vec::new()));

    let graph = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './generator-expression.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(0, ModuleRequestKeyIr::plain("./generator-expression.js"), 1)],
    })
    .expect("a valid GeneratorExpression dependency must build a Module graph");
    assert_eq!(graph.units.len(), 2);
}

#[test]
fn rejected_async_generator_expression_super_dependency_keeps_its_code_through_graph_build() {
    let dependency_source = "export default (async function*(value = super()) {});";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/async-generator-expression-super.js"),
        dependency_source.to_string(),
        "file:///root/async-generator-expression-super.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    let ModuleParse::Rejected { error, .. } = &dependency.parse else {
        panic!("the AsyncGeneratorExpression early error must be retained as a rejected parse");
    };
    assert_eq!(
        crate::modules::early::module_parse_failure_diagnostic(error).code(),
        Some(EarlyErrorCode::AsyncGeneratorExpressionContainsSuper)
    );
    assert_eq!(
        dependency.module_requests(),
        None,
        "a rejected dependency must not be rescanned for requests"
    );

    let diagnostics = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './async-generator-expression-super.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./async-generator-expression-super.js"),
            1,
        )],
    })
    .expect_err("the retained rejected dependency must stop graph construction");
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one retained parse diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.kind(), IrDiagnosticKind::LinkError);
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(
        diagnostic.code(),
        Some(EarlyErrorCode::AsyncGeneratorExpressionContainsSuper)
    );
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
    let span = diagnostic
        .span
        .expect("the retained AsyncGeneratorExpression rejection must keep its source span");
    assert!(
        span.start < span.end,
        "{dependency_source:?}: {diagnostic:?}"
    );
}

#[test]
fn retained_async_generator_expression_without_super_builds_a_real_module_graph() {
    let dependency_source =
        "export default (async function*(value) { await value; yield value; });";
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/async-generator-expression.js"),
        dependency_source.to_string(),
        "file:///root/async-generator-expression.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    assert_eq!(dependency.module_requests(), Some(Vec::new()));

    let graph = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './async-generator-expression.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(
            0,
            ModuleRequestKeyIr::plain("./async-generator-expression.js"),
            1,
        )],
    })
    .expect("a valid AsyncGeneratorExpression dependency must build a Module graph");
    assert_eq!(graph.units.len(), 2);
}

#[test]
fn retained_class_owned_super_dependency_builds_a_real_module_graph() {
    let dependency_source = concat!(
        "class Base {};\n",
        "export class Derived extends Base {\n",
        "  constructor() { super(); }\n",
        "  method() { return () => super.value; }\n",
        "  field = super.value;\n",
        "  static { void super.value; }\n",
        "}",
    );
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/class-owned-super.js"),
        dependency_source.to_string(),
        "file:///root/class-owned-super.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    assert_eq!(
        dependency.module_requests(),
        Some(Vec::new()),
        "valid class-owned super must remain a successfully parsed Module"
    );

    let graph = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './class-owned-super.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(0, ModuleRequestKeyIr::plain("./class-owned-super.js"), 1)],
    })
    .expect("valid class-owned super must build a Module graph");
    assert_eq!(graph.units.len(), 2);
}

#[test]
fn retained_import_meta_dependency_keeps_its_module_goal_through_graph_build() {
    let dependency_source = concat!(
        "export const direct = import.meta;\n",
        "export function nested() { return import.meta; }",
    );
    let dependency = ModuleSourceIr::new(
        ModuleKey::from_host("/root/import-meta.js"),
        dependency_source.to_string(),
        "file:///root/import-meta.js".to_string(),
    );
    assert_eq!(dependency.goal(), ParseGoal::Module);
    assert_eq!(
        dependency.module_requests(),
        Some(Vec::new()),
        "direct and nested ImportMeta must remain a successfully parsed Module"
    );

    let graph = build_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './import-meta.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            dependency,
        ],
        entry: 0,
        resolutions: vec![(0, ModuleRequestKeyIr::plain("./import-meta.js"), 1)],
    })
    .expect("a Module dependency containing ImportMeta must build without a parse rejection");

    let dependency = &graph.units[unit_of(&graph, "/root/import-meta.js") as usize];
    assert_eq!(dependency.source_text, dependency_source);
    let import_meta_text: Vec<&str> = dependency
        .record
        .import_meta_sites
        .iter()
        .map(|site| &dependency.source_text[site.start..site.end])
        .collect();
    assert_eq!(import_meta_text, vec!["import.meta", "import.meta"]);
}

/// The default: everything the entry reaches through an ordinary `import`
/// evaluates inline, which is what an unphased graph has always done.
#[test]
fn an_unphased_graph_evaluates_every_unit_eagerly() {
    let graph = linked(&[
        ("/root/entry.js", "import { x } from './a.js';\nx;"),
        ("/root/a.js", "export const x = 1;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    assert_eq!(
        graph.evaluation_modes,
        vec![ModuleEvaluationModeIr::Eager, ModuleEvaluationModeIr::Eager]
    );
}

/// `import defer` is the only edge reaching the dependency, so it is linked
/// but its body waits for the first touch of its namespace.
#[test]
fn a_defer_only_dependency_is_deferred() {
    let graph = linked(&[
        ("/root/entry.js", "import defer * as ns from './a.js';\nns;"),
        ("/root/a.js", "export const x = 1;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    assert_eq!(
        graph.evaluation_mode(unit_of(&graph, "/root/a.js")),
        ModuleEvaluationModeIr::Deferred
    );
}

/// An evaluation-phase importer wins over a deferred one: a module that
/// something evaluates has already run by the time the deferred namespace
/// is touched, and `import defer` of it is then indistinguishable from
/// `import *`.
#[test]
fn a_module_also_imported_eagerly_is_not_deferred() {
    let graph = linked(&[
        (
            "/root/entry.js",
            "import defer * as ns from './a.js';\nimport { x } from './a.js';\nns; x;",
        ),
        ("/root/a.js", "export const x = 1;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    assert_eq!(
        graph.evaluation_mode(unit_of(&graph, "/root/a.js")),
        ModuleEvaluationModeIr::Eager
    );
}

/// `import source` neither evaluates nor instantiates its target — and
/// therefore does not evaluate what that target imports either, which a
/// per-request vote over incoming phases would get wrong.
#[test]
fn a_source_only_module_and_its_own_dependency_never_evaluate() {
    let graph = linked(&[
        ("/root/entry.js", "import source src from './a.js';\nsrc;"),
        ("/root/a.js", "import './b.js';\nexport const x = 1;"),
        ("/root/b.js", "globalThis.ran = true;"),
    ]);
    assert_eq!(
        graph.link_errors,
        vec![ModuleLinkErrorIr::SourceUnavailable {
            referrer: graph.entry,
            request: ModuleRequestIr::from_key(
                ModuleRequestKeyIr::plain("./a.js"),
                ImportPhaseIr::Source,
            ),
        }]
    );
    assert_eq!(
        graph.evaluation_mode(unit_of(&graph, "/root/a.js")),
        ModuleEvaluationModeIr::NotEvaluated
    );
    assert_eq!(
        graph.evaluation_mode(unit_of(&graph, "/root/b.js")),
        ModuleEvaluationModeIr::NotEvaluated
    );
}

/// Loading/linking edges are not evaluation edges. In particular, a defer
/// edge and a source edge pointing back at the importer do not make the two
/// modules an `InnerModuleEvaluation` cycle.
#[test]
fn non_evaluation_phase_edges_do_not_form_an_evaluation_cycle() {
    let graph = linked(&[
        ("/root/entry.js", "import defer * as ns from './a.js';\nns;"),
        (
            "/root/a.js",
            "import source entry from './entry.js';\nexport const x = 1;\nentry;",
        ),
    ]);
    assert_eq!(
        graph.link_errors,
        vec![ModuleLinkErrorIr::SourceUnavailable {
            referrer: unit_of(&graph, "/root/a.js"),
            request: ModuleRequestIr::from_key(
                ModuleRequestKeyIr::plain("./entry.js"),
                ImportPhaseIr::Source,
            ),
        }]
    );
    assert_eq!(
        graph.evaluation_mode(unit_of(&graph, "/root/a.js")),
        ModuleEvaluationModeIr::Deferred
    );
    let components = components(&graph);
    assert_eq!(components.len(), 2, "{components:?}");
    assert!(
        components.iter().all(|component| component.len() == 1),
        "{components:?}"
    );
}

/// A source-phase target is never evaluated, so even a raw `[[HasTLA]]`
/// record inside it cannot turn the linked graph asynchronous.
#[test]
fn non_evaluation_phase_tla_does_not_make_graph_async() {
    let graph = linked(&[
        ("/root/entry.js", "import source src from './a.js';\nsrc;"),
        ("/root/a.js", "export const x = await 1;"),
    ]);
    assert_eq!(
        graph.link_errors,
        vec![ModuleLinkErrorIr::SourceUnavailable {
            referrer: graph.entry,
            request: ModuleRequestIr::from_key(
                ModuleRequestKeyIr::plain("./a.js"),
                ImportPhaseIr::Source,
            ),
        }]
    );
    let target = unit_of(&graph, "/root/a.js");
    assert_eq!(
        graph.evaluation_mode(target),
        ModuleEvaluationModeIr::NotEvaluated
    );
    assert!(graph.has_tla(target));
    assert!(
        graph
            .async_evaluation()
            .iter()
            .all(|asynchronous| !asynchronous),
        "source-only targets do not participate in AsyncModuleExecution"
    );
    assert_eq!(graph.pending_async_dependencies(graph.entry), 0);
    assert_eq!(graph.pending_async_dependencies(target), 0);
}

/// Source-phase resolution retains the source symbol, then initialization
/// rejects the loaded Source Text Module independently of its default export.
#[test]
fn a_source_phase_import_rejects_a_source_text_module() {
    let graph = linked(&[
        ("/root/entry.js", "import source src from './a.js';\nsrc;"),
        ("/root/a.js", "export const x = 1;"),
    ]);
    assert!(matches!(
        graph.link_errors.as_slice(),
        [ModuleLinkErrorIr::SourceUnavailable { .. }]
    ));
    let target = unit_of(&graph, "/root/a.js");
    assert_eq!(
        graph.units[0].resolved_imports,
        vec![ResolvedBindingIr::Resolved {
            module: target,
            binding: ModuleBindingNameIr::ModuleSource,
        }]
    );
    assert_eq!(
        namespace_target_reference(&graph, &graph.units[0].resolved_imports[0]),
        Some(MergedName::minted(target, UnitCellRole::ModuleSource))
    );
}

#[test]
fn deferred_top_level_await_modules_keep_their_linked_evaluation_mode() {
    let graph = linked(&[
        ("/root/entry.js", "import defer * as ns from './a.js';\nns;"),
        ("/root/a.js", "export const x = await 1;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    let target = unit_of(&graph, "/root/a.js");
    assert!(graph.unit(target).record.has_top_level_await);
    assert_eq!(
        graph.evaluation_mode(target),
        ModuleEvaluationModeIr::Deferred
    );
}

#[test]
fn one_key_reached_through_several_specifiers_is_one_unit() {
    let graph = linked(&[
        (
            "/root/entry.js",
            "import { x } from './shared.js';\nimport * as ns from '../root/shared.js';\nx; ns;",
        ),
        ("/root/mid.js", "export { x } from 'shared.js';"),
        ("/root/shared.js", "export const x = 1;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    assert_eq!(graph.units.len(), 3);
    assert_eq!(graph.keys.len(), 3);
    // Three different specifiers, one unit behind all of them.
    let shared = unit_of(&graph, "/root/shared.js");
    assert_eq!(graph.resolutions.len(), 3);
    assert!(graph.resolutions.values().all(|target| *target == shared));
}

/// `ModuleGraphSources::resolutions` is a public embedder boundary. Its
/// request may be constructed independently of the retained parse, so an
/// opposite input order must still name the same ModuleRequest Record.
#[test]
fn a_public_host_resolution_row_matches_canonical_request_attributes() {
    fn attribute(key: &str, value: &str) -> ImportAttributeIr {
        ImportAttributeIr {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    let modules = vec![
        ModuleSourceIr::new(
            ModuleKey::from_host("/root/entry.js"),
            "import { value } from './dep.js' with { charset: 'utf8', syntax: 'javascript' };\n\
             value;"
                .to_string(),
            "file:///root/entry.js".to_string(),
        ),
        ModuleSourceIr::new(
            ModuleKey::from_host("/root/dep.js"),
            "export const value = 41;".to_string(),
            "file:///root/dep.js".to_string(),
        ),
    ];
    let host_request = ModuleRequestKeyIr::try_new(
        "./dep.js",
        // Reverse of the source's canonical order.
        vec![
            attribute("syntax", "javascript"),
            attribute("charset", "utf8"),
        ],
    )
    .expect("host attributes are unique");
    let sources = ModuleGraphSources {
        realm_requests: Default::default(),
        modules,
        entry: 0,
        resolutions: vec![(0, host_request, 1)],
    };

    let mut graph = build_graph(&sources).expect("test modules parse");
    link(&mut graph);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    assert_eq!(
        graph.units[0].resolved_imports,
        vec![ResolvedBindingIr::Resolved {
            module: 1,
            binding: ModuleBindingNameIr::Name(LocalName::from_bound_name("value")),
        }]
    );
}

/// The source's attributed re-export must retain the same request identity
/// independently supplied by an in-memory host resolution row.
#[test]
fn an_attributed_reexport_uses_the_matching_public_host_resolution_row() {
    fn attribute(key: &str, value: &str) -> ImportAttributeIr {
        ImportAttributeIr {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    let modules = vec![
        ModuleSourceIr::new(
            ModuleKey::from_host("/root/entry.js"),
            "export { value as renamed } from './dep.js' with { charset: 'utf8', syntax: 'javascript' };"
                .to_string(),
            "file:///root/entry.js".to_string(),
        ),
        ModuleSourceIr::new(
            ModuleKey::from_host("/root/dep.js"),
            "export const value = 41;".to_string(),
            "file:///root/dep.js".to_string(),
        ),
    ];
    let host_request = ModuleRequestKeyIr::try_new(
        "./dep.js",
        // Reverse of the source's canonical order.
        vec![
            attribute("syntax", "javascript"),
            attribute("charset", "utf8"),
        ],
    )
    .expect("host attributes are unique");
    let sources = ModuleGraphSources {
        realm_requests: Default::default(),
        modules,
        entry: 0,
        resolutions: vec![(0, host_request, 1)],
    };

    let mut graph = build_graph(&sources).expect("test modules parse");
    link(&mut graph);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    let resolved = ResolvedBindingIr::Resolved {
        module: 1,
        binding: ModuleBindingNameIr::Name(LocalName::from_bound_name("value")),
    };
    assert_eq!(
        graph.units[0].resolved_indirect_exports,
        vec![resolved.clone()]
    );
    assert_eq!(
        graph.resolve_export(0, &ExportName::new("renamed")),
        resolved
    );
}

#[test]
fn eval_defer_and_source_occurrences_share_one_resolution_key() {
    let sources = ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './dep.js';\n\
                 import defer * as deferred from './dep.js';\n\
                 import source artifact from './dep.js';\n\
                 deferred; artifact;"
                    .to_string(),
                "file:///root/entry.js".to_string(),
            ),
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/dep.js"),
                "export const value = 41;".to_string(),
                "file:///root/dep.js".to_string(),
            ),
        ],
        entry: 0,
        resolutions: vec![(0, ModuleRequestKeyIr::plain("./dep.js"), 1)],
    };

    let mut graph = build_graph(&sources).expect("test modules parse");
    assert_eq!(graph.resolutions.len(), 1);
    assert_eq!(graph.units[0].record.requested_modules.len(), 3);
    assert_eq!(graph.units[0].record.module_resolution_requests.len(), 1);
    for phase in [
        ImportPhaseIr::Evaluation,
        ImportPhaseIr::Defer,
        ImportPhaseIr::Source,
    ] {
        let request = ModuleRequestIr::from_key(ModuleRequestKeyIr::plain("./dep.js"), phase);
        assert_eq!(graph.resolve_request(0, &request), Some(1));
    }

    link(&mut graph);
    assert_eq!(
        graph.link_errors,
        vec![ModuleLinkErrorIr::SourceUnavailable {
            referrer: graph.entry,
            request: ModuleRequestIr::from_key(
                ModuleRequestKeyIr::plain("./dep.js"),
                ImportPhaseIr::Source,
            ),
        }]
    );
    assert_eq!(graph.evaluation_mode(1), ModuleEvaluationModeIr::Eager);
}

#[test]
fn conflicting_public_resolution_rows_have_no_last_write_winner() {
    let request = ModuleRequestKeyIr::plain("./dep.js");
    let sources = ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "import './dep.js';".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/first.js"),
                "export const first = 1;".to_string(),
                "file:///root/first.js".to_string(),
            ),
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/second.js"),
                "export const second = 2;".to_string(),
                "file:///root/second.js".to_string(),
            ),
        ],
        entry: 0,
        resolutions: vec![(0, request.clone(), 1), (0, request.clone(), 2)],
    };

    let graph = build_graph(&sources).expect("test modules parse");
    assert_eq!(graph.resolve_request_key(0, &request), None);
    assert_eq!(
        graph.link_errors,
        vec![ModuleLinkErrorIr::InconsistentResolution {
            referrer: 0,
            request,
        }]
    );
}

#[test]
fn a_repeated_key_with_different_text_is_one_unit_and_an_inconsistent_load() {
    let sources = ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "export const x = 1;".to_string(),
                "file:///root/entry.js".to_string(),
            ),
            ModuleSourceIr::new(
                ModuleKey::from_host("/root/entry.js"),
                "export const x = 2;".to_string(),
                "file:///root/entry.js".to_string(),
            ),
        ],
        entry: 0,
        resolutions: Vec::new(),
    };
    let graph = build_graph(&sources).expect("test modules parse");
    assert_eq!(graph.units.len(), 1);
    assert_eq!(
        graph.link_errors,
        vec![ModuleLinkErrorIr::InconsistentLoad {
            key: ModuleKey::from_host("/root/entry.js"),
        }]
    );
}

#[test]
fn a_two_node_cycle_is_one_contiguous_component() {
    let graph = linked(&[
        (
            "/root/a.js",
            "import { b } from './b.js';\nexport const a = 1;\nb;",
        ),
        (
            "/root/b.js",
            "import { a } from './a.js';\nexport const b = 2;\na;",
        ),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    assert_eq!(components(&graph), vec![vec![1, 0]]);
}

#[test]
fn a_three_node_cycle_is_one_contiguous_component() {
    let graph = linked(&[
        ("/root/a.js", "import './b.js';\nexport const a = 1;"),
        ("/root/b.js", "import './c.js';\nexport const b = 2;"),
        ("/root/c.js", "import './a.js';\nexport const c = 3;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    let components = components(&graph);
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].len(), 3);
    // The entry is the component root, so it runs last.
    assert_eq!(components[0].last().copied(), Some(graph.entry));
}

// -- `[[HasTLA]]` / `[[AsyncEvaluation]]` ------------------------------

#[test]
fn a_graph_with_no_await_evaluates_synchronously_throughout() {
    let graph = linked(&[
        ("/root/entry.js", "import { x } from './a.js';\nx;"),
        ("/root/a.js", "export const x = 1;"),
    ]);
    assert!(!graph.has_top_level_await());
    assert_eq!(graph.async_evaluation(), vec![false, false]);
    assert_eq!(graph.pending_async_dependencies(graph.entry), 0);
}

/// 16.2.1.5.2 step 11.b.i: an importer inherits its dependency's
/// `[[AsyncEvaluation]]`, and keeps inheriting it up the chain.
#[test]
fn async_evaluation_propagates_transitively_to_every_importer() {
    let graph = linked(&[
        ("/root/entry.js", "import { y } from './mid.js';\ny;"),
        (
            "/root/mid.js",
            "import { x } from './leaf.js';\nexport const y = x;",
        ),
        ("/root/leaf.js", "export const x = await 1;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    let leaf = unit_of(&graph, "/root/leaf.js");
    let mid = unit_of(&graph, "/root/mid.js");
    let entry = unit_of(&graph, "/root/entry.js");

    assert!(graph.has_tla(leaf));
    assert!(!graph.has_tla(mid));
    assert!(!graph.has_tla(entry));

    let asynchronous = graph.async_evaluation();
    assert!(asynchronous[leaf as usize]);
    assert!(asynchronous[mid as usize]);
    assert!(asynchronous[entry as usize]);

    assert_eq!(graph.pending_async_dependencies(leaf), 0);
    assert_eq!(graph.pending_async_dependencies(mid), 1);
    assert_eq!(graph.pending_async_dependencies(entry), 1);
}

/// A synchronous sibling of an asynchronous module stays synchronous: only
/// the dependency edge carries `[[AsyncEvaluation]]`, never mere membership
/// in the same graph.
#[test]
fn a_sibling_that_does_not_import_the_awaiting_module_stays_synchronous() {
    let graph = linked(&[
        (
            "/root/entry.js",
            "import { x } from './a.js';\nimport { y } from './b.js';\nx; y;",
        ),
        ("/root/a.js", "export const x = await 1;"),
        ("/root/b.js", "export const y = 2;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    let asynchronous = graph.async_evaluation();
    assert!(asynchronous[unit_of(&graph, "/root/a.js") as usize]);
    assert!(!asynchronous[unit_of(&graph, "/root/b.js") as usize]);
    assert!(asynchronous[graph.entry as usize]);
    // Two dependencies, one of them asynchronous.
    assert_eq!(graph.pending_async_dependencies(graph.entry), 1);
}

/// A cycle shares one `[[TopLevelCapability]]`, so one member's `await`
/// makes every member asynchronous — and a member never waits on another
/// member, because `InnerModuleEvaluation` evaluates them together.
#[test]
fn one_await_in_a_cycle_makes_the_whole_component_asynchronous() {
    let graph = linked(&[
        (
            "/root/a.js",
            "import { b } from './b.js';\nexport const a = 1;\nb;",
        ),
        (
            "/root/b.js",
            "import { a } from './a.js';\nexport const b = await 2;\na;",
        ),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    assert_eq!(graph.async_evaluation(), vec![true, true]);
    assert_eq!(graph.pending_async_dependencies(0), 0);
    assert_eq!(graph.pending_async_dependencies(1), 0);
}

#[test]
fn a_self_importing_module_is_one_unit() {
    // The import has to be aliased. A self-import of an unaliased name is a
    // duplicate lexical declaration — the import binding `x` and the
    // `export const x` are two declarations of `x` in one module
    // environment — so `import { x } from './a.js'; export const x = 1;` is
    // a SyntaxError rather than a graph shape, and boa rejects it before
    // this file is reached.
    let graph = linked(&[(
        "/root/a.js",
        "import { x as y } from './a.js';\nexport const x = 1;\ny;",
    )]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    assert_eq!(graph.units.len(), 1);
    assert_eq!(components(&graph), vec![vec![0]]);
    assert_eq!(
        graph.units[0].resolved_imports,
        vec![ResolvedBindingIr::Resolved {
            module: 0,
            binding: ModuleBindingNameIr::Name(LocalName::from_bound_name("x")),
        }]
    );
}

#[test]
fn an_unresolved_request_is_the_only_error_it_reports() {
    let graph = linked(&[("/root/entry.js", "import { x } from './missing.js';\nx;")]);
    assert_eq!(
        graph.link_errors,
        vec![ModuleLinkErrorIr::UnresolvedModule {
            referrer: 0,
            request: ModuleRequestKeyIr::plain("./missing.js"),
        }]
    );
}

#[test]
fn dependencies_evaluate_before_the_modules_that_import_them() {
    let graph = linked(&[
        ("/root/entry.js", "import './a.js';\nimport './b.js';"),
        ("/root/a.js", "export const a = 1;"),
        ("/root/b.js", "export const b = 2;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    let a = unit_of(&graph, "/root/a.js");
    let b = unit_of(&graph, "/root/b.js");
    assert_eq!(graph.evaluation_order, vec![a, b, graph.entry]);
}

#[test]
fn an_earlier_source_occurrence_does_not_reorder_later_evaluation_dependencies() {
    let graph = linked(&[
        (
            "/root/entry.js",
            "import source artifact from './m.js';\n\
             import './n.js';\n\
             import './m.js';\n\
             artifact;",
        ),
        ("/root/m.js", "export const m = 1;"),
        ("/root/n.js", "export const n = 1;"),
    ]);
    assert_eq!(
        graph.link_errors,
        vec![ModuleLinkErrorIr::SourceUnavailable {
            referrer: graph.entry,
            request: ModuleRequestIr::from_key(
                ModuleRequestKeyIr::plain("./m.js"),
                ImportPhaseIr::Source,
            ),
        }]
    );
    let m = unit_of(&graph, "/root/m.js");
    let n = unit_of(&graph, "/root/n.js");
    assert_eq!(graph.evaluation_order, vec![n, m, graph.entry]);
}

#[test]
fn an_earlier_defer_occurrence_does_not_reorder_later_evaluation_dependencies() {
    let graph = linked(&[
        (
            "/root/entry.js",
            "import defer * as deferred from './m.js';\n\
             import './n.js';\n\
             import './m.js';\n\
             deferred;",
        ),
        ("/root/m.js", "export const m = 1;"),
        ("/root/n.js", "export const n = 1;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    let m = unit_of(&graph, "/root/m.js");
    let n = unit_of(&graph, "/root/n.js");
    assert_eq!(graph.evaluation_order, vec![n, m, graph.entry]);
    assert_eq!(graph.evaluation_mode(m), ModuleEvaluationModeIr::Eager);
}

#[test]
fn an_indirect_export_chain_resolves_to_the_module_that_declares_it() {
    let graph = linked(&[
        ("/root/entry.js", "import { x } from './a.js';\nx;"),
        ("/root/a.js", "export { x } from './b.js';"),
        ("/root/b.js", "export { x } from './c.js';"),
        ("/root/c.js", "export const x = 1;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    let c = unit_of(&graph, "/root/c.js");
    let resolved = ResolvedBindingIr::Resolved {
        module: c,
        binding: ModuleBindingNameIr::Name(LocalName::from_bound_name("x")),
    };
    assert_eq!(graph.units[0].resolved_imports, vec![resolved.clone()]);
    // Every link of the chain points at the one declaring cell.
    let a = unit_of(&graph, "/root/a.js");
    assert_eq!(
        graph.unit(a).resolved_indirect_exports,
        vec![resolved.clone()]
    );
    // The merged scope names the exporter's binding exactly as the
    // exporter spells it — no `$m{unit}$` prefix — which is what makes an
    // importer's read a read of the exporter's own cell.
    assert_eq!(
        namespace_target_reference(&graph, &resolved),
        Some(graph.unit(c).merged(&LocalName::from_bound_name("x")))
    );
    assert_eq!(
        namespace_target_reference(&graph, &resolved).map(|name| name.as_str().to_string()),
        Some("x".to_string())
    );
}

#[test]
fn an_indirect_export_of_a_missing_name_fails_at_link_with_no_importer() {
    // Nothing imports `nope`; the re-export alone must fail.
    let graph = linked(&[
        ("/root/entry.js", "import './a.js';"),
        ("/root/a.js", "export { nope } from './b.js';"),
        ("/root/b.js", "export const x = 1;"),
    ]);
    let a = unit_of(&graph, "/root/a.js");
    assert_eq!(
        graph.link_errors,
        vec![ModuleLinkErrorIr::MissingExport {
            referrer: a,
            request: ModuleRequestIr::plain("./b.js"),
            import_name: ExportName::new("nope"),
        }]
    );
}

#[test]
fn two_star_paths_to_different_bindings_are_ambiguous() {
    let graph = linked(&[
        ("/root/entry.js", "import { x } from './a.js';\nx;"),
        (
            "/root/a.js",
            "export * from './b.js';\nexport * from './c.js';",
        ),
        ("/root/b.js", "export const x = 1;"),
        ("/root/c.js", "export const x = 2;"),
    ]);
    let a = unit_of(&graph, "/root/a.js");
    assert_eq!(
        graph.link_errors,
        vec![ModuleLinkErrorIr::AmbiguousExport {
            module: a,
            export_name: ExportName::new("x"),
        }]
    );
    assert_eq!(
        graph.resolve_export(a, &ExportName::new("x")),
        ResolvedBindingIr::Ambiguous
    );
}

#[test]
fn two_star_paths_to_the_same_binding_are_not_ambiguous() {
    let graph = linked(&[
        ("/root/entry.js", "import { x } from './a.js';\nx;"),
        (
            "/root/a.js",
            "export * from './b.js';\nexport * from './c.js';",
        ),
        ("/root/b.js", "export * from './d.js';"),
        ("/root/c.js", "export * from './d.js';"),
        ("/root/d.js", "export const x = 1;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    let d = unit_of(&graph, "/root/d.js");
    assert_eq!(
        graph.units[0].resolved_imports,
        vec![ResolvedBindingIr::Resolved {
            module: d,
            binding: ModuleBindingNameIr::Name(LocalName::from_bound_name("x")),
        }]
    );
}

#[test]
fn a_cycle_of_export_stars_terminates_and_collects_both_sides() {
    let graph = linked(&[
        ("/root/entry.js", "import { x, y } from './a.js';\nx; y;"),
        ("/root/a.js", "export * from './b.js';\nexport const x = 1;"),
        ("/root/b.js", "export * from './a.js';\nexport const y = 2;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    let a = unit_of(&graph, "/root/a.js");
    let mut names = graph.exported_names(a);
    names.sort();
    assert_eq!(names, vec![ExportName::new("x"), ExportName::new("y")]);
}

#[test]
fn default_is_not_reachable_through_export_star() {
    let graph = linked(&[
        ("/root/entry.js", "import d from './a.js';\nd;"),
        ("/root/a.js", "export * from './b.js';"),
        ("/root/b.js", "export default 1;\nexport const z = 3;"),
    ]);
    let a = unit_of(&graph, "/root/a.js");
    assert_eq!(graph.exported_names(a), vec![ExportName::new("z")]);
    assert_eq!(
        graph.resolve_export(a, &ExportName::default_export()),
        ResolvedBindingIr::NotFound
    );
    assert_eq!(
        graph.link_errors,
        vec![ModuleLinkErrorIr::MissingExport {
            referrer: 0,
            request: ModuleRequestIr::plain("./a.js"),
            import_name: ExportName::default_export(),
        }]
    );
}

#[test]
fn a_namespace_import_resolves_to_the_namespace_cell() {
    let graph = linked(&[
        ("/root/entry.js", "import * as ns from './a.js';\nns;"),
        ("/root/a.js", "export const x = 1;"),
    ]);
    assert!(graph.link_errors.is_empty(), "{:?}", graph.link_errors);
    let a = unit_of(&graph, "/root/a.js");
    let binding = ResolvedBindingIr::Resolved {
        module: a,
        binding: ModuleBindingNameIr::Namespace(ModuleNamespaceModeIr::Eager),
    };
    assert_eq!(graph.units[0].resolved_imports, vec![binding.clone()]);
    assert_eq!(
        namespace_target_reference(&graph, &binding),
        Some(MergedName::minted(a, UnitCellRole::Namespace))
    );
}

/// Ledger R3: unit ids are spelled into two length-preserving rewrites, so
/// a graph the linker cannot name is rejected where the id is minted rather
/// than saturated into a ten-digit id that violates both byte budgets and
/// fails later with a confusing message.
#[test]
fn a_graph_larger_than_the_unit_id_cap_is_rejected_at_the_mint_site() {
    let over_cap = usize::try_from(MAX_LINKABLE_MODULE_UNIT_ID).expect("cap fits") + 2;
    let empty = scriptw_front::parse("", scriptw_front::ParseOptions::module())
        .expect("empty module parses");
    let ParsedSource::Module(empty) = empty else {
        unreachable!("module options produce a module")
    };
    let modules: Vec<ModuleSourceIr> = (0..over_cap)
        .map(|index| {
            ModuleSourceIr::from_parsed(
                ModuleKey::from_host(format!("/root/m{index}.js")),
                format!("file:///root/m{index}.js"),
                empty.clone(),
            )
        })
        .collect();
    let sources = ModuleGraphSources {
        realm_requests: Default::default(),
        modules,
        entry: 0,
        resolutions: Vec::new(),
    };
    let diagnostics = build_graph(&sources).expect_err("the graph exceeds the unit-id cap");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == Some(EarlyErrorCode::ModuleTooManyUnits)),
        "{diagnostics:?}"
    );
}

/// The cap admits everything up to and including itself, so the rejection
/// above is not off by one.
#[test]
fn the_unit_id_cap_is_inclusive() {
    assert!(ModuleUnitId::try_from(
        usize::try_from(MAX_LINKABLE_MODULE_UNIT_ID).expect("cap fits")
    )
    .is_ok_and(|id| id <= MAX_LINKABLE_MODULE_UNIT_ID));
}

#[path = "graph_source_phase_export_tests.rs"]
mod source_phase_exports;
