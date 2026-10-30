use scriptw_front::{parse, ParseGoal, ParseOptions, ParsedSource};
use scriptw_ir::{
    lower_module_graph, lower_script_graph, ExprIr, FunctionExecutionKind, FunctionFlavor,
    FunctionProtocolIr, ModuleEvaluationModeIr, ModuleExecutionGraphIr, ModuleGraphSources,
    ModuleKey, ModuleSourceIr, ProgramIr, ScriptIr, StatementIr,
};

fn sources(files: &[(&str, &str)], goal: ParseGoal) -> ModuleGraphSources {
    let modules: Vec<_> = files
        .iter()
        .enumerate()
        .map(|(index, (name, source))| {
            let key = ModuleKey::from_host(*name);
            let url = format!("file:///{}", key.as_str());
            if index == 0 && goal == ParseGoal::Script {
                let ParsedSource::Script(parsed) =
                    parse(*source, ParseOptions::script()).expect("Script fixture parses")
                else {
                    panic!("Script parse goal");
                };
                ModuleSourceIr::from_parsed_script(key, url, parsed)
            } else {
                ModuleSourceIr::new(key, (*source).into(), url)
            }
        })
        .collect();
    let mut resolutions = Vec::new();
    for (referrer, module) in modules.iter().enumerate() {
        for request in module.module_requests().unwrap_or_default() {
            let target = files
                .iter()
                .position(|(name, _)| *name == request.specifier().trim_start_matches("./"))
                .expect("fixture target exists");
            resolutions.push((referrer as u32, request, target as u32));
        }
    }
    ModuleGraphSources {
        realm_requests: Default::default(),
        modules,
        entry: 0,
        resolutions,
    }
}

fn module_program(files: &[(&str, &str)]) -> ProgramIr {
    let program = lower_module_graph(&sources(files, ParseGoal::Module));
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    program
}

fn activation_graph(script: &ScriptIr) -> Option<&ModuleExecutionGraphIr> {
    script.body.statements.iter().find_map(|statement| {
        let StatementIr::Expression(expression) = statement else {
            return None;
        };
        let ExprIr::ModuleExecutionGraph(graph) = &expression.expr else {
            return None;
        };
        Some(graph.as_ref())
    })
}

#[test]
fn nested_module_import_captures_do_not_infer_source_placeholder_values() {
    module_program(&[
        (
            "entry.js",
            "import defer * as namespace from './value.js'; import { value } from './value.js'; for (const read of [() => namespace.value, () => value.property]) print(read());",
        ),
        ("value.js", "export let value = { property: 3 };"),
    ]);
}

#[test]
fn synchronous_deferred_modules_have_private_owners_and_canonical_environment_slots() {
    let program = module_program(&[
        (
            "entry.js",
            "{}; import defer * as first from './first.js'; print(first.value); {};",
        ),
        (
            "first.js",
            "import { value as next } from './last.js'; export let value = next; export function read() { return next; }",
        ),
        ("last.js", "{}; export const { value } = { value: 3 }; {};"),
    ]);
    let modules = program.modules.as_ref().expect("linked graph");
    assert_eq!(modules.evaluation_mode(0), ModuleEvaluationModeIr::Eager);
    assert_eq!(modules.evaluation_mode(1), ModuleEvaluationModeIr::Deferred);
    assert_eq!(modules.evaluation_mode(2), ModuleEvaluationModeIr::Deferred);
    let script = program.script.as_ref().expect("linked Script IR");
    let graph = activation_graph(script).expect("private graph operation");
    assert_eq!(graph.record_count(), 3);
    assert_eq!(graph.activations().len(), 3);
    assert!(
        !script
            .global_bindings
            .lexical_names()
            .any(|name| name.contains("namespace")),
        "private namespace cells must not become Script bindings"
    );
    for activation in graph.activations() {
        let owner = script
            .functions
            .iter()
            .find(|function| &function.id == activation.function())
            .expect("private owner remains reachable");
        assert_eq!(owner.protocol, FunctionProtocolIr::ModuleActivation);
        assert_eq!(owner.protocol.flavor(), FunctionFlavor::Arrow);
        assert_eq!(
            owner.protocol.execution_kind(),
            FunctionExecutionKind::Generator
        );
        assert!(!owner.protocol.is_constructable());
        let suspension = owner
            .generator_plan
            .as_ref()
            .expect("private suspension plan");
        assert_eq!(suspension.state_count, 2);
        assert_eq!(suspension.suspension_points.len(), 1);
        assert!(owner.strict);
        assert!(!owner.captures_lexical_arguments);
        assert_eq!(
            activation.kind(),
            scriptw_ir::ModuleActivationKindIr::Synchronous
        );
    }
    let last = script
        .functions
        .iter()
        .find(|function| &function.id == graph.activations()[2].function())
        .unwrap();
    assert!(last
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == "value"));
}

