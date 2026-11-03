use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{
    lower, lower_module_graph, EnvironmentIdentifierIr, EnvironmentIdentifierOperationIr,
    EnvironmentIdentifierResolutionStart, ExprIr, FunctionFlavor, FunctionIr, FunctionProtocolIr,
    FunctionTargetKnowledge, KindSet, ModuleActivationKindIr, ModuleGraphSources, ModuleKey,
    ModuleSourceIr, ProgramIr, ProvenBindingCellOrigin, StatementIr, TypedExpr, ValueKind,
    LEXICAL_ARGUMENTS_NAME, LEXICAL_NEW_TARGET_NAME, LEXICAL_THIS_NAME,
};

fn lower_module(source: &str) -> ProgramIr {
    let parsed = parse(source, ParseOptions::module()).expect("module should parse");
    lower(&parsed)
}

fn lower_modules(files: &[(&str, &str)]) -> ProgramIr {
    let modules: Vec<_> = files
        .iter()
        .map(|(name, source)| {
            ModuleSourceIr::new(
                ModuleKey::from_host(*name),
                (*source).into(),
                format!("file:///{name}"),
            )
        })
        .collect();
    let mut resolutions = Vec::new();
    for (referrer, module) in modules.iter().enumerate() {
        for request in module.module_requests().expect("fixture requests parse") {
            let target = files
                .iter()
                .position(|(name, _)| *name == request.specifier().trim_start_matches("./"))
                .expect("fixture request has its source");
            resolutions.push((referrer as u32, request, target as u32));
        }
    }
    lower_module_graph(&ModuleGraphSources {
        realm_requests: Default::default(),
        modules,
        entry: 0,
        resolutions,
    })
}

fn function_return(function: &FunctionIr) -> Option<&TypedExpr> {
    fn returned(statement: &StatementIr) -> Option<&TypedExpr> {
        match statement {
            StatementIr::Return(value) => Some(value),
            // A non-simple parameter list gives the source body its own
            // lexical record. This is still the same function's return.
            StatementIr::Block(body) => body.statements.iter().find_map(returned),
            StatementIr::LexicalBlock(statements) => statements.iter().find_map(returned),
            _ => None,
        }
    }
    function.body.statements.iter().find_map(returned)
}

fn module_capture_reference<'a>(
    program: &'a ProgramIr,
    name: &str,
) -> (&'a FunctionIr, &'a TypedExpr, &'a EnvironmentIdentifierIr) {
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .find(|function| function.name == name)
        .expect("source function exists");
    let value = function_return(function)
        .unwrap_or_else(|| panic!("source return missing in {name}: {:?}", function.body));
    let ExprIr::EnvironmentIdentifier(reference) = &value.expr else {
        panic!("expected held identifier operation in {name}: {value:?}");
    };
    (function, value, reference)
}

fn assert_module_activation_capture(
    program: &ProgramIr,
    name: &str,
    expected_kind: ModuleActivationKindIr,
) {
    let (function, value, reference) = module_capture_reference(program, name);
    let EnvironmentIdentifierResolutionStart::Cell(cell) = reference.resolution_start() else {
        panic!("{name} should address its canonical Module cell: {reference:?}");
    };
    let capture = function
        .captured_bindings
        .iter()
        .find(|capture| capture.name == cell.storage_name())
        .expect("proof names an actual capture");
    assert_eq!(cell.owner(), function.id);
    assert_eq!(cell.source_name(), reference.name);
    assert_eq!(capture.source_name, reference.name);
    assert_eq!(cell.slot(), capture.slot);
    assert_eq!(
        cell.origin(),
        ProvenBindingCellOrigin::Captured {
            entry_hops: capture.hops
        }
    );
    let script = program.script.as_ref().unwrap();
    let activation = script
        .body
        .statements
        .iter()
        .find_map(|statement| {
            let StatementIr::Expression(value) = statement else {
                return None;
            };
            let ExprIr::ModuleExecutionGraph(graph) = &value.expr else {
                return None;
            };
            graph.activations().iter().find(|owner| owner.module() == 0)
        })
        .expect("entry Module owns a trusted graph activation");
    assert_eq!(activation.kind(), expected_kind);
    let owner = script
        .functions
        .iter()
        .find(|owner| &owner.id == activation.function())
        .unwrap();
    assert_eq!(
        owner.protocol,
        match expected_kind {
            ModuleActivationKindIr::Synchronous => FunctionProtocolIr::ModuleActivation,
            ModuleActivationKindIr::Async => FunctionProtocolIr::AsyncModuleActivation,
        }
    );
    assert!(owner
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == capture.name && binding.slot == capture.slot));
    assert_eq!(value.kind, ValueKind::Dynamic);
    assert_eq!(value.possible_kinds, KindSet::all_runtime_tags());
    assert_eq!(value.function_targets, FunctionTargetKnowledge::unknown());
    assert!(value.heap_shape.is_none());
}

