const ENVIRONMENT_REFERENCE_SOURCE: &str =
    include_str!("../../src/environments/environment_reference.rs");
const REFERENCE_SOURCE: &str = include_str!("../../../scriptw-ir/src/reference.rs");
const ENVIRONMENT_EMITTER: &str = include_str!("../../src/expressions/environment_identifier.rs");
const LOWERING_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering/assignment.rs");
const LOGICAL_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/object_environment_logical.rs");
const FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_object_environment_logical_assignment.js"
);
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/object-environment-logical-assignment-reference.md"
);

macro_rules! witness {
    ($path:literal) => {
        (
            $path,
            include_str!(concat!("../../../../test262/vendor/test262/test/", $path)),
        )
    };
}

const SELECTED_WITNESSES: [(&str, &str); 3] = [
    witness!(
        "language/expressions/logical-assignment/lgcl-and-assignment-operator-unresolved-lhs.js"
    ),
    witness!(
        "language/expressions/logical-assignment/lgcl-or-assignment-operator-unresolved-lhs.js"
    ),
    witness!(
        "language/expressions/logical-assignment/lgcl-nullish-assignment-operator-unresolved-lhs.js"
    ),
];

const ADJACENT_RHS_WITNESSES: [(&str, &str); 6] = [
    witness!(
        "language/expressions/logical-assignment/lgcl-and-assignment-operator-unresolved-rhs.js"
    ),
    witness!(
        "language/expressions/logical-assignment/lgcl-and-assignment-operator-unresolved-rhs-put.js"
    ),
    witness!(
        "language/expressions/logical-assignment/lgcl-or-assignment-operator-unresolved-rhs.js"
    ),
    witness!(
        "language/expressions/logical-assignment/lgcl-or-assignment-operator-unresolved-rhs-put.js"
    ),
    witness!(
        "language/expressions/logical-assignment/lgcl-nullish-assignment-operator-unresolved-rhs.js"
    ),
    witness!(
        "language/expressions/logical-assignment/lgcl-nullish-assignment-operator-unresolved-rhs-put.js"
    ),
];

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end after {start}: {end}"))
        .0
}

fn assert_before(source: &str, earlier: &str, later: &str) {
    let earlier = source.find(earlier).expect("earlier operation");
    let later = source.find(later).expect("later operation");
    assert!(earlier < later, "`{earlier}` must precede `{later}`");
}

#[test]
fn shared_private_lifecycle_keeps_putvalue_inside_the_taken_branch() {
    let objects = bounded(
        REFERENCE_SOURCE,
        "impl ObjectEnvironmentBindingObject {",
        "/// Declarative-frame depth in the function currently being lowered.",
    );
    let logical = bounded(
        objects,
        "    fn logical_assignment(",
        "    /// GetValue, eager operation, same-base PutValue, then result.",
    );
    for marker in [
        "op: LogicalBinaryOp",
        "let lhs = self.clone().get_value(referenced_name, strictness);",
        "let write = self.put_value(referenced_name, strictness, rhs);",
        "ExprIr::LogicalShortCircuit {",
        "lhs: Box::new(lhs)",
        "rhs: Box::new(write)",
    ] {
        assert!(
            logical.contains(marker),
            "missing shared lifecycle: {marker}"
        );
    }
    assert_before(logical, "let lhs =", "let write =");
    assert_before(logical, "let write =", "ExprIr::LogicalShortCircuit");
    assert!(!logical.contains("ExprIr::GlobalPropertyWrite"));

    let get = bounded(
        objects,
        "    fn get_value(self, referenced_name: &str, strictness: Strictness) -> TypedExpr {",
        "    /// SetMutableBinding on the Object Environment Record selected before RHS.",
    );
    let put = bounded(
        objects,
        "    fn put_value(",
        "    /// GetValue, ToNumeric/delta, same-base PutValue, then prefix/postfix",
    );
    for operation in [
        "SpecOperationIr::WithEnvironmentGetBindingValueSloppy",
        "SpecOperationIr::WithEnvironmentGetBindingValueStrict",
    ] {
        assert!(get.contains(operation));
    }
    assert!(get.contains("self.read()"));
    let runtime_get = bounded(
        ENVIRONMENT_REFERENCE_SOURCE,
        "    fn emit_environment_identifier_get_body(",
        "    fn emit_environment_identifier_put_body(",
    );
    assert!(runtime_get.contains("RuntimeErrorMessage::OBJECT_ENVIRONMENT_BINDING_MISSING"));
    assert!(runtime_get.contains("Strictness::Sloppy => value.set_undefined(function)"));
    assert!(runtime_get.contains("EnvironmentReferenceKind::WithObject.code()"));
    assert_before(
        runtime_get,
        "ObjectHasPropertyArguments::new(",
        "ObjectReadArguments::new(",
    );
    assert!(put.contains("let recheck = self.has_property(referenced_name);"));
    assert!(put.contains("Strictness::Sloppy => TypedExpr::from_info("));
    assert!(put.contains("Strictness::Strict => TypedExpr::from_info("));
    assert!(put.contains("ExprIr::PropertyWrite"));
    assert!(put.contains("name: NativeErrorKind::ReferenceError"));
}