#[test]
fn private_source_spans_survive_ecmascript_line_terminators_and_ordinary_arrays() {
    for terminator in ["\r", "\r\n", "\n", "\u{2028}", "\u{2029}"] {
        let source = format!(
            "const ordinary = [async () => {{ return 1; }}, () => 0];{terminator}\
             export default function () {{ return ordinary; }}"
        );
        let program = module_program(&[
            (
                "entry.js",
                "import defer * as ns from './value.js'; print(ns.default.name);",
            ),
            ("value.js", &source),
        ]);
        let script = program.script.as_ref().unwrap();
        assert_eq!(activation_graph(script).unwrap().activations().len(), 2);
        assert_eq!(
            script
                .functions
                .iter()
                .filter(|function| function.protocol == FunctionProtocolIr::ModuleActivation)
                .count(),
            2
        );
        assert!(script
            .functions
            .iter()
            .any(|function| function.protocol == FunctionProtocolIr::AsyncArrow));
    }
}

#[test]
fn tla_and_script_graphs_use_private_owners_and_static_source_rejects() {
    let asynchronous = module_program(&[
        (
            "entry.js",
            "{}; import defer * as ns from './value.js'; {}; await 0; print(ns.value); {};",
        ),
        ("value.js", "export const value = 2;"),
    ]);
    let async_script = asynchronous.script.as_ref().unwrap();
    let async_graph = activation_graph(async_script).unwrap();
    assert_eq!(
        async_graph.activations()[0].kind(),
        scriptw_ir::ModuleActivationKindIr::Async
    );
    let owner = async_script
        .functions
        .iter()
        .find(|function| &function.id == async_graph.activations()[0].function())
        .unwrap();
    assert_eq!(owner.protocol, FunctionProtocolIr::AsyncModuleActivation);
    assert_eq!(
        owner.protocol.source_execution_kind(),
        FunctionExecutionKind::Async
    );
    assert!(!owner.protocol.is_constructable());
    assert!(!owner.captures_lexical_arguments);
    assert!(owner.generator_plan.is_none());
    fn contains_boundary(statements: &[StatementIr]) -> bool {
        statements.iter().any(|statement| match statement {
            StatementIr::AsyncModuleInstantiation => true,
            StatementIr::Block(block) => contains_boundary(&block.statements),
            StatementIr::LexicalBlock(statements) => contains_boundary(statements),
            _ => false,
        })
    }
    assert!(contains_boundary(&owner.body.statements));
    assert!(async_script.prepared_scripts.is_empty());
    let script = lower_script_graph(&sources(
        &[
            (
                "entry.js",
                "import.defer('./value.js').then(ns => print(ns.value));",
            ),
            ("value.js", "export const value = 2;"),
        ],
        ParseGoal::Script,
    ));
    assert!(script.is_wasm_supported(), "{:?}", script.diagnostics);
    let script = script.script.as_ref().unwrap();
    let graph = activation_graph(script).expect("a Script owns the canonical module graph");
    assert_eq!(graph.record_count(), 2);
    assert_eq!(graph.activations().len(), 1);
    assert_eq!(graph.activations()[0].module(), 1);
    assert!(
        script
            .global_bindings
            .iter()
            .all(|binding| { !binding.name.starts_with("$scriptw$module$") }),
        "compiler-owned dispatchers must stay outside the observable Global Environment"
    );
    assert!(script
        .global_bindings
        .lexical_names()
        .all(|name| { !name.starts_with("$scriptw$module$") }));
    assert!(!script.body.statements.iter().any(|statement| {
        matches!(statement, StatementIr::Expression(expression)
            if matches!(expression.expr, ExprIr::ModuleEvaluate(_) | ExprIr::ModuleEntryEvaluation(_)))
    }), "a Script imports modules only through its import continuations");
    let source_phase = lower_module_graph(&sources(&[
        (
            "entry.js",
            "import source source from './source.js'; import defer * as ns from './value.js'; source; print(ns.value);",
        ),
        ("source.js", "export const value = 1;"),
        ("value.js", "export const value = 2;"),
    ], ParseGoal::Module));
    assert!(source_phase.script.is_none());
    assert!(source_phase
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code()
            == Some(scriptw_ir::EarlyErrorCode::ModuleSourceUnavailable)));
}

