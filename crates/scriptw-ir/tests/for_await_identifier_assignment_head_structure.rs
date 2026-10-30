const FOR_OF_SOURCE: &str = include_str!("../src/lowering/for_of.rs");
const FOR_IN_SOURCE: &str = include_str!("../src/lowering/for_in.rs");
const FOR_IN_HEAD_SOURCE: &str = include_str!("../src/lowering/for_in/head.rs");
const ASSIGNMENT_SOURCE: &str = include_str!("../src/lowering/assignment.rs");
const LOWERING_SOURCE: &str = include_str!("../src/lowering.rs");
const ANALYSIS_SOURCE: &str = include_str!("../src/analysis.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end after {start}: {end}"))
        .0
}

fn code_without_whitespace(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .flat_map(str::chars)
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn bare_identifier_head_has_a_closed_assignment_target_domain() {
    let domain = bounded(
        FOR_OF_SOURCE,
        "use super::*;",
        "struct LexicalForOfPatternBinding {",
    );
    assert!(!domain.contains("#[derive("));
    assert_eq!(
        FOR_OF_SOURCE
            .matches("enum ForOfBareIdentifierHead")
            .count(),
        1
    );
    let variants = bounded(
        FOR_OF_SOURCE,
        "enum ForOfBareIdentifierHead {",
        "struct LexicalForOfPatternBinding {",
    );
    assert_eq!(
        code_without_whitespace(variants),
        "Absent,AssignmentTarget{source_name:String},BorrowedVar{source_name:String},}"
    );
    assert!(!variants.contains("bool"));
    assert!(!variants.contains("Option"));
    assert!(!variants.contains("_ =>"));
}

#[test]
fn bare_identifier_and_borrowed_var_heads_use_private_temporaries() {
    let identifier_arm = bounded(
        FOR_OF_SOURCE,
        "IterableLoopInitializer::Identifier(identifier) => {",
        "IterableLoopInitializer::Var(variable) =>",
    );
    assert_eq!(
        identifier_arm
            .matches("ForOfBareIdentifierHead::AssignmentTarget")
            .count(),
        1
    );
    assert_eq!(
        identifier_arm
            .matches("self.alloc_temp_binding_name(\"forof.assignment\")")
            .count(),
        1
    );
    assert!(identifier_arm.contains("BindingMode::Let"));
    assert!(!identifier_arm.contains("BindingMode::Var"));

    let var_arm = bounded(
        FOR_OF_SOURCE,
        "IterableLoopInitializer::Var(variable) => match variable.binding() {",
        "IterableLoopInitializer::Let(Binding::Identifier(identifier)) =>",
    );
    assert!(var_arm.contains("BindingMode::Var"));
    assert!(var_arm.contains("self.borrows_direct_eval_variable_environment()"));
    assert!(var_arm.contains("ForOfBareIdentifierHead::BorrowedVar"));
    assert!(var_arm.contains("self.alloc_temp_binding_name(\"forof.var\")"));
    assert!(!var_arm.contains("ForOfBareIdentifierHead::AssignmentTarget"));
    assert!(!var_arm.contains("forof.assignment"));
}

#[test]
fn bare_identifier_prefix_uses_the_checked_reference_write_path() {
    let prefix = bounded(
        FOR_OF_SOURCE,
        "} else if let ForOfBareIdentifierHead::AssignmentTarget",
        "} else if let Some(access) = access_initializer.as_ref() {",
    );
    for required in [
        "ExprIr::Identifier(storage_name.clone())",
        "self.lower_bare_iteration_head_write(source_name.clone(), value)",
    ] {
        assert!(prefix.contains(required), "missing `{required}`");
    }
    // for-in's bare head shares the same write, so the two statements cannot
    // drift apart on eval-visible, `with`, or strict unresolvable heads.
    assert!(FOR_IN_SOURCE.contains("self.lower_for_in_iteration_initialization("));
    let for_in_prefix = bounded(
        FOR_IN_HEAD_SOURCE,
        "ForInDestination::Identifier(name) => {",
        "ForInDestination::Property(access)",
    );
    assert!(for_in_prefix
        .contains("self.lower_bare_iteration_head_write_with_evidence(name.clone(), key)"));
    assert!(for_in_prefix.contains("StatementIr::DeclarationEvaluation(write.value)"));
    assert!(!for_in_prefix.contains("StatementIr::Expression("));
    assert!(!for_in_prefix.contains("self.declare_binding("));
    let write = bounded(
        ASSIGNMENT_SOURCE,
        "pub(super) fn lower_bare_iteration_head_write_with_evidence(",
        "pub(super) fn lower_assign(",
    );
    let order = [
        "self.uses_runtime_identifier_environment_for(&source_name)",
        "EnvironmentIdentifierOperationIr::Assign",
        "self.locate_identifier_reference(&source_name)",
        ".select_preceding(reference.declarative_position())",
        "self.lower_with_scoped_identifier_write_with_evidence(",
    ];
    let mut cursor = 0;
    for required in order {
        let offset = write[cursor..]
            .find(required)
            .unwrap_or_else(|| panic!("missing or out of order `{required}`"));
        cursor += offset + required.len();
    }
    // The shared `with` write binds the RHS once; the located fallback only
    // ever sees that bound value.
    let shared = bounded(
        LOWERING_SOURCE,
        "fn lower_with_scoped_identifier_write_with_evidence(",
        "fn lower_pattern_assign(",
    );
    let order = [
        "self.with_environment_reference_plan(name.clone(), objects)",
        "PutValueBindings::allocate(",
        "plan.put_value(bindings, value, |bound| {",
        "self.lower_located_identifier_assign_value_with_evidence(",
        "name.clone(),",
        "bound,",
        "fallback,",
    ];
    let mut cursor = 0;
    for required in order {
        let offset = shared[cursor..]
            .find(required)
            .unwrap_or_else(|| panic!("missing or out of order `{required}`"));
        cursor += offset + required.len();
    }
    // The head assignment is completion-neutral: ForIn/OfBodyEvaluation's
    // loop value comes from the body, never from the per-iteration binding.
    assert_eq!(
        prefix
            .matches("StatementIr::DeclarationEvaluation(assignment)")
            .count(),
        1
    );
    assert!(!prefix.contains("StatementIr::Expression("));
    assert!(!prefix.contains("self.declare_binding("));
}

#[test]
fn capture_analysis_records_a_bare_head_even_when_the_body_never_reads_it() {
    let scan = bounded(
        ANALYSIS_SOURCE,
        "Statement::ForOfLoop(for_of) => {\n                let outer_cursor",
        "Statement::ForInLoop(for_in) => {\n                let outer_cursor",
    );
    let assignment_reference = bounded(
        scan,
        "if let IterableLoopInitializer::Identifier(identifier) = for_of.initializer() {",
        "let mut body_aliases = capture_aliases.clone();",
    );
    assert_eq!(assignment_reference.matches("self.record_ref(").count(), 1);
    assert!(assignment_reference.contains("interner.resolve_expect(identifier.sym())"));
    assert!(assignment_reference.contains("capture_aliases"));
    assert!(!assignment_reference.contains("supported_bound_names"));
    assert!(!assignment_reference.contains("declare"));
}