#[test]
fn with_selection_and_global_start_consume_one_selected_reference() {
    assert!(REFERENCE_SOURCE.contains(
        "#[derive(Debug)]\n#[must_use = \"a with-environment Reference must be consumed by GetValue, PutValue, DeleteBinding, logical assignment, numeric update, or compound assignment\"]\npub(crate) struct WithEnvironmentReferencePlan {"
    ));
    let with_impl = bounded(
        REFERENCE_SOURCE,
        "impl WithEnvironmentReferencePlan {",
        "/// `[[Strict]]` of a Reference Record (6.2.5).",
    );
    let with_logical = bounded(
        with_impl,
        "    pub(crate) fn logical_assignment(",
        "    /// Consume one ResolveBinding result for an eager compound assignment.",
    );
    for marker in [
        "self,",
        "op: LogicalBinaryOp",
        "rhs: TypedExpr",
        "fallback: TypedExpr",
        "for environment in outer",
        "environment.logical_assignment_or_else(",
        "rhs.clone()",
        "innermost.logical_assignment_or_else(",
    ] {
        assert!(with_logical.contains(marker), "missing with plan: {marker}");
    }

    let resolution = bounded(
        REFERENCE_SOURCE,
        "    fn logical_assignment_or_else(",
        "    /// Compose one selected Object Environment Record's GetBindingValue,",
    );
    assert!(resolution.contains("binding_object.binding_visible("));
    assert!(resolution.contains("binding_object.logical_assignment("));
    assert!(resolution.contains("condition: Box::new(binding_visible)"));
    assert!(resolution.contains("then_expr: Box::new(selected)"));
    assert!(resolution.contains("else_expr: Box::new(fallback)"));
    assert_before(resolution, "let binding_visible =", "let selected =");

    let global = bounded(
        LOGICAL_SOURCE,
        "    fn lower_global_environment_logical_assignment(",
        "    pub(super) fn lower_located_identifier_logical_assignment(",
    );
    assert!(global.contains("EnvironmentIdentifierIr::global("));
    assert!(global.contains("EnvironmentIdentifierOperationIr::LogicalCompound {"));
    assert!(!global.contains("ExprIr::GlobalPropertyRead"));
    assert!(!global.contains("ExprIr::GlobalPropertyWrite"));
    assert!(
        ENVIRONMENT_EMITTER.contains("EnvironmentIdentifierResolutionStart::GlobalEnvironment =>")
    );
    assert!(ENVIRONMENT_EMITTER
        .contains("self.emit_resolve_global_identifier(&key, identifier.strictness, function)?"));
    assert_before(
        ENVIRONMENT_EMITTER,
        "let reference = match identifier.resolution_start()",
        "self.emit_environment_identifier_get(&reference, read, &value, function)?;",
    );
    // The resolved Reference's logical assignment shares its GetValue tail
    // with a proven cell's; the Reference target still puts through PutValue.
    assert!(ENVIRONMENT_EMITTER.contains("IdentifierPut::Reference(&reference),"));
    assert!(ENVIRONMENT_EMITTER.contains(
        "IdentifierPut::Reference(reference) => {\n                self.emit_environment_identifier_put(reference, value, function)"
    ));
    let logical = bounded(
        ENVIRONMENT_EMITTER,
        "            Operation::LogicalCompound { operation, rhs } => {",
        "            _ => {",
    );
    assert_before(
        logical,
        "self.open_frame(ControlFrameKind::If",
        "self.compile_expr_to_value(rhs",
    );
    assert_before(
        logical,
        "self.compile_expr_to_value(rhs",
        "self.emit_identifier_put(&target",
    );
    assert!(!logical.contains("emit_resolve_"));
    assert_before(
        logical,
        "self.emit_identifier_put(&target",
        "self.pop_control(ControlFrameKind::If)",
    );
}