#[test]
fn flat_lowering_keeps_module_lexical_this_outside_the_root_script() {
    let program = lower_script_graph(&sources(
        &[
            ("entry.js", "import('./target.js'); (() => this);"),
            (
                "target.js",
                "export const directArrow = () => this; export const nestedArrow = () => () => this; export function ordinary() { return this; } export function arrowFromCall() { return () => this; }",
            ),
        ],
        ParseGoal::Script,
    ));
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.expect("Script with canonical Module owners");
    let returns = |function: &scriptw_ir::FunctionIr, expected_undefined: bool| {
        assert!(
            function.body.statements.iter().any(|statement| {
                matches!(statement, StatementIr::Return(value)
                if if expected_undefined { matches!(value.expr, ExprIr::Undefined) }
                   else { matches!(value.expr, ExprIr::This) })
            }),
            "{} has the appropriate source this binding",
            function.name
        );
    };
    for name in ["directArrow", "ordinary"] {
        let function = script
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("{name} remains in the flat function registry"));
        returns(function, name == "directArrow");
    }
    // Engine controls exercise nested-arrow invocation and an ordinary
    // function's returned arrow with a non-default receiver.
}

#[test]
fn module_eval_named_environments_never_expose_compiler_dispatchers() {
    let program = module_program(&[
        (
            "entry.js",
            "export const hidden = eval('typeof $scriptw$module$import$0'); import('./value.js');",
        ),
        ("value.js", "export const value = 42;"),
    ]);
    let script = program.script.expect("canonical Module source");
    let scriptw_ir::EvalEnvironmentRoleIr::Declarative { bindings, .. } = script
        .eval_environment
        .expect("direct eval retains the root's named environment")
    else {
        panic!("module root has a declarative environment")
    };
    assert!(
        bindings
            .iter()
            .all(|binding| { !binding.source_name.starts_with("$scriptw$module$") }),
        "physical private slots never become source-visible eval bindings"
    );
}

#[test]
fn a_script_with_only_rejected_imports_retains_its_root_and_no_activation() {
    let program = lower_script_graph(&sources(
        &[
            (
                "entry.js",
                "var rootValue = 7; import('./invalid.js').catch(() => 0); rootValue;",
            ),
            ("invalid.js", "export const = 1;"),
        ],
        ParseGoal::Script,
    ));
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("preserved entry Script");
    let graph = activation_graph(script).expect("rejected import jobs still own graph state");
    assert_eq!(graph.record_count(), 1);
    assert!(graph.activations().is_empty());
    assert!(script
        .global_bindings
        .iter()
        .any(|binding| binding.name == "rootValue"));
    assert!(!script.body.statements.iter().any(|statement| {
        matches!(statement, StatementIr::Expression(expression)
            if matches!(expression.expr, ExprIr::ModuleEvaluate(_) | ExprIr::ModuleEntryEvaluation(_)))
    }));
}

#[test]
fn host_resolutions_cannot_select_a_script_record_as_a_module_target() {
    let graph = sources(&[("entry.js", "import('./entry.js');")], ParseGoal::Script);
    // This embedder supplied only the Script Record at that URL. A proper
    // host loader supplies a separate Module parse product for this import.
    assert_eq!(graph.resolutions[0].2, 0);
    let program = lower_script_graph(&graph);
    assert!(!program.is_wasm_supported());
    assert!(program.script.is_none());
    assert!(program.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("host module resolution selected a Script Record")
    }));
}

