const REFERENCE_SOURCE: &str = include_str!("../../../scriptw-ir/src/reference.rs");
const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/ir.rs");
const LOWERING_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering.rs");
const ORDINARY_PROPERTY_REFERENCE_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/ordinary_property_compound.rs");
const ORDINARY_PROPERTY_UPDATE_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/ordinary_property_update.rs");
const EARLY_ERRORS_SOURCE: &str = include_str!("../../../scriptw-ir/src/early_errors.rs");
const EXPRESSIONS_SOURCE: &str = include_str!("../../src/expressions.rs");
const PLANNING_SOURCE: &str = include_str!("../../src/planning.rs");
const DATA_SOURCE: &str = include_str!("../../src/data.rs");
const FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_ordinary_property_numeric_update_reference.js"
);
const CLI_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/language_numerics.rs");
const RUNNER_SOURCE: &str = include_str!("../../../scriptw-test262/src/lib.rs");
const KNOWN_FAILURES: &str = include_str!("../../../scriptw-cli/tests/known-failures.tsv");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/ordinary-property-numeric-update-reference.md"
);

const EXACT_TEST262: &[(&str, &str)] = &[
    (
        "postfix-decrement/S11.3.2_A6_T1",
        include_str!(
            "../../../../test262/vendor/test262/test/language/expressions/postfix-decrement/S11.3.2_A6_T1.js"
        ),
    ),
    (
        "postfix-increment/S11.3.1_A6_T1",
        include_str!(
            "../../../../test262/vendor/test262/test/language/expressions/postfix-increment/S11.3.1_A6_T1.js"
        ),
    ),
    (
        "prefix-decrement/S11.4.5_A6_T1",
        include_str!(
            "../../../../test262/vendor/test262/test/language/expressions/prefix-decrement/S11.4.5_A6_T1.js"
        ),
    ),
    (
        "prefix-increment/S11.4.4_A6_T1",
        include_str!(
            "../../../../test262/vendor/test262/test/language/expressions/prefix-increment/S11.4.4_A6_T1.js"
        ),
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
fn ir_owns_one_closed_ordinary_property_numeric_update_reference() {
    let carrier = bounded(
        REFERENCE_SOURCE,
        "pub struct OrdinaryPropertyNumericUpdateIr {",
        "/// A lowerer-owned ordinary property Reference",
    );
    for field in [
        "base_and_receiver: Box<TypedExpr>",
        "referenced_name: PropertyKeyIr",
        "strictness: Strictness",
        "op: NumericUpdateOp",
        "return_mode: UpdateReturnMode",
        "value_kind: NumericUpdateValueKind",
        "possible_getters: PropertyHookTargets",
        "possible_setters: PropertyHookTargets",
    ] {
        assert!(carrier.contains(field), "carrier lost {field}");
        assert!(!carrier.contains(&format!("pub {field}")));
    }
    assert!(carrier.contains("fn new("));
    assert!(!carrier.contains("pub fn new("));
    for accessor in [
        "base_and_receiver",
        "referenced_name",
        "strictness",
        "op",
        "return_mode",
        "value_kind",
        "possible_getters",
        "possible_setters",
    ] {
        assert!(carrier.contains(&format!("pub fn {accessor}(&self)")));
    }

    assert!(REFERENCE_SOURCE.contains(
        "#[derive(Debug)]\n#[must_use = \"an ordinary property Reference plan must be consumed by one mutation\"]\npub(crate) struct OrdinaryPropertyReferencePlan"
    ));
    let plan = bounded(
        REFERENCE_SOURCE,
        "pub(crate) struct OrdinaryPropertyReferencePlan {",
        "/// One fused mutation of a Super Property Reference.",
    );
    assert!(!plan.contains("impl Clone for OrdinaryPropertyReferencePlan"));
    assert!(!plan.contains("impl Copy for OrdinaryPropertyReferencePlan"));
    positions_in_order(
        plan,
        &[
            "pub(crate) fn numeric_update(\n        self,",
            "op: NumericUpdateOp",
            "return_mode: UpdateReturnMode",
            "possible_getters: PropertyHookTargets",
            "possible_setters: PropertyHookTargets",
            "let value_kind = ValueKind::Dynamic;",
            "KindSet::from_kind(ValueKind::Number)\n                .union(KindSet::from_kind(ValueKind::BigInt))",
            "ExprIr::OrdinaryPropertyNumericUpdate(OrdinaryPropertyNumericUpdateIr::new(",
        ],
    );
    let numeric_signature = bounded(plan, "pub(crate) fn numeric_update(", ") -> TypedExpr {");
    assert!(!numeric_signature.contains("value_kind"));
    assert!(IR_SOURCE.contains("OrdinaryPropertyNumericUpdate(OrdinaryPropertyNumericUpdateIr),"));
    assert!(EARLY_ERRORS_SOURCE.contains("ExprIr::OrdinaryPropertyNumericUpdate(update) =>"));
    assert!(IR_SOURCE.contains("ExprIr::OrdinaryPropertyNumericUpdate(update) =>"));
}

#[test]
fn lowering_exhaustively_intercepts_simple_updates_before_decomposed_property_access() {
    let reference = bounded(
        ORDINARY_PROPERTY_REFERENCE_LOWERING_SOURCE,
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

    let update = bounded(
        ORDINARY_PROPERTY_UPDATE_LOWERING_SOURCE,
        "pub(super) fn lower_ordinary_property_numeric_update(",
        "\n}\n\n#[cfg(test)]",
    );
    for mapping in [
        "UpdateOp::IncrementPost => (NumericUpdateOp::Increment, UpdateReturnMode::Postfix)",
        "UpdateOp::IncrementPre => (NumericUpdateOp::Increment, UpdateReturnMode::Prefix)",
        "UpdateOp::DecrementPost => (NumericUpdateOp::Decrement, UpdateReturnMode::Postfix)",
        "UpdateOp::DecrementPre => (NumericUpdateOp::Decrement, UpdateReturnMode::Prefix)",
    ] {
        assert!(update.contains(mapping), "update map lost {mapping}");
    }
    positions_in_order(
        update,
        &[
            "self.lower_ordinary_property_reference_plan(access)",
            "self.record_ordinary_property_get(&metadata);",
            "let possible_getters = Self::possible_ordinary_property_getters(&metadata);",
            "let coercion_may_call_user_code =",
            "self.ordinary_property_numeric_coercion_may_call_user_code(&metadata);",
            "self.possible_ordinary_property_setters(&metadata, coercion_may_call_user_code);",
            "plan.numeric_update(op, return_mode, possible_getters, possible_setters)",
            "self.record_ordinary_property_possible_write(",
        ],
    );
    let possible_write = bounded(
        ORDINARY_PROPERTY_REFERENCE_LOWERING_SOURCE,
        "pub(super) fn record_ordinary_property_possible_write(",
        "    /// Lower a source-level plain assignment",
    );
    positions_in_order(
        possible_write,
        &[
            "self.record_ordinary_property_mutation_authority_effects(",
            "self.possible_ordinary_property_setters(metadata, intervening_user_code);",
            "self.observe_unknown_property_hook(setter);",
            "self.observe_ordinary_property_hook_this(setter, receiver_info.clone());",
            "self.invalidate_unknown_user_code_effects();",
            "self.invalidate_ordinary_property_shape_aliases(",
        ],
    );
    assert!(ORDINARY_PROPERTY_UPDATE_LOWERING_SOURCE
        .contains("fn ordinary_property_numeric_update_owns_one_reference()"));

    let dispatch = bounded(
        LOWERING_SOURCE,
        "    fn lower_property_access_update(&mut self, op: UpdateOp, access: &PropertyAccess)",
        "    fn lower_update(&mut self, op: UpdateOp, target: &UpdateTarget)",
    );
    for marker in [
        "PropertyAccess::Simple(access) =>",
        "self.lower_ordinary_property_numeric_update(op, access)",
        "PropertyAccess::Super(access) => self.lower_super_property_numeric_update(op, access)",
        "PropertyAccess::Private(access) => self.lower_private_numeric_update(op, access)",
    ] {
        assert!(dispatch.contains(marker), "dispatch lost {marker}");
    }
    assert!(!dispatch.contains("_ =>"));
    assert!(!dispatch.contains("unsupported_expr("));
    assert!(!IR_SOURCE.contains("\n    PropertyUpdate {"));
    assert!(!LOWERING_SOURCE.contains("ExprIr::PropertyUpdate"));
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
fn aot_typestate_forces_get_tonumeric_delta_put_and_result_publication() {
    assert_gc_roles(&[
        "ReadOrdinaryPropertyNumericUpdateLocals",
        "ReadyToWriteOrdinaryPropertyNumericUpdateLocals",
    ]);
    let sealed = bounded(
        EXPRESSIONS_SOURCE,
        "trait OrdinaryPropertyReferenceSource {",
        "impl<'a> FunctionBuilder<'a> {",
    );
    assert_eq!(
        sealed
            .matches("impl OrdinaryPropertyReferenceSource for ")
            .count(),
        5
    );
    for name in [
        "Assignment",
        "LogicalAssignment",
        "GetCapture",
        "EagerCompoundAssignment",
        "NumericUpdate",
    ] {
        assert!(sealed.contains(&format!(
            "impl OrdinaryPropertyReferenceSource for OrdinaryProperty{name}Ir"
        )));
    }
    let get = gc_body(
        "fn emit_get_numeric_value_from_raw_ordinary_property_reference(",
        "fn emit_numeric_update_from_read_ordinary_property_reference(",
    );
    assert!(get.contains("raw:EvaluatedRawOrdinaryPropertyReferenceLocals,"));
    positions_in_order(
        &get,
        &[
            "self.emit_get_value_from_raw_ordinary_property_reference(",
            "self.emit_numeric_reference_old_value(value_kind,&old_value,function)?;",
            "Ok(ReadOrdinaryPropertyNumericUpdateLocals{",
        ],
    );
    let conversion = gc_body(
        "fn emit_numeric_reference_old_value(",
        "fn emit_get_numeric_value_from_raw_ordinary_property_reference(",
    );
    for marker in [
        "NumericUpdateValueKind::Dynamic=>",
        "NumericUpdateValueKind::Number=>",
        "NumericUpdateValueKind::BigInt=>{}",
        "self.emit_value_to_numeric_locals(old,&pending,function)?",
        "self.emit_value_to_number_payload(old,&pending,function)?",
    ] {
        assert!(conversion.contains(marker), "lost {marker}");
    }
    positions_in_order(
        &conversion,
        &[
            "self.completion().copy_from(&pending,function);",
            "self.emit_propagate_current_throw_if_needed(function);",
            "old.copy_from(pending.value(),function);",
            "pending.clear(function);",
        ],
    );
    let delta = gc_body(
        "fn emit_numeric_update_from_read_ordinary_property_reference(",
        "fn emit_put_value_from_ready_ordinary_property_numeric_update(",
    );
    assert!(delta.contains("read:ReadOrdinaryPropertyNumericUpdateLocals,"));
    positions_in_order(
        &delta,
        &[
            "letReadOrdinaryPropertyNumericUpdateLocals{",
            "self.emit_numeric_update_to_locals(",
            "update.op(),",
            "update.value_kind(),",
            "&old_value,",
            "&new_value,",
            "Ok(ReadyToWriteOrdinaryPropertyNumericUpdateLocals{",
        ],
    );
    let put = gc_body(
        "fn emit_put_value_from_ready_ordinary_property_numeric_update(",
        "fn compile_ordinary_property_numeric_update_to_value(",
    );
    assert!(put.contains("ready:ReadyToWriteOrdinaryPropertyNumericUpdateLocals,"));
    positions_in_order(
        &put,
        &[
            "letReadyToWriteOrdinaryPropertyNumericUpdateLocals{",
            "self.emit_ordinary_reference_set(",
            "update.strictness(),",
            "RuntimeErrorMessage::CANNOT_ASSIGN_TO_PROPERTY,",
            "output.copy_from(",
            "matchupdate.return_mode(){",
            "UpdateReturnMode::Prefix=>&new_value,",
            "UpdateReturnMode::Postfix=>&old_value,",
            "new_value.clear(function);",
            "reference.clear(function);",
            "old_value.clear(function);",
        ],
    );
    assert!(!put.contains("_=>"));
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
        "fn compile_ordinary_property_numeric_update_to_value(",
        "/// The sole expression publisher",
    );
    positions_in_order(
        &entry,
        &[
            "self.evaluate_raw_ordinary_property_reference(",
            "self.emit_get_numeric_value_from_raw_ordinary_property_reference(",
            "self.emit_numeric_update_from_read_ordinary_property_reference(",
            "self.emit_put_value_from_ready_ordinary_property_numeric_update(",
        ],
    );
}

#[test]
fn exhaustive_consumers_and_gc_owners_name_each_fused_phase() {
    let marker = "ExprIr::OrdinaryPropertyNumericUpdate(update) =>";
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
fn fixture_observes_all_modes_numeric_domains_and_reference_phases() {
    for marker in [
        "++values[updateKey(\"numberPrefixIncrement\")]",
        "values[updateKey(\"numberPostfixIncrement\")]++",
        "--values[updateKey(\"numberPrefixDecrement\")]",
        "values[updateKey(\"numberPostfixDecrement\")]--",
        "++values[updateKey(\"bigintPrefixIncrement\")]",
        "values[updateKey(\"bigintPostfixIncrement\")]++",
        "--values[updateKey(\"bigintPrefixDecrement\")]",
        "values[updateKey(\"bigintPostfixDecrement\")]--",
    ] {
        assert!(FIXTURE.contains(marker), "fixture lost mode {marker}");
    }
    for oracle in [
        "one key coercion per update",
        "abrupt base order",
        "abrupt raw key order",
        "nullishReference(null, \"null base\")",
        "nullishReference(undefined, \"undefined base\")",
        "ToPropertyKey abrupt order",
        "complete numeric update Reference lifecycle",
        "mutated raw key not recoerced",
        "ToNumeric abrupt nonpublication",
        "ToNumeric abrupt skips Set",
        "strict Set false nonpublication",
        "strict Set false no write",
        "sloppy Set false postfix result",
        "sloppy Set false ignored write",
    ] {
        assert!(FIXTURE.contains(oracle), "fixture lost oracle {oracle}");
    }
    assert!(FIXTURE.contains(
        "base,raw-key,to-key:p,proxy-get:p:true,getter:true,to-numeric,proxy-set:p:true:2,setter:true:2"
    ));
    assert!(FIXTURE.contains("\"use strict\";\n  return rejectingProxy[rejectingKey]++;"));
    assert!(CLI_SOURCE
        .contains("fn run_wasm_backend_preserves_ordinary_property_numeric_update_reference()"));
    assert!(CLI_SOURCE.contains("wasm_ordinary_property_numeric_update_reference.js"));
}

#[test]
fn exact_a6_inventory_is_raw_unmasked_and_runs_in_both_modes() {
    assert_eq!(EXACT_TEST262.len(), 4);
    for (path, source) in EXACT_TEST262 {
        assert!(
            !source.contains("flags:"),
            "{path} must execute in both modes"
        );
        let basename = path.rsplit('/').next().expect("exact basename");
        assert!(!RUNNER_SOURCE.contains(basename), "runner masks {path}");
        assert!(
            !KNOWN_FAILURES.contains(basename),
            "known failures mask {path}"
        );
        assert!(source.contains("base = null"));
        assert!(source.contains("throw new DummyError()"));
        assert!(source.contains("property key evaluated"));
    }
}

#[test]
fn dry_status_records_the_exact_current_baseline_and_nonclaims() {
    for marker in [
        "four physical files and eight executions",
        "NumericUpdateOp::{Increment, Decrement}",
        "UpdateReturnMode::{Prefix, Postfix}",
        "ToPropertyKey` exactly once",
        "apply `ToNumeric` exactly once",
        "This batch does not change eager or logical compound assignment",
        "resumable/suspended property",
    ] {
        assert!(CONTRACT.contains(marker), "contract lost {marker}");
    }
}
