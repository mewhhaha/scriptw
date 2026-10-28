const REFERENCE_SOURCE: &str = include_str!("../../../scriptw-ir/src/reference.rs");
const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/ir.rs");
const LOWERING_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering.rs");
const ASSIGNMENT_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/assignment.rs");
const THROW_INFERENCE_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/throw_inference/expression.rs");
const ORDINARY_PROPERTY_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/ordinary_property_compound.rs");
const EARLY_ERRORS_SOURCE: &str = include_str!("../../../scriptw-ir/src/early_errors.rs");
const EXPRESSIONS_SOURCE: &str = include_str!("../../src/expressions.rs");
const PLANNING_SOURCE: &str = include_str!("../../src/planning.rs");
const DATA_SOURCE: &str = include_str!("../../src/data.rs");
const FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_ordinary_property_eager_compound_assignment.js"
);
const CLI_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/language_numerics.rs");
const RUNNER_SOURCE: &str = include_str!("../../../scriptw-test262/src/lib.rs");
const KNOWN_FAILURES: &str = include_str!("../../../scriptw-cli/tests/known-failures.tsv");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/ordinary-property-eager-compound-assignment-reference.md"
);

macro_rules! exact_test262 {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!(
                "../../../../test262/vendor/test262/test/language/expressions/compound-assignment/",
                $name,
                ".js"
            )),
        )
    };
}