#[test]
fn script_source_jobs_use_the_canonical_owner_without_target_activations() {
    for resolved in [false, true] {
        let mut graph = sources(
            &[
                ("entry.js", "import.source('./source.js');"),
                ("source.js", "export const unused = 1;"),
            ],
            ParseGoal::Script,
        );
        if !resolved {
            graph.modules.truncate(1);
            graph.resolutions.clear();
        }
        let program = lower_script_graph(&graph);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let graph = activation_graph(program.script.as_ref().unwrap()).unwrap();
        assert_eq!(graph.record_count(), 1);
        assert!(graph.activations().is_empty());
    }
}

#[test]
fn original_module_parse_rejects_syntax_an_async_wrapper_could_otherwise_admit() {
    for source in ["return 1;", "new.target;", "yield 1;", "with ({}) {}"] {
        let graph = sources(
            &[
                ("entry.js", "import defer * as ns from './invalid.js'; ns;"),
                ("invalid.js", source),
            ],
            ParseGoal::Module,
        );
        let program = lower_module_graph(&graph);
        assert!(!program.is_wasm_supported(), "{source}");
        assert!(
            program.script.is_none(),
            "the rejected Module must not reach generated Script parsing"
        );
        assert!(!program.diagnostics.is_empty());
    }
}

#[test]
fn global_script_prelude_is_separate_from_module_and_dynamic_eval_candidates() {
    let ParsedSource::Script(prelude) = parse(
        "function helper() { return this; } let globalLexical = 4;",
        ParseOptions::script(),
    )
    .expect("global Script parses") else {
        panic!("Script parse goal")
    };
    let program = scriptw_ir::lower_module_graph_with_prelude(
        &sources(
            &[
                (
                    "entry.js",
                    "import defer * as ns from './dependency.js'; print(helper(), ns.value);",
                ),
                ("dependency.js", "export const value = globalLexical;"),
            ],
            ParseGoal::Module,
        ),
        &prelude,
        scriptw_ir::HostSurfacePolicy::Test262,
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.unwrap();
    let prelude = script
        .module_prelude
        .as_ref()
        .expect("separate Script thunk");
    assert_eq!(prelude.kind, scriptw_ir::PreparedScriptKind::RealmScript);
    assert!(!prelude.strict);
    assert!(prelude
        .global_bindings
        .iter()
        .any(|binding| binding.name == "helper"));
    assert!(prelude
        .global_bindings
        .lexical_names()
        .any(|name| name == "globalLexical"));
    assert!(
        script.prepared_scripts.is_empty(),
        "prelude is not an eval candidate"
    );
    assert!(activation_graph(&script).is_some());
    let helper = script
        .functions
        .iter()
        .find(|function| function.name == "helper")
        .unwrap();
    assert!(
        !helper.strict,
        "Module strictness must not affect a Script function"
    );
    let ids: std::collections::BTreeSet<_> = script.functions.iter().map(|f| &f.id).collect();
    assert_eq!(
        ids.len(),
        script.functions.len(),
        "source units share allocation IDs"
    );
}

#[test]
fn source_rejection_jobs_keep_module_lexical_owners_outside_global_script() {
    let ParsedSource::Script(prelude) = parse(
        "const shared = 'global'; function helperRead() { return shared; }",
        ParseOptions::script(),
    )
    .unwrap() else {
        panic!("Script parse goal")
    };
    let program = scriptw_ir::lower_module_graph_with_prelude(&sources(&[
        ("entry.js", "import.source('./source.js'); const shared = 'entry'; var localVar; helperRead();"),
        ("source.js", "export const unused = 1;")], ParseGoal::Module),
        &prelude, scriptw_ir::HostSurfacePolicy::Test262);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().unwrap();
    let graph = activation_graph(script).unwrap();
    assert_eq!(graph.activations().len(), 1);
    assert!(script.global_bindings.lexical_names().next().is_none());
    assert!(script.prepared_scripts.is_empty());
    let owner = script
        .functions
        .iter()
        .find(|function| &function.id == graph.activations()[0].function())
        .unwrap();
    assert_eq!(owner.protocol, FunctionProtocolIr::ModuleActivation);
    assert!(owner.strict);
    assert!(!owner.captures_lexical_arguments);
    assert!(script
        .module_prelude
        .as_ref()
        .unwrap()
        .global_bindings
        .lexical_names()
        .any(|name| name == "shared"));
}

#[test]
fn ordinary_modules_with_colliding_names_use_distinct_activation_owners() {
    let program = module_program(&[
        (
            "entry.js",
            "import { read as imported } from './dependency.js'; const shared = 'entry'; var localVar; function localFunction() {} print(imported(), shared);",
        ),
        (
            "dependency.js",
            "const shared = 'dependency'; var localVar; function localFunction() {} export function read() { return shared; }",
        ),
    ]);
    let script = program.script.as_ref().unwrap();
    let graph = activation_graph(script).expect("ordinary Module graph uses canonical owners");
    assert_eq!(graph.activations().len(), 2);
    assert_ne!(
        graph.activations()[0].function(),
        graph.activations()[1].function()
    );
    assert!(script.global_bindings.lexical_names().next().is_none());
    assert!(script
        .global_bindings
        .iter()
        .all(|binding| { binding.declarations == scriptw_ir::GlobalDeclarationSetIr::None }));
    assert!(script.prepared_scripts.is_empty());
    for activation in graph.activations() {
        let owner = script
            .functions
            .iter()
            .find(|function| &function.id == activation.function())
            .unwrap();
        assert_eq!(owner.protocol, FunctionProtocolIr::ModuleActivation);
        assert!(owner.strict);
        assert!(!owner.captures_lexical_arguments);
        assert!(owner
            .owned_env_bindings
            .iter()
            .any(|binding| binding.name == "shared"));
    }
}

#[test]
fn synchronous_cycles_keep_ordered_requests_for_runtime_component_selection() {
    let program = module_program(&[
        (
            "entry.js",
            "import './left.js'; import './external.js'; import './right.js'; print('entry');",
        ),
        ("left.js", "import './entry.js'; print('left');"),
        ("external.js", "print('external');"),
        ("right.js", "import './entry.js'; print('right');"),
    ]);
    let script = program.script.as_ref().unwrap();
    let graph = activation_graph(script).expect("evaluation cycles use canonical owners");
    assert_eq!(graph.activations().len(), 4);
    let expected_dependencies = [vec![1, 2, 3], vec![0], vec![], vec![0]];
    for activation in graph.activations() {
        assert_eq!(
            activation.kind(),
            scriptw_ir::ModuleActivationKindIr::Synchronous
        );
        assert_eq!(
            activation
                .requests()
                .iter()
                .map(scriptw_ir::ModuleExecutionRequestIr::target)
                .collect::<Vec<_>>(),
            expected_dependencies[activation.module() as usize]
        );
        assert!(activation
            .requests()
            .iter()
            .all(|request| request.phase() == scriptw_ir::ModuleRequestPhaseIr::Evaluation));
        assert!(script
            .functions
            .iter()
            .any(|function| &function.id == activation.function()));
    }
    let initial = script
        .body
        .statements
        .iter()
        .filter_map(|statement| {
            let StatementIr::Expression(expression) = statement else {
                return None;
            };
            let ExprIr::ModuleEntryEvaluation(entry) = &expression.expr else {
                return None;
            };
            let ExprIr::ModuleEvaluate(plan) = &entry.evaluation().expr else {
                panic!("the synchronous entry owns its actual evaluation operation");
            };
            Some(plan.module())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        initial,
        [0],
        "static dependencies must be visited through entry DFS"
    );
}

#[test]
fn deferred_evaluation_components_remain_deferred_and_self_cycles_are_admitted() {
    let program = module_program(&[
        (
            "entry.js",
            "import defer * as cycle from './left.js'; print(cycle.value);",
        ),
        ("left.js", "import './right.js'; export const value = 1;"),
        ("right.js", "import './left.js';"),
    ]);
    let modules = program.modules.as_ref().unwrap();
    assert_eq!(modules.evaluation_mode(1), ModuleEvaluationModeIr::Deferred);
    assert_eq!(modules.evaluation_mode(2), ModuleEvaluationModeIr::Deferred);
    assert_eq!(
        activation_graph(program.script.as_ref().unwrap())
            .unwrap()
            .activations()
            .len(),
        3
    );
    let self_cycle = module_program(&[(
        "entry.js",
        "import * as self from './entry.js'; export const value = 1; print(self.value);",
    )]);
    assert_eq!(
        activation_graph(self_cycle.script.as_ref().unwrap())
            .unwrap()
            .activations()
            .len(),
        1
    );
}

#[test]
fn synchronous_module_try_scopes_do_not_allocate_source_resume_states() {
    fn flatten_scopes<'a>(statements: &'a [StatementIr], flattened: &mut Vec<&'a StatementIr>) {
        for statement in statements {
            match statement {
                StatementIr::Block(block) => flatten_scopes(&block.statements, flattened),
                StatementIr::LexicalBlock(statements) => flatten_scopes(statements, flattened),
                statement => flattened.push(statement),
            }
        }
    }

    for prefix in ["", "import defer * as namespace from './dependency.js';"] {
        let source = format!(
            r#"{prefix}
for (let index = 0; index < 2; index++) {{
  try {{ throw index; }} catch (error) {{ print(error); }}
  try {{ print(index); }} finally {{ print('finally'); }}
  try {{ throw index; }} catch (error) {{ print(error); }} finally {{ print('both'); }}
}}
print('after loop');
"#
        );
        let program = module_program(&[
            ("entry.js", &source),
            ("dependency.js", "export const value = 1;"),
        ]);
        let script = program.script.as_ref().unwrap();
        let activation = activation_graph(script)
            .unwrap()
            .activations()
            .iter()
            .find(|activation| activation.module() == 0)
            .unwrap();
        let owner = script
            .functions
            .iter()
            .find(|function| &function.id == activation.function())
            .unwrap();
        let plan = owner.generator_plan.as_ref().unwrap();
        assert_eq!(plan.state_count, 2);
        assert_eq!(plan.suspension_points.len(), 1);
        assert_eq!(plan.suspension_points[0].suspend_state, 0);
        assert_eq!(plan.suspension_points[0].resume_state, 1);
        let mut statements = Vec::new();
        flatten_scopes(&owner.body.statements, &mut statements);
        assert_eq!(
            statements
                .iter()
                .filter(|statement| matches!(statement, StatementIr::GeneratorYield { .. }))
                .count(),
            1
        );
        let loop_index = statements
            .iter()
            .position(|statement| matches!(statement, StatementIr::For { .. }))
            .expect("source loop retains ordinary control flow");
        assert!(statements[loop_index + 1..]
            .iter()
            .any(|statement| matches!(statement, StatementIr::Expression(_))));
        let StatementIr::For { body, .. } = statements[loop_index] else {
            unreachable!();
        };
        let StatementIr::Block(block) = body.as_ref() else {
            panic!("loop fixture owns a block");
        };
        let mut loop_statements = Vec::new();
        flatten_scopes(&block.statements, &mut loop_statements);
        let mut forms = Vec::new();
        for statement in loop_statements {
            let (form, generator_plan, async_plan) = match statement {
                StatementIr::TryCatch {
                    generator_plan,
                    async_plan,
                    ..
                } => ("catch", generator_plan, async_plan),
                StatementIr::TryFinally {
                    generator_plan,
                    async_plan,
                    ..
                } => ("finally", generator_plan, async_plan),
                StatementIr::TryCatchFinally {
                    generator_plan,
                    async_plan,
                    ..
                } => ("catch finally", generator_plan, async_plan),
                _ => continue,
            };
            assert!(
                generator_plan.is_none(),
                "module {form} cannot change the private boundary state"
            );
            assert!(
                async_plan.is_none(),
                "a synchronous module has no async try plan"
            );
            forms.push(form);
        }
        assert_eq!(forms, ["catch", "finally", "catch finally"]);
    }
}

#[test]
fn synchronous_module_resources_have_no_suspension_lifetime() {
    let program = module_program(&[(
        "entry.js",
        "using resource = { [Symbol.dispose]() {} }; try { throw 1; } catch (error) { print(error); } print('after catch');",
    )]);
    let script = program.script.as_ref().unwrap();
    let activation = activation_graph(script)
        .unwrap()
        .activations()
        .first()
        .unwrap();
    let owner = script
        .functions
        .iter()
        .find(|function| &function.id == activation.function())
        .unwrap();
    fn resource_scope(
        statements: &[StatementIr],
    ) -> Option<&scriptw_ir::SyncDisposableScopeExecutionIr> {
        statements.iter().find_map(|statement| match statement {
            StatementIr::SyncDisposableScope { execution, .. } => Some(execution),
            StatementIr::Block(block) => resource_scope(&block.statements),
            StatementIr::LexicalBlock(statements) => resource_scope(statements),
            _ => None,
        })
    }
    assert!(matches!(
        resource_scope(&owner.body.statements),
        Some(scriptw_ir::SyncDisposableScopeExecutionIr::Immediate)
    ));
}

#[test]
fn synchronous_module_resource_forms_use_the_canonical_owner_lifetime() {
    for source in [
        "using resource = null; print('after');",
        "{ using resource = null; } print('after');",
        "for (using resource = null; false;) {} print('after');",
        "for (using resource of [null]) {} print('after');",
        "function nested() { using resource = null; return 1; } print(nested());",
    ] {
        let program = module_program(&[("entry.js", source)]);
        let script = program.script.as_ref().unwrap();
        let activation = activation_graph(script)
            .unwrap()
            .activations()
            .first()
            .unwrap();
        let owner = script
            .functions
            .iter()
            .find(|function| &function.id == activation.function())
            .unwrap();
        assert_eq!(owner.protocol, FunctionProtocolIr::ModuleActivation);
        let plan = owner.generator_plan.as_ref().unwrap();
        assert_eq!(plan.state_count, 2, "{source}");
        assert_eq!(plan.suspension_points.len(), 1, "{source}");
    }
}

#[test]
fn static_source_rejection_precedes_module_resource_lowering() {
    for resource in [
        "using resource = null;",
        "for (using resource = null; false;) {}",
        "for (using resource of [null]) {}",
    ] {
        let entry = format!("import source source from './source.js'; {resource}");
        let program = lower_module_graph(&sources(
            &[
                ("entry.js", &entry),
                ("source.js", "export const value = 1;"),
            ],
            ParseGoal::Module,
        ));
        assert!(program.script.is_none());
        assert!(
            program
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code()
                    == Some(scriptw_ir::EarlyErrorCode::ModuleSourceUnavailable)),
            "{:?}",
            program.diagnostics
        );
    }
}