#[test]
fn module_activation_cells_keep_live_reference_operations_and_open_callee_candidates() {
    let program = lower_module(
        r#"
        export let cell = 1;
        function target(value) { return value; }
        export function read() { return cell; }
        export function readType() { return typeof cell; }
        export function write(value) { return cell = value; }
        export function add(value) { return cell += value; }
        export function logical(value) { return cell ||= value; }
        export function increment() { return cell++; }
        export function call(value) { return target(value); }
        export function make() {
            function middle() {
                return function deep(a, b, c, d, e) { return cell; };
            }
            return middle();
        }
    "#,
    );
    for name in [
        "read",
        "readType",
        "write",
        "add",
        "logical",
        "increment",
        "call",
        "deep",
    ] {
        assert_module_activation_capture(&program, name, ModuleActivationKindIr::Synchronous);
    }
    let (_, _, call) = module_capture_reference(&program, "call");
    let EnvironmentIdentifierOperationIr::Call {
        direct_eval,
        callee_targets,
        ..
    } = &call.operation
    else {
        panic!("cell selection retains the actual Call operation");
    };
    assert!(direct_eval.is_none());
    assert!(callee_targets.exact_targets().is_none());
    let target = program
        .script
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .find(|function| function.name == "target")
        .unwrap();
    assert!(callee_targets.known_targets().contains(&target.id));
}

#[test]
fn async_module_activation_cells_keep_the_same_proof_before_and_after_await() {
    let program = lower_module(
        r#"
        export let cell = 1;
        export function before() { return cell; }
        await 0;
        cell = 2;
        export function after() { return cell; }
    "#,
    );
    for name in ["before", "after"] {
        assert_module_activation_capture(&program, name, ModuleActivationKindIr::Async);
    }
}

#[test]
fn module_import_aliases_address_the_importing_cell_without_exporter_value_facts() {
    let program = lower_modules(&[
        (
            "entry.js",
            r#"
            import { value as renamed, invoke as importedCall } from './source.js';
            export function read() { return renamed; }
            export function write(value) { return renamed = value; }
            export function add(value) { return renamed += value; }
            export function call(value) { return importedCall(value); }
        "#,
        ),
        (
            "source.js",
            "export let value = 1; export function invoke(value) { return value; }",
        ),
    ]);
    for name in ["read", "write", "add", "call"] {
        assert_module_activation_capture(&program, name, ModuleActivationKindIr::Synchronous);
    }
    let (_, _, read) = module_capture_reference(&program, "read");
    assert_eq!(
        read.name, "renamed",
        "the source reference keeps its local alias"
    );
    let (_, _, call) = module_capture_reference(&program, "call");
    let EnvironmentIdentifierOperationIr::Call { callee_targets, .. } = &call.operation else {
        panic!("imported call remains a held Reference operation");
    };
    assert!(callee_targets.exact_targets().is_none());
}

