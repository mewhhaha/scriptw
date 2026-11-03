use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{
    lower, EnvironmentIdentifierIr, EnvironmentIdentifierOperationIr,
    EnvironmentIdentifierResolutionStart, ExprIr, FunctionFlavor, FunctionIr,
    FunctionTargetKnowledge, KindSet, ProgramIr, StatementIr, TypedExpr, ValueKind,
};

fn lower_module(source: &str) -> ProgramIr {
    let parsed = parse(source, ParseOptions::module()).expect("module should parse");
    lower(&parsed)
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
    let EnvironmentIdentifierResolutionStart::CapturedCell(cell) = reference.resolution_start()
    else {
        panic!("ordinary function capture should have a fixed cell: {reference:?}");
    };
    let capture = function
        .captured_bindings
        .iter()
        .find(|binding| binding.source_name == "cell")
        .unwrap();
    assert_eq!((cell.slot(), cell.hops()), (capture.slot, capture.hops));
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
        let EnvironmentIdentifierResolutionStart::CapturedCell(cell) = reference.resolution_start()
        else {
            panic!("{name} should retain a direct held cell: {reference:?}");
        };
        assert!(function
            .captured_bindings
            .iter()
            .any(|binding| binding.source_name == reference.name
                && binding.slot == cell.slot()
                && binding.hops == cell.hops()));
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
fn module_captured_cells_exclude_module_root_globals_eval_and_resumable_owners() {
    for (source, name) in [
        (
            "let cell = 1; export function read() { return cell; }",
            "read",
        ),
        ("export function read() { return external; }", "read"),
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
    let EnvironmentIdentifierResolutionStart::CapturedCell(cell) = reference.resolution_start()
    else {
        panic!("block capture should retain its analyzed cell: {reference:?}");
    };
    let capture = function
        .captured_bindings
        .iter()
        .find(|binding| binding.source_name == "cell")
        .unwrap();
    assert_eq!((cell.slot(), cell.hops()), (capture.slot, capture.hops));
    assert_ne!(
        capture.name, "cell",
        "block identity has its own storage name"
    );
}