#[test]
fn async_module_resources_have_a_canonical_execution_owner() {
    for source in [
        "await 0; using resource = null;",
        "using resource = null; await 0;",
        "await 0; for (using resource = null; false;) {}",
        "await 0; for (using resource of [null]) {}",
    ] {
        let program = module_program(&[("entry.js", source)]);
        let script = program.script.as_ref().unwrap();
        let activation = &activation_graph(script).unwrap().activations()[0];
        assert_eq!(activation.kind(), scriptw_ir::ModuleActivationKindIr::Async);
        let owner = script
            .functions
            .iter()
            .find(|function| &function.id == activation.function())
            .unwrap();
        assert_eq!(owner.protocol, FunctionProtocolIr::AsyncModuleActivation);
        assert!(owner.generator_plan.is_none());
    }
}

#[test]
fn original_request_phases_survive_linking_without_a_static_evaluation_schedule() {
    let program = module_program(&[
        (
            "entry.js",
            "import defer * as deferred from './dependency.js'; import { value } from './dependency.js'; value; deferred;",
        ),
        ("dependency.js", "export const value = await 0;"),
    ]);
    let graph = activation_graph(program.script.as_ref().unwrap()).unwrap();
    let entry = graph
        .activations()
        .iter()
        .find(|activation| activation.module() == 0)
        .unwrap();
    assert_eq!(
        entry.kind(),
        scriptw_ir::ModuleActivationKindIr::Synchronous
    );
    assert_eq!(
        entry
            .requests()
            .iter()
            .map(|request| (request.phase(), request.target()))
            .collect::<Vec<_>>(),
        [
            (scriptw_ir::ModuleRequestPhaseIr::Defer, 1),
            (scriptw_ir::ModuleRequestPhaseIr::Evaluation, 1),
        ]
    );
    assert_eq!(
        program
            .script
            .as_ref()
            .unwrap()
            .module_entry_evaluation()
            .unwrap()
            .kind(),
        scriptw_ir::ModuleEntryEvaluationKindIr::Promise
    );
}