#[test]
fn pre_rhs_location_snapshot_and_closed_mapper_make_ordering_explicit() {
    let located = bounded(
        LOGICAL_SOURCE,
        "/// One identifier logical-assignment Reference located before RHS lowering.",
        "pub(super) enum LogicalAssignmentReachability {",
    );
    assert!(located.contains("pub(super) struct LocatedIdentifierLogicalAssignment {"));
    assert!(located.contains(
        "#[must_use = \"a pre-RHS logical-assignment Reference must be consumed after RHS lowering\"]\npub(super) struct LocatedIdentifierLogicalAssignment {"
    ));
    assert!(located.contains("reference: LocatedIdentifierReference"));
    assert!(located.contains("global_value_candidates: Option<ValueInfo>"));
    assert!(located.contains("pub(super) fn reject_definite_tdz(self)"));
    assert!(!located.contains("#[derive"));
    assert!(!located.contains("Clone"));
    assert!(!located.contains("Copy"));
    assert!(!located.contains("pub(super) reference:"));
    assert!(!located.contains("pub(super) global_value_candidates:"));

    let producer = bounded(
        LOGICAL_SOURCE,
        "    pub(super) fn locate_identifier_logical_assignment(",
        "    fn lower_global_environment_logical_assignment(",
    );
    assert!(producer.contains("let reference = self.locate_identifier_reference(name);"));
    assert!(
        producer.contains(".filter(|info| info.source != GlobalPropertySource::DefinitelyDeleted)")
    );
    assert!(producer.contains(".map(|info| info.value_info.clone())"));
    assert!(producer.contains("LocatedIdentifierLogicalAssignment {"));
    assert_eq!(
        LOGICAL_SOURCE
            .matches("fn locate_identifier_logical_assignment(")
            .count(),
        1
    );
    assert_eq!(
        LOWERING_SOURCE
            .matches("self.locate_identifier_logical_assignment(&name)")
            .count(),
        1,
    );

    let reachability = bounded(
        LOGICAL_SOURCE,
        "pub(super) enum LogicalAssignmentReachability {",
        "impl<'a> ScriptLowerer<'a> {",
    );
    assert!(reachability.contains("Definite"));
    assert!(reachability.contains("WithEnvironmentFallback"));

    let arm = bounded(
        LOWERING_SOURCE,
        "            AssignOp::BoolAnd | AssignOp::BoolOr | AssignOp::Coalesce => {",
        "            AssignOp::And\n            | AssignOp::Or",
    );
    for mapping in [
        "AssignOp::BoolAnd => LogicalBinaryOp::And",
        "AssignOp::BoolOr => LogicalBinaryOp::Or",
        "AssignOp::Coalesce => LogicalBinaryOp::Coalesce",
    ] {
        assert!(arm.contains(mapping), "missing closed mapping: {mapping}");
    }
    assert_before(
        arm,
        "let reference = self.locate_identifier_logical_assignment(&name);",
        "let rhs_value = self.lower_conditionally_reached_expression(rhs);",
    );
    assert!(arm.contains("reference.reject_definite_tdz()"));
    assert_before(
        arm,
        "self.invalidate_unknown_user_code_effects();",
        "let rhs_value = self.lower_conditionally_reached_expression(rhs);",
    );
    assert!(arm.contains("plan.logical_assignment(logical_op, rhs_value, fallback)"));
    assert!(arm.contains("LogicalAssignmentReachability::WithEnvironmentFallback"));
    assert!(arm.contains("LogicalAssignmentReachability::Definite"));

    let consumer = bounded(
        LOGICAL_SOURCE,
        "    pub(super) fn lower_located_identifier_logical_assignment(",
        "\n    }\n}",
    );
    assert!(consumer.contains("located: LocatedIdentifierLogicalAssignment"));
    assert!(consumer.contains("let LocatedIdentifierLogicalAssignment {"));
    assert!(consumer.contains("return self.lower_global_environment_logical_assignment("));
    assert!(consumer.contains("ExprIr::LogicalShortCircuit {"));
    assert!(consumer.contains("rhs: Box::new(write)"));
    assert_eq!(
        consumer
            .matches("LogicalAssignmentReachability::WithEnvironmentFallback =>")
            .count(),
        3,
    );
    assert!(consumer.contains("value.widen_for_possible_replacement();"));

    let global = bounded(
        LOGICAL_SOURCE,
        "    fn lower_global_environment_logical_assignment(",
        "    pub(super) fn lower_located_identifier_logical_assignment(",
    );
    assert!(global.contains("result_info.widen_for_possible_replacement();"));
    assert!(global.contains("self.invalidate_unknown_user_code_effects();"));
    assert!(global.contains("EnvironmentIdentifierIr::global("));
    assert!(global.contains("rhs: Box::new(rhs)"));
}

