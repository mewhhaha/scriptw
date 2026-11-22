use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{
    lower, BindingMode, EnvironmentIdentifierOperationIr, ExprIr, FunctionIr,
    FunctionTargetKnowledge, KindSet, ProgramIr, StatementIr, TypedExpr, ValueKind,
};

fn program(source: &str, options: ParseOptions) -> ProgramIr {
    let parsed = parse(source, options).expect("source parses");
    let lowered = lower(&parsed);
    assert!(lowered.is_wasm_supported(), "{:?}", lowered.diagnostics);
    lowered
}

fn named<'a>(program: &'a ProgramIr, name: &str) -> &'a FunctionIr {
    program
        .script
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing function {name}"))
}

fn returned(function: &FunctionIr) -> &TypedExpr {
    fn find(statement: &StatementIr) -> Option<&TypedExpr> {
        match statement {
            StatementIr::Return(value) => Some(value),
            StatementIr::Block(block) => block.statements.iter().find_map(find),
            StatementIr::LexicalBlock(statements) => statements.iter().find_map(find),
            _ => None,
        }
    }
    function.body.statements.iter().find_map(find).unwrap()
}

fn module_call_candidates(program: &ProgramIr) -> &FunctionTargetKnowledge {
    let value = returned(named(program, "run"));
    // Candidate identity belongs to the held call Reference, not its result.
    assert_eq!(value.kind, ValueKind::Dynamic);
    assert_eq!(value.possible_kinds, KindSet::all_runtime_tags());
    assert!(value.heap_shape.is_none());
    assert_eq!(value.function_targets, FunctionTargetKnowledge::unknown());
    let ExprIr::EnvironmentIdentifier(reference) = &value.expr else {
        panic!("Module call retains its live Reference: {value:?}");
    };
    let EnvironmentIdentifierOperationIr::Call {
        callee_targets,
        direct_eval,
        ..
    } = &reference.operation
    else {
        panic!("expected call Reference: {reference:?}");
    };
    assert!(direct_eval.is_none());
    assert!(callee_targets.exact_targets().is_none());
    callee_targets
}

#[test]
fn module_declaration_capture_keeps_only_an_open_original_callee_candidate() {
    let program = program(
        "function frame(row) { return row.x; } export function run(row) { return frame(row); }",
        ParseOptions::module(),
    );
    let targets = module_call_candidates(&program);
    assert_eq!(targets.known_targets().len(), 1);
    assert!(targets
        .known_targets()
        .contains(&named(&program, "frame").id));
    let capture = named(&program, "run")
        .captured_bindings
        .iter()
        .find(|capture| capture.source_name == "frame")
        .unwrap();
    assert_ne!(capture.mode, BindingMode::Const);
}

#[test]
fn reassignment_does_not_turn_a_declaration_candidate_into_a_value_fact() {
    for replacement in [
        "function replacement(row) { return 'changed'; }",
        "new Proxy(frame, {apply(target, receiver, args) { return 'proxy'; }})",
        "17",
    ] {
        let program = program(
            &format!(
                "function frame(row) {{ return row.x; }}
                 export function run(row) {{ return frame(row); }}
                 export function replace() {{ frame = {replacement}; }}"
            ),
            ParseOptions::module(),
        );
        let targets = module_call_candidates(&program);
        assert_eq!(targets.known_targets().len(), 1, "{replacement}");
        assert!(targets
            .known_targets()
            .contains(&named(&program, "frame").id));
    }
}

#[test]
fn block_and_parameter_shadows_cannot_borrow_an_outer_declaration_candidate() {
    for source in [
        "export function make() { function frame(row) { return row.x; }
           { let frame = 17; return function run(row) { return frame(row); }; } }",
        "function frame(row) { return row.x; }
         export function make(frame) { return function run(row) { return frame(row); }; }",
        "export function make() { function frame(row) { return row.x; }
           { function frame(row) { return row.y; }
             return function run(row) { return frame(row); }; } }",
    ] {
        let program = program(source, ParseOptions::module());
        assert!(
            module_call_candidates(&program).known_targets().is_empty(),
            "{source}"
        );
    }
}

#[test]
fn immutable_script_capture_keeps_its_existing_exact_function_facts() {
    let program = program(
        "function make() { const fn = function stable(row) { return row.x; };
           return function run(row) { return fn(row); }; } make();",
        ParseOptions::script(),
    );
    let function = named(&program, "run");
    assert!(function
        .captured_bindings
        .iter()
        .any(|capture| { capture.source_name == "fn" && capture.mode == BindingMode::Const }));
    let ExprIr::CallIndirect { callee, .. } = &returned(function).expr else {
        panic!("ordinary Script capture retains its existing call IR");
    };
    assert!(matches!(callee.expr, ExprIr::Identifier(_)));
    assert_eq!(callee.kind, ValueKind::Function);
    assert_eq!(
        callee.possible_kinds,
        KindSet::from_kind(ValueKind::Function)
    );
    assert_eq!(
        callee.function_targets.exact_single_target(),
        Some(&named(&program, "stable").id)
    );
}

/// Only bindings that their owner's declaration instantiation initializes
/// before any closure can name them carry the capture TDZ proof.
#[test]
fn captured_binding_initialization_proves_only_instantiated_bindings() {
    use scriptw_ir::CapturedBindingInitializationIr as Initialization;
    let program = program(
        r#"
        let moduleLexical = 0;
        function moduleDeclared() { return 0; }
        export function make(simple) {
            var hoisted = 1;
            let lexical = 2;
            const constant = 3;
            class Klass {}
            function declared() { return 0; }
            return function read() {
                return [hoisted, lexical, constant, Klass, declared, simple,
                    moduleLexical, moduleDeclared];
            };
        }
        export function withDefault(first = 0, later) {
            return function readLater() { return later; };
        }
        "#,
        ParseOptions::module(),
    );
    let capture = |function: &str, source: &str| {
        named(&program, function)
            .captured_bindings
            .iter()
            .find(|capture| capture.source_name == source)
            .unwrap_or_else(|| panic!("{function} captures {source}"))
            .initialization
    };
    for source in ["hoisted", "declared", "simple"] {
        assert_eq!(
            capture("read", source),
            Initialization::BeforeAnyReference,
            "{source}"
        );
    }
    // Module function declarations stay checked: a cyclic import can reach
    // them before the module body runs.
    for source in [
        "lexical",
        "constant",
        "Klass",
        "moduleLexical",
        "moduleDeclared",
    ] {
        assert_eq!(capture("read", source), Initialization::Checked, "{source}");
    }
    assert_eq!(
        capture("readLater", "later"),
        Initialization::Checked,
        "a parameter of a list with expressions has its own TDZ"
    );
}