#[test]
fn ordinary_module_var_locals_and_simple_parameters_do_not_allocate_cells() {
    let program = module_program(&[(
        "entry.js",
        "export function sum(input) { var total = 0; for (var index = 0; index < input; index++) total += index * 2 + 0.5; return total; }",
    )]);
    let script = program.script.as_ref().unwrap();
    let sum = script
        .functions
        .iter()
        .find(|function| function.name == "sum")
        .unwrap();
    for name in ["input", "total", "index"] {
        assert!(
            !sum.owned_env_bindings
                .iter()
                .any(|binding| binding.name == name),
            "uncaptured {name} needs private activation storage: {:?}",
            sum.owned_env_bindings
        );
    }
    assert!(sum.body.statements.iter().any(|statement| matches!(statement,
        StatementIr::Return(value) if matches!(&value.expr, ExprIr::Identifier(name) if name == "total")
    )), "the local result must bypass named environment resolution");
}

#[test]
fn ordinary_module_captured_vars_keep_their_original_owner_and_cell() {
    let program = module_program(&[(
        "entry.js",
        "export function make(seed) { var state = seed; return function update(delta) { state += delta; return state; }; }",
    )]);
    let script = program.script.as_ref().unwrap();
    let make = script
        .functions
        .iter()
        .find(|function| function.name == "make")
        .unwrap();
    let update = script
        .functions
        .iter()
        .find(|function| function.name == "update")
        .unwrap();
    let state = make
        .owned_env_bindings
        .iter()
        .find(|binding| binding.name == "state")
        .expect("a live closure keeps the source var cell");
    let captured = update
        .captured_bindings
        .iter()
        .find(|binding| binding.source_name == "state")
        .expect("the nested callable retains its original state binding");
    assert_eq!(captured.slot, state.slot);
    assert!(!make
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == "seed"));
    assert!(!update
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == "delta"));
}