const EXACT_TEST262: &[(&str, &str)] = &[
    exact_test262!("S11.13.2_A7.1_T1"),
    exact_test262!("S11.13.2_A7.1_T2"),
    exact_test262!("S11.13.2_A7.1_T3"),
    exact_test262!("S11.13.2_A7.1_T4"),
    exact_test262!("S11.13.2_A7.2_T1"),
    exact_test262!("S11.13.2_A7.2_T2"),
    exact_test262!("S11.13.2_A7.2_T3"),
    exact_test262!("S11.13.2_A7.2_T4"),
    exact_test262!("S11.13.2_A7.3_T1"),
    exact_test262!("S11.13.2_A7.3_T2"),
    exact_test262!("S11.13.2_A7.3_T3"),
    exact_test262!("S11.13.2_A7.3_T4"),
    exact_test262!("S11.13.2_A7.4_T1"),
    exact_test262!("S11.13.2_A7.4_T2"),
    exact_test262!("S11.13.2_A7.4_T3"),
    exact_test262!("S11.13.2_A7.4_T4"),
    exact_test262!("S11.13.2_A7.5_T1"),
    exact_test262!("S11.13.2_A7.5_T2"),
    exact_test262!("S11.13.2_A7.5_T3"),
    exact_test262!("S11.13.2_A7.5_T4"),
    exact_test262!("S11.13.2_A7.6_T1"),
    exact_test262!("S11.13.2_A7.6_T2"),
    exact_test262!("S11.13.2_A7.6_T3"),
    exact_test262!("S11.13.2_A7.6_T4"),
    exact_test262!("S11.13.2_A7.7_T1"),
    exact_test262!("S11.13.2_A7.7_T2"),
    exact_test262!("S11.13.2_A7.7_T3"),
    exact_test262!("S11.13.2_A7.7_T4"),
    exact_test262!("S11.13.2_A7.8_T1"),
    exact_test262!("S11.13.2_A7.8_T2"),
    exact_test262!("S11.13.2_A7.8_T3"),
    exact_test262!("S11.13.2_A7.8_T4"),
    exact_test262!("S11.13.2_A7.9_T1"),
    exact_test262!("S11.13.2_A7.9_T2"),
    exact_test262!("S11.13.2_A7.9_T3"),
    exact_test262!("S11.13.2_A7.9_T4"),
    exact_test262!("S11.13.2_A7.10_T1"),
    exact_test262!("S11.13.2_A7.10_T2"),
    exact_test262!("S11.13.2_A7.10_T3"),
    exact_test262!("S11.13.2_A7.10_T4"),
    exact_test262!("S11.13.2_A7.11_T1"),
    exact_test262!("S11.13.2_A7.11_T2"),
    exact_test262!("S11.13.2_A7.11_T3"),
    exact_test262!("S11.13.2_A7.11_T4"),
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

fn positions_in_order(source: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let offset = source[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing marker after byte {cursor}: {marker}"));
        cursor += offset + marker.len();
    }
}

#[test]
fn ir_owns_one_closed_ordinary_property_eager_reference() {
    assert!(REFERENCE_SOURCE.contains(
        "#[derive(Debug)]\n#[must_use = \"an ordinary property Reference plan must be consumed by one mutation\"]\npub(crate) struct OrdinaryPropertyReferencePlan"
    ));
    let plan = bounded(
        REFERENCE_SOURCE,
        "pub(crate) struct OrdinaryPropertyReferencePlan {",
        "/// One fused mutation of a Super Property Reference.",
    );
    for field in [
        "base_and_receiver: Box<TypedExpr>",
        "referenced_name: PropertyKeyIr",
        "strictness: Strictness",
    ] {
        assert!(plan.contains(field));
        assert!(!plan.contains(&format!("pub {field}")));
    }
    assert!(!plan.contains("impl Clone for OrdinaryPropertyReferencePlan"));
    assert!(!plan.contains("impl Copy for OrdinaryPropertyReferencePlan"));
    positions_in_order(
        plan,
        &[
            "pub(crate) fn eager_compound_assignment(\n        self,",
            "old_value_binding: String",
            "op: EagerCompoundAssignmentOp",
            "rhs: TypedExpr",
            "possible_getters: PropertyHookTargets",
            "possible_setters: PropertyHookTargets",
            "ExprIr::Identifier(old_value_binding.clone())",
            "let result = op.apply(old_value, rhs);",
            "ExprIr::OrdinaryPropertyEagerCompoundAssignment(",
        ],
    );

    let carrier = bounded(
        REFERENCE_SOURCE,
        "pub struct OrdinaryPropertyEagerCompoundAssignmentIr {",
        "/// A lowerer-owned ordinary property Reference",
    );
    for field in [
        "base_and_receiver: Box<TypedExpr>",
        "referenced_name: PropertyKeyIr",
        "strictness: Strictness",
        "old_value_binding: String",
        "result: Box<TypedExpr>",
        "possible_getters: PropertyHookTargets",
        "possible_setters: PropertyHookTargets",
    ] {
        assert!(carrier.contains(field));
        assert!(!carrier.contains(&format!("pub {field}")));
    }
    assert!(carrier.contains("fn new("));
    assert!(!carrier.contains("pub fn new("));
    for accessor in [
        "base_and_receiver",
        "referenced_name",
        "strictness",
        "old_value_binding",
        "result",
        "possible_getters",
        "possible_setters",
    ] {
        assert!(carrier.contains(&format!("pub fn {accessor}(&self)")));
    }
    assert!(IR_SOURCE.contains(
        "OrdinaryPropertyEagerCompoundAssignment(OrdinaryPropertyEagerCompoundAssignmentIr),"
    ));
    assert!(EARLY_ERRORS_SOURCE
        .contains("ExprIr::OrdinaryPropertyEagerCompoundAssignment(assignment) =>"));
    assert!(THROW_INFERENCE_SOURCE
        .contains("ExprIr::OrdinaryPropertyEagerCompoundAssignment(assignment) =>"));
}

#[test]
fn lowering_intercepts_all_eager_access_operators_before_generic_reference_decomposition() {
    let reference = bounded(
        ORDINARY_PROPERTY_LOWERING_SOURCE,
        "pub(super) fn lower_ordinary_property_reference_plan(",
        "    /// Lower one ordinary property Reference directly",
    );
    positions_in_order(
        reference,
        &[
            "let base_and_receiver = Box::new(self.lower_property_target(access.target()));",
            "let referenced_name = match access.field()",
            "self.lower_expression(expression)",
            "let plan = OrdinaryPropertyReferencePlan::new(",
            "self.reference_strictness()",
            "(plan, referenced_name, metadata)",
        ],
    );
    let producer = bounded(
        ORDINARY_PROPERTY_LOWERING_SOURCE,
        "pub(super) fn lower_ordinary_property_eager_compound_assignment(",
        "\n}\n\n#[cfg(test)]",
    );
    positions_in_order(
        producer,
        &[
            "self.lower_ordinary_property_reference_plan(access)",
            "self.record_ordinary_property_get(&metadata);",
            "let possible_getters = Self::possible_ordinary_property_getters(&metadata);",
            "let rhs_effect_accounting = self.prepare_potentially_effectful_expression(rhs);",
            "let rhs = self.lower_expression(rhs);",
            "let coercion_may_call_user_code =",
            "self.ordinary_property_numeric_coercion_may_call_user_code(&metadata);",
            "let possible_setters =",
            "self.possible_ordinary_property_setters(&metadata, coercion_may_call_user_code);",
            "let old_value_binding =",
            "plan.eager_compound_assignment(",
            "possible_getters",
            "possible_setters",
        ],
    );
    assert!(ORDINARY_PROPERTY_LOWERING_SOURCE
        .contains("fn ordinary_property_eager_compound_assignment_owns_one_reference()"));

    let assign = ASSIGNMENT_LOWERING_SOURCE;
    for marker in [
        "EagerCompoundAssignmentOp::Arithmetic(arithmetic)",
        "EagerCompoundAssignmentOp::Bitwise(bitwise)",
    ] {
        assert!(
            assign.matches(marker).count() >= 2,
            "ordinary and super access arms must both select {marker}"
        );
    }
    assert_eq!(
        assign
            .matches(".lower_ordinary_property_eager_compound_assignment(")
            .count(),
        2,
        "arithmetic and bitwise access domains need the fused producer"
    );
    assert!(!IR_SOURCE.contains("\n    PropertyCompoundAssign {"));
    assert!(!LOWERING_SOURCE.contains("ExprIr::PropertyCompoundAssign"));
}

fn gc_body(start: &str, end: &str) -> String {
    bounded(EXPRESSIONS_SOURCE, start, end)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn assert_gc_roles(names: &[&str]) {
    let roles = bounded(
        EXPRESSIONS_SOURCE,
        "struct EvaluatedRawOrdinaryPropertyReferenceLocals {",
        "/// The sealed input required",
    );
    for name in names {
        let marker = format!("struct {name} {{");
        let before = EXPRESSIONS_SOURCE
            .split_once(&marker)
            .expect("phase declaration")
            .0;
        assert!(before.rsplit("\n\n").next().unwrap().contains("#[must_use"));
        let fields = bounded(roles, &marker, "\n}");
        assert!(!fields.contains("pub "));
        assert!(fields.contains("reference: CanonicalOrdinaryPropertyReferenceLocals,"));
        assert!(fields.contains("old_value: ValueLocals,"));
    }
    assert!(!roles.contains("derive("));
    assert!(!roles.contains("impl Clone"));
    assert!(!roles.contains("impl Copy"));
    assert!(!roles.contains("TaggedLocals"));
}

#[test]
fn aot_typestate_forces_raw_key_get_result_and_putvalue_transitions() {
    assert_gc_roles(&[
        "ReadOrdinaryPropertyReferenceLocals",
        "ReadyToWriteOrdinaryPropertyReferenceLocals",
    ]);
    let evaluate = gc_body(
        "fn evaluate_raw_ordinary_property_reference(",
        "fn canonicalize_ordinary_property_reference(",
    );
    positions_in_order(&evaluate, &[
        "self.compile_expr_to_value(reference.base_and_receiver(),&base_and_receiver,function)?;",
        "self.compile_raw_property_key_expression_to_value(",
        "Ok(EvaluatedRawOrdinaryPropertyReferenceLocals{",
    ]);
    assert!(!evaluate.contains("emit_value_to_property_key_locals("));
    let canonical = gc_body(
        "fn canonicalize_ordinary_property_reference(",
        "fn emit_get_value_from_raw_ordinary_property_reference(",
    );
    positions_in_order(
        &canonical,
        &[
            "letEvaluatedRawOrdinaryPropertyReferenceLocals{",
            "self.compile_nullish_tagged_i32(",
            "self.emit_expression_native_error(",
            "self.emit_value_to_object_locals(",
            "self.emit_propagate_current_throw_if_needed(function);",
            "self.emit_value_to_property_key_locals(",
            "referenced_name.clear(function);",
            "Ok(CanonicalOrdinaryPropertyReferenceLocals{",
        ],
    );
    assert_eq!(
        canonical
            .matches("emit_value_to_property_key_locals(")
            .count(),
        1
    );
    let get = gc_body(
        "fn emit_get_value_from_raw_ordinary_property_reference(",
        "fn evaluate_rhs_for_raw_ordinary_property_assignment(",
    );
    positions_in_order(
        &get,
        &[
            "self.canonicalize_ordinary_property_reference(",
            "ObjectReadArguments::new(",
            "&reference.target_object,",
            "&reference.base_and_receiver,",
            "&reference.property_key,",
            "self.emit_propagate_current_throw_if_needed(function);",
            "old_value.copy_from(",
            "Ok(ReadOrdinaryPropertyReferenceLocals{",
        ],
    );
    let result = gc_body(
        "fn emit_result_from_read_ordinary_property_reference(",
        "fn emit_put_value_from_ready_ordinary_property_reference(",
    );
    assert!(result.contains("read:ReadOrdinaryPropertyReferenceLocals,"));
    positions_in_order(
        &result,
        &[
            "letReadOrdinaryPropertyReferenceLocals{",
            "self.retain_expression_operand(",
            "self.compile_expr_to_value(mutation.result(),&result,function)",
            "self.pop_scope();",
            "self.release_local_binding(id,function);",
            "compiled?;",
            "Ok(ReadyToWriteOrdinaryPropertyReferenceLocals{",
        ],
    );
    let put = gc_body(
        "fn emit_put_value_from_ready_ordinary_property_reference(",
        "fn compile_ordinary_property_eager_compound_assignment_to_value(",
    );
    assert!(put.contains("ready:ReadyToWriteOrdinaryPropertyReferenceLocals,"));
    positions_in_order(
        &put,
        &[
            "letReadyToWriteOrdinaryPropertyReferenceLocals{",
            "self.emit_ordinary_reference_set(",
            "RuntimeErrorMessage::CANNOT_ASSIGN_TO_PROPERTY,",
            "output.copy_from(&result,function);",
            "result.clear(function);",
            "reference.clear(function);",
            "old_value.clear(function);",
        ],
    );
    assert!(!put.contains("emit_value_to_property_key_locals("));
    let set = gc_body(
        "fn emit_ordinary_reference_set(",
        "fn emit_put_value_from_ready_ordinary_property_assignment(",
    );
    positions_in_order(
        &set,
        &[
            "OrdinarySetArguments::new(",
            "self.emit_propagate_current_throw_if_needed(function);",
            "ifstrictness.throws_on_failed_set(){",
            "self.emit_expression_native_error(NativeErrorKind::TypeError,message,function)?;",
        ],
    );
    let entry = gc_body(
        "fn compile_ordinary_property_eager_compound_assignment_to_value(",
        "fn emit_numeric_reference_old_value(",
    );
    positions_in_order(
        &entry,
        &[
            "self.evaluate_raw_ordinary_property_reference(",
            "self.emit_get_value_from_raw_ordinary_property_reference(",
            "self.emit_result_from_read_ordinary_property_reference(",
            "self.emit_put_value_from_ready_ordinary_property_reference(",
        ],
    );
    // Every compile_expr_to_value call routes abrupt completion before its caller continues.
    let publisher = EXPRESSIONS_SOURCE
        .split_once("pub(crate) fn compile_expr_to_value(")
        .unwrap()
        .1;
    assert!(publisher.contains("self.emit_propagate_current_throw_if_needed(function);"));
}

#[test]
fn exhaustive_consumers_and_gc_owners_name_each_fused_phase() {
    let marker = "ExprIr::OrdinaryPropertyEagerCompoundAssignment(mutation) =>";
    assert_eq!(
        EXPRESSIONS_SOURCE.matches(marker).count(),
        1,
        "the sole whole-value publisher owns the fused node"
    );
    assert_eq!(DATA_SOURCE.matches(marker).count(), 1);
    assert_eq!(PLANNING_SOURCE.matches(marker).count(), 3);
    for owner in [
        "fn expr_exposes_global_object(",
        "fn collect_expr_global_property_names(",
        "fn expr_references_function(",
    ] {
        assert_eq!(
            bounded(PLANNING_SOURCE, owner, "\n}\n")
                .matches(marker)
                .count(),
            1,
            "{owner}"
        );
    }
    assert!(
        !PLANNING_SOURCE.contains("fn count_expr_temp_locals("),
        "GC reservation replaced the raw temp budget"
    );
    let clear = gc_body(
        "impl CanonicalOrdinaryPropertyReferenceLocals {",
        "#[must_use =",
    );
    positions_in_order(
        &clear,
        &[
            "fnclear(self,",
            "self.property_key.clear(function);",
            "self.target_object.clear(function);",
            "self.base_and_receiver.clear(function);",
        ],
    );
}

#[test]
fn fixture_observes_all_eager_operators_and_the_reference_lifecycle() {
    for marker in [
        "] *= 3", "] /= 3", "] %= 5", "] += 3", "] -= 3", "] <<= 2", "] >>= 2", "] >>>= 2",
        "] &= 11", "] ^= 10", "] |= 3", "] **= 3",
    ] {
        assert!(FIXTURE.contains(marker), "fixture lost operator {marker}");
    }
    for oracle in [
        "abrupt base order",
        "abrupt raw key order",
        "nullishReference(null, \"null base\")",
        "nullishReference(undefined, \"undefined base\")",
        "ToPropertyKey abrupt order",
        "complete Reference lifecycle",
        "canonical key write",
        "mutated raw key not recoerced",
        "strict Set false nonpublication",
        "strict Set false no write",
        "sloppy Set false ignored write",
        "RHS abrupt nonpublication",
        "RHS abrupt skips Set",
    ] {
        assert!(FIXTURE.contains(oracle), "fixture lost oracle {oracle}");
    }
    assert!(FIXTURE.contains(
        "base,raw-key,to-key,proxy-get:p:true,getter:true,rhs,proxy-set:p:true:3,setter:true:3"
    ));
    assert!(FIXTURE.contains("\"use strict\";\n  return rejectingProxy[rejectingKey] +="));
    assert!(CLI_SOURCE
        .contains("fn run_wasm_backend_preserves_ordinary_property_eager_compound_reference()"));
    assert!(CLI_SOURCE.contains("wasm_ordinary_property_eager_compound_assignment.js"));
}

#[test]
fn exact_a7_inventory_is_raw_unmasked_and_keeps_all_t3_controls() {
    assert_eq!(EXACT_TEST262.len(), 44);
    for group in 1..=11 {
        for witness in 1..=4 {
            let name = format!("S11.13.2_A7.{group}_T{witness}");
            assert_eq!(
                EXACT_TEST262
                    .iter()
                    .filter(|(candidate, _)| *candidate == name)
                    .count(),
                1,
                "missing or duplicate exact witness {name}"
            );
        }
    }
    assert_eq!(
        EXACT_TEST262
            .iter()
            .filter(|(name, _)| name.ends_with("_T3"))
            .count(),
        11
    );
    for (name, source) in EXACT_TEST262 {
        assert!(
            !source.contains("flags:"),
            "{name} must execute in both modes"
        );
        assert!(
            !RUNNER_SOURCE.contains(name),
            "runner masks exact witness {name}"
        );
        assert!(
            !KNOWN_FAILURES.contains(name),
            "known-failure inventory masks exact witness {name}"
        );
    }
}

#[test]
fn status_records_the_exact_baseline_and_keeps_the_batch_bounded() {
    for marker in [
        "44 physical files",
        "for 88 matrix\nexecutions",
        "Exponentiation has no file in this legacy matrix",
        "This batch does not change logical assignment",
        "prefix/postfix numeric update",
        "resumable/suspended property Reference",
    ] {
        assert!(CONTRACT.contains(marker), "contract lost boundary {marker}");
    }
}