#[test]
fn module_captured_cells_use_finalized_capture_without_value_facts() {
    // React's CommonJS factories contain unrelated arguments references. Those
    // must retain their existing storage without disabling ordinary captures.
    let program = lower_module(
        r#"
        export function diagnostics() { return arguments[0]; }
        const factory = () => {
            let cell = 1;
            return function read() { return cell; };
        };
        export const reader = factory();
    "#,
    );
    let (function, value, reference) = module_capture_reference(&program, "read");
    let EnvironmentIdentifierResolutionStart::Cell(cell) = reference.resolution_start() else {
        panic!("ordinary function capture should have a fixed cell: {reference:?}");
    };
    let capture = function
        .captured_bindings
        .iter()
        .find(|binding| binding.source_name == "cell")
        .unwrap();
    assert_eq!(cell.slot(), capture.slot);
    assert_eq!(cell.storage_name(), capture.name);
    assert_eq!(cell.owner(), function.id);
    assert_eq!(
        cell.origin(),
        ProvenBindingCellOrigin::Captured {
            entry_hops: capture.hops
        }
    );
    assert_eq!(value.kind, ValueKind::Dynamic);
    assert_eq!(value.possible_kinds, KindSet::all_runtime_tags());
    assert_eq!(value.function_targets, FunctionTargetKnowledge::unknown());
    assert!(value.heap_shape.is_none());
    assert!(matches!(
        reference.operation,
        EnvironmentIdentifierOperationIr::Read
    ));
}

#[test]
fn module_captured_cells_keep_reference_operations_and_unknown_calls() {
    let program = lower_module(
        r#"
        export function make(cell, fn) {
            function readType() { return typeof cell; }
            function write(value) { return cell = value; }
            function add(value) { return cell += value; }
            function increment() { return cell++; }
            function call(value) { return fn(value); }
            return [readType, write, add, increment, call];
        }
    "#,
    );
    for name in ["readType", "write", "add", "increment", "call"] {
        let (function, _, reference) = module_capture_reference(&program, name);
        let EnvironmentIdentifierResolutionStart::Cell(cell) = reference.resolution_start() else {
            panic!("{name} should retain a direct held cell: {reference:?}");
        };
        assert!(function
            .captured_bindings
            .iter()
            .any(|binding| binding.source_name == reference.name
                && binding.slot == cell.slot()
                && cell.origin()
                    == ProvenBindingCellOrigin::Captured {
                        entry_hops: binding.hops
                    }));
        assert!(match (&reference.operation, name) {
            (EnvironmentIdentifierOperationIr::Typeof, "readType")
            | (EnvironmentIdentifierOperationIr::Assign { .. }, "write")
            | (EnvironmentIdentifierOperationIr::EagerCompound { .. }, "add")
            | (EnvironmentIdentifierOperationIr::Update { .. }, "increment") => true,
            (EnvironmentIdentifierOperationIr::Call { direct_eval, .. }, "call") =>
                direct_eval.is_none(),
            _ => false,
        });
    }
}

#[test]
fn module_captured_cells_exclude_eval_complex_parameters_and_user_resumable_owners() {
    for (source, name) in [
        (
            "export function make(cell) { eval(''); return function read() { return cell; }; }",
            "read",
        ),
        (
            "export function make(cell) { return function read(value = 0) { return cell; }; }",
            "read",
        ),
        (
            "export async function make(cell) { return function read() { return cell; }; }",
            "read",
        ),
        (
            "export function* make(cell) { yield function read() { return cell; }; }",
            "read",
        ),
        (
            "let cell = 1; export function read(value = 0) { return cell; }",
            "read",
        ),
        (
            "let cell = 1; export async function make() { return function read() { return cell; }; }",
            "read",
        ),
        (
            "let cell = 1; export function* make() { yield function read() { return cell; }; }",
            "read",
        ),
    ] {
        let program = lower_module(source);
        let (_, _, reference) = module_capture_reference(&program, name);
        assert_eq!(
            reference.resolution_start(),
            EnvironmentIdentifierResolutionStart::CurrentEnvironment,
            "{source}"
        );
    }
}