#[test]
fn module_exports_keep_canonical_cells_when_function_vars_are_private() {
    let program = module_program(&[(
        "entry.js",
        "export let counter = 1; export function advance(input) { var total = input * 2; counter += total; return counter; }",
    )]);
    let script = program.script.as_ref().unwrap();
    let graph = activation_graph(script).unwrap();
    let owner = script
        .functions
        .iter()
        .find(|function| &function.id == graph.activations()[0].function())
        .unwrap();
    assert!(
        owner
            .owned_env_bindings
            .iter()
            .any(|binding| binding.name == "counter"),
        "the exported binding still owns the module's canonical cell"
    );
    let advance = script
        .functions
        .iter()
        .find(|function| function.name == "advance")
        .unwrap();
    assert!(!advance
        .owned_env_bindings
        .iter()
        .any(|binding| matches!(binding.name.as_str(), "input" | "total")));
}

#[test]
fn real_direct_eval_conservatively_retains_module_function_var_cells() {
    let program = module_program(&[(
        "entry.js",
        "export function sum(input) { var total = input; return total; } export function evaluate() { var value = 7; return eval('value'); }",
    )]);
    let script = program.script.as_ref().unwrap();
    for (function, names) in [
        ("sum", &["input", "total"][..]),
        ("evaluate", &["value"][..]),
    ] {
        let function = script
            .functions
            .iter()
            .find(|candidate| candidate.name == function)
            .unwrap();
        let Some(scriptw_ir::EvalEnvironmentRoleIr::Declarative { bindings, .. }) =
            &function.eval_environment
        else {
            panic!("real eval retains named environment visibility");
        };
        for name in names {
            let binding = bindings
                .iter()
                .find(|binding| binding.source_name == *name)
                .expect("eval-visible source name keeps its binding");
            assert!(function
                .owned_env_bindings
                .iter()
                .any(|owned| owned.slot == binding.slot));
        }
    }
}