#[test]
fn durable_fixture_and_exact_current_pin_inventory_bound_the_claim() {
    for (path, source) in SELECTED_WITNESSES {
        assert!(source.contains("flags: [onlyStrict]"), "{path}");
        assert!(
            source.contains("features: [logical-assignment-operators]"),
            "{path}"
        );
        assert!(source.contains("assert.throws(ReferenceError"), "{path}");
        assert!(source.contains("unresolved"), "{path}");
        assert!(CONTRACT.contains(path), "contract omits {path}");
    }
    for (path, source) in ADJACENT_RHS_WITNESSES {
        assert!(
            source.contains("features: [logical-assignment-operators]"),
            "{path}"
        );
        assert!(source.contains("unresolved"), "{path}");
    }

    for marker in [
        "missingLogicalAnd &&= missingRhs()",
        "missingLogicalOr ||= missingRhs()",
        "missingLogicalNullish ??= missingRhs()",
        "shortLogicalAnd &&= shortRhs()",
        "shortLogicalOr ||= shortRhs()",
        "shortLogicalNullish ??= shortRhs()",
        "snapshotLogicalValue ||= (snapshotLogicalValue = \"rhs\")",
        "strictLogicalAnd &&= strictRhs(\"and\")",
        "strictLogicalOr ||= strictRhs(\"or\")",
        "strictLogicalNullish ??= strictRhs(\"nullish\")",
        "sloppyLogicalAnd &&= \"and\"",
        "sloppyLogicalOr ||= \"or\"",
        "sloppyLogicalNullish ??= \"nullish\"",
        "[Symbol.unscopables]: { nestedLogicalValue: true }",
        "lifecycleTrace === \"huhgdrhs\"",
        "strictResult === \"not written\"",
    ] {
        assert!(FIXTURE.contains(marker), "fixture omits {marker}");
    }
    assert!(CONTRACT.contains("No vendored Test262 logical-assignment file contains a `with`"));
    assert!(CONTRACT.contains("Property and private References are separate lowering domains"));
    assert!(CONTRACT
        .contains("assignment, eager arithmetic/bitwise compound assignment, numeric update,"));
}