#[test]
fn module_block_and_iteration_cells_do_not_inherit_the_activation_lifetime_proof() {
    for source in [
        "export let saved; { let cell = 1; saved = function read() { return cell; }; }",
        "export let saved; for (let cell of [1, 2]) { saved = function read() { return cell; }; }",
    ] {
        let program = lower_module(source);
        let (_, _, reference) = module_capture_reference(&program, "read");
        assert_eq!(
            reference.resolution_start(),
            EnvironmentIdentifierResolutionStart::CurrentEnvironment,
            "{source}"
        );
    }
}

#[test]
fn module_unbound_references_start_at_live_global_record_for_every_operation() {
    let program = lower_module(
        r#"
        export function diagnostics() { return arguments[0]; }
        export function read() { return external; }
        export function readType() { return typeof external; }
        export function write(value) { return external = value; }
        export function add(value) { return external += value; }
        export function logical(value) { return external ||= value; }
        export function increment() { return external++; }
        export function call(value) { return external(value); }
        export function make() {
            let first = 1;
            function middle() {
                let second = first;
                return function deep() { { let third = second; return external; } };
            }
            return middle();
        }
    "#,
    );
    for name in [
        "read",
        "readType",
        "write",
        "add",
        "logical",
        "increment",
        "call",
        "deep",
    ] {
        let (_, value, reference) = module_capture_reference(&program, name);
        assert_eq!(reference.name, "external");
        assert_eq!(
            reference.resolution_start(),
            EnvironmentIdentifierResolutionStart::GlobalEnvironment,
            "{name}"
        );
        assert!(matches!(
            (&reference.operation, name),
            (EnvironmentIdentifierOperationIr::Read, "read" | "deep")
                | (EnvironmentIdentifierOperationIr::Typeof, "readType")
                | (EnvironmentIdentifierOperationIr::Assign { .. }, "write")
                | (
                    EnvironmentIdentifierOperationIr::EagerCompound { .. },
                    "add"
                )
                | (
                    EnvironmentIdentifierOperationIr::LogicalCompound { .. },
                    "logical"
                )
                | (EnvironmentIdentifierOperationIr::Update { .. }, "increment")
                | (
                    EnvironmentIdentifierOperationIr::Call {
                        direct_eval: None,
                        ..
                    },
                    "call"
                )
        ));
        if matches!(name, "read" | "deep") {
            assert_eq!(value.kind, ValueKind::Dynamic);
            assert_eq!(value.possible_kinds, KindSet::all_runtime_tags());
            assert_eq!(value.function_targets, FunctionTargetKnowledge::unknown());
            assert!(value.heap_shape.is_none());
        }
    }
}

#[test]
fn module_global_start_proof_excludes_eval_parameter_scopes_and_implicit_arguments() {
    for source in [
        "export function read() { eval(''); return external; }",
        "export function read() { return external; } export function unrelated() { eval(''); }",
        "export function read(value = 0) { return external; }",
        "export function read(...values) { return external; }",
        "export function read({value}) { return external; }",
        "export function read() { return arguments; }",
    ] {
        let program = lower_module(source);
        let (_, _, reference) = module_capture_reference(&program, "read");
        assert_eq!(
            reference.resolution_start(),
            EnvironmentIdentifierResolutionStart::CurrentEnvironment,
            "{source}"
        );
    }
}

#[test]
fn module_global_start_proof_respects_closure_lexical_and_function_bindings() {
    for source in [
        "export function make() { function read() { return later; } let later = 1; return read; }",
        "export const holder = function read() { return read; };",
        "export function make() { function target() {} return function read() { return target; }; }",
    ] {
        let program = lower_module(source);
        let (_, _, reference) = module_capture_reference(&program, "read");
        assert_ne!(
            reference.resolution_start(),
            EnvironmentIdentifierResolutionStart::GlobalEnvironment,
            "{source}"
        );
    }
}

#[test]
fn module_captured_cells_keep_implicit_arguments_on_its_live_reference() {
    let program = lower_module("export function make() { return () => arguments; }");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let arrow = program
        .script
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .find(|function| {
            function.protocol.flavor() == FunctionFlavor::Arrow
                && function.captures_lexical_arguments
        })
        .expect("source arrow capturing arguments, excluding synthetic module getters");
    let value = function_return(arrow).unwrap();
    let ExprIr::EnvironmentIdentifier(reference) = &value.expr else {
        panic!("implicit arguments retains its named reference: {value:?}");
    };
    assert_eq!(reference.name, "arguments");
    assert_eq!(
        reference.resolution_start(),
        EnvironmentIdentifierResolutionStart::CurrentEnvironment
    );
}

#[test]
fn module_captured_cells_match_block_storage_not_shadowed_spelling() {
    let program = lower_module(
        r#"
        export function make(cell) {
            { let cell = 3; return function read() { return cell; }; }
        }
    "#,
    );
    let (function, _, reference) = module_capture_reference(&program, "read");
    let EnvironmentIdentifierResolutionStart::Cell(cell) = reference.resolution_start() else {
        panic!("block capture should retain its analyzed cell: {reference:?}");
    };
    let capture = function
        .captured_bindings
        .iter()
        .find(|binding| binding.source_name == "cell")
        .unwrap();
    assert_eq!(cell.slot(), capture.slot);
    assert_eq!(cell.storage_name(), capture.name);
    assert_eq!(
        cell.origin(),
        ProvenBindingCellOrigin::Captured {
            entry_hops: capture.hops
        }
    );
    assert_ne!(
        capture.name, "cell",
        "block identity has its own storage name"
    );
}

#[test]
fn module_owned_cells_keep_existing_storage_and_unknown_value_facts() {
    let program = lower_module(
        r#"
        export function diagnostics() { return arguments[0]; }
        export function parameter(value) { function hold() { return value; } return value; }
        export function variable() { var value = 1; function hold() { return value; } return value; }
        export function lexical() { let value = 2; return value; }
    "#,
    );
    for name in ["parameter", "variable", "lexical"] {
        let (function, value, reference) = module_capture_reference(&program, name);
        let EnvironmentIdentifierResolutionStart::Cell(cell) = reference.resolution_start() else {
            panic!("{name} should retain a proven existing owned cell: {reference:?}");
        };
        assert_eq!(cell.origin(), ProvenBindingCellOrigin::Owned);
        assert_eq!(cell.owner(), function.id);
        assert!(
            function.owned_env_bindings.iter().any(|binding| {
                binding.name == cell.storage_name() && binding.slot == cell.slot()
            }),
            "owned proof must not allocate or substitute another cell: {function:?}"
        );
        assert_eq!(value.kind, ValueKind::Dynamic);
        assert_eq!(value.possible_kinds, KindSet::all_runtime_tags());
        assert_eq!(value.function_targets, FunctionTargetKnowledge::unknown());
        assert!(value.heap_shape.is_none());
    }
}

#[test]
fn module_owned_cells_preserve_held_operations_and_block_identity() {
    let program = lower_module(
        r#"
        export function diagnostics() { return arguments[0]; }
        export function readType(cell) { function hold() { return cell; } return typeof cell; }
        export function write(cell, value) { function hold() { return cell; } return cell = value; }
        export function add(cell, value) { function hold() { return cell; } return cell += value; }
        export function logical(cell, value) { function hold() { return cell; } return cell ||= value; }
        export function increment(cell) { function hold() { return cell; } return cell++; }
        export function call(fn, value) { function hold() { return fn; } return fn(value); }
        export function block(cell) {
            { let cell = 3; function nested() { return cell; } return cell; }
        }
    "#,
    );
    for name in ["readType", "write", "add", "logical", "increment", "call"] {
        let (function, _, reference) = module_capture_reference(&program, name);
        let EnvironmentIdentifierResolutionStart::Cell(cell) = reference.resolution_start() else {
            panic!("{name} must locate its existing own cell: {reference:?}");
        };
        assert_eq!(cell.origin(), ProvenBindingCellOrigin::Owned);
        assert_eq!(cell.owner(), function.id);
        assert!(matches!(
            (&reference.operation, name),
            (EnvironmentIdentifierOperationIr::Typeof, "readType")
                | (EnvironmentIdentifierOperationIr::Assign { .. }, "write")
                | (
                    EnvironmentIdentifierOperationIr::EagerCompound { .. },
                    "add"
                )
                | (
                    EnvironmentIdentifierOperationIr::LogicalCompound { .. },
                    "logical"
                )
                | (EnvironmentIdentifierOperationIr::Update { .. }, "increment")
                | (
                    EnvironmentIdentifierOperationIr::Call {
                        direct_eval: None,
                        ..
                    },
                    "call"
                )
        ));
    }
    let (_, _, owned) = module_capture_reference(&program, "block");
    let (_, _, captured) = module_capture_reference(&program, "nested");
    let EnvironmentIdentifierResolutionStart::Cell(owned) = owned.resolution_start() else {
        panic!("the block's resolved owned cell must retain its lexical identity");
    };
    let EnvironmentIdentifierResolutionStart::Cell(captured) = captured.resolution_start() else {
        panic!("the nested function must capture that same block cell");
    };
    assert_eq!(owned.origin(), ProvenBindingCellOrigin::Owned);
    assert!(matches!(
        captured.origin(),
        ProvenBindingCellOrigin::Captured { .. }
    ));
    assert_eq!(owned.storage_name(), captured.storage_name());
    assert_eq!(owned.slot(), captured.slot());
    assert_ne!(owned.storage_name(), "cell");
    assert_ne!(owned.owner(), captured.owner());
}

#[test]
fn module_owned_cells_exclude_eval_non_simple_parameters_and_arguments() {
    for source in [
        "export function read(cell) { eval(''); return cell; }",
        "export function read(cell = 0) { return cell; }",
        "export function read(...cell) { return cell; }",
        "export function read({cell}) { return cell; }",
        "export function read() { return arguments; }",
        "export function read($arguments) { return $arguments; }",
    ] {
        let program = lower_module(source);
        let (_, _, reference) = module_capture_reference(&program, "read");
        assert_eq!(
            reference.resolution_start(),
            EnvironmentIdentifierResolutionStart::CurrentEnvironment,
            "{source}"
        );
    }
}

#[test]
fn module_private_locals_do_not_inherit_arguments_storage_requirements() {
    let program = lower_module(
        r#"
        export function diagnostics() { return arguments[0]; }
        export function parameter(value) { return value; }
        export function variable() { var value = 1; return value; }
        export function snapshot(value) { value = 9; arguments[0]; return value; }
    "#,
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    for name in ["parameter", "variable", "snapshot"] {
        let functions = program
            .script
            .as_ref()
            .unwrap()
            .functions
            .iter()
            .filter(|function| function.name == name)
            .collect::<Vec<_>>();
        assert!(!functions.is_empty(), "source function {name} exists");
        for function in functions {
            let value = function_return(function).expect("private local return remains explicit");
            let ExprIr::Identifier(storage) = &value.expr else {
                panic!("{name} must use ordinary local lowering: {value:?}");
            };
            assert!(function
                .owned_env_bindings
                .iter()
                .all(|binding| binding.name != *storage));
            assert!(function
                .captured_bindings
                .iter()
                .all(|binding| binding.name != *storage));
            assert_eq!(
                function
                    .owned_env_bindings
                    .iter()
                    .any(|binding| binding.name == LEXICAL_ARGUMENTS_NAME),
                name == "snapshot",
                "retain the arguments cell exactly when this owner observes it: {function:?}"
            );
            if name != "parameter" {
                assert_eq!(value.kind, ValueKind::Number);
                assert_eq!(value.possible_kinds, KindSet::from_kind(ValueKind::Number));
            }
        }
    }
}

#[test]
fn module_synthetic_cells_are_omitted_for_unobserved_arguments_and_direct_entry_values() {
    let program = lower_module(
        "export function frame(row) { return row.x; } export function context() { return [this, new.target]; }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    for function in &program.script.as_ref().unwrap().functions {
        if !["frame", "context"].contains(&function.name.as_str()) {
            continue;
        }
        assert!(function.owned_env_bindings.is_empty(), "{function:?}");
        assert!(function.eval_environment.is_none(), "{function:?}");
    }
}

#[test]
fn module_synthetic_cell_capture_discovery_keeps_real_context_and_adjusts_hops() {
    let program = lower_module(
        r#"
        export function context(value) { return () => [this, new.target, arguments[0], value]; }
        export function make(value) {
            function middle() { return () => value; }
            return middle;
        }
    "#,
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let functions = &program.script.as_ref().unwrap().functions;
    let context = functions
        .iter()
        .find(|function| function.name == "context")
        .unwrap();
    for name in [
        LEXICAL_THIS_NAME,
        LEXICAL_NEW_TARGET_NAME,
        LEXICAL_ARGUMENTS_NAME,
    ] {
        assert!(
            context
                .owned_env_bindings
                .iter()
                .any(|binding| binding.name == name),
            "{name}: {context:?}"
        );
        assert!(
            functions.iter().any(
                |function| function.protocol.flavor() == FunctionFlavor::Arrow
                    && function
                        .captured_bindings
                        .iter()
                        .any(|binding| binding.source_name == name)
            ),
            "{name}"
        );
    }
    let middle = functions
        .iter()
        .find(|function| function.name == "middle")
        .unwrap();
    assert!(middle.owned_env_bindings.is_empty(), "{middle:?}");
    assert!(middle.eval_environment.is_none(), "{middle:?}");
    let capture = functions
        .iter()
        .filter(|function| {
            function.protocol.flavor() == FunctionFlavor::Arrow
                && !function.captures_lexical_arguments
        })
        .flat_map(|function| &function.captured_bindings)
        .find(|binding| binding.source_name == "value")
        .expect("the arrow below middle captures make's parameter");
    assert_eq!(
        capture.hops, 1,
        "only the arrow's own retained record precedes make"
    );
}

#[test]
fn module_synthetic_cell_pruning_retains_arguments_observers_and_dynamic_owners() {
    for source in [
        "export function read() { return arguments; }",
        "export function read() { return {arguments}; }",
        "export function read() { return function() { return arguments; }; }",
        "export function read() { return () => arguments; }",
        "export function read($arguments) { return $arguments; }",
        "export function read() { return eval('arguments'); }",
        "export function read(value) { return () => eval('arguments[0]'); }",
        "export function read(value) { const parent = () => arguments[0]; return function() { return eval('parent()'); }; }",
        "export function read(value) { return (() => ((eval))('arguments[0]'))(); }",
        "export function read(value = 1) { return value; }",
        "export async function read() { return 1; }",
        "export function* read() { yield 1; }",
    ] {
        let program = lower_module(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let function = program
            .script
            .as_ref()
            .unwrap()
            .functions
            .iter()
            .find(|function| function.name == "read")
            .unwrap();
        assert!(
            function
                .owned_env_bindings
                .iter()
                .any(|binding| binding.name == LEXICAL_ARGUMENTS_NAME),
            "{source}: {function:?}"
        );
    }
    let program = lower_module("export function read($this) { return $this; }");
    let function = program
        .script
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .find(|function| function.name == "read")
        .unwrap();
    assert!(function
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == LEXICAL_THIS_NAME));
}
