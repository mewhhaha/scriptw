const REFERENCE_SOURCE: &str = include_str!("../../../scriptw-ir/src/reference.rs");
const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/ir.rs");
const EARLY_ERRORS_SOURCE: &str = include_str!("../../../scriptw-ir/src/early_errors.rs");
const LOWERING_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering.rs");
const CONDITIONAL_FLOW_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/conditional_flow.rs");
const THROW_INFERENCE_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/throw_inference/expression.rs");
const BUILTIN_CALL_INFO_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/builtin_call_info.rs");
const DEFINE_PROPERTY_CALL_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/define_property_call.rs");
const ASSIGNMENT_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering/assignment.rs");
const REFERENCE_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/ordinary_property_compound.rs");
const LOGICAL_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/ordinary_property_logical.rs");
const PLANNING_SOURCE: &str = include_str!("../../src/planning.rs");
const PLANNING_TESTS: &str = include_str!("../../src/planning/tests.rs");
const FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_ordinary_property_logical_assignment_reference.js"
);
const CLI_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/language_numerics.rs");
const RUNNER_SOURCE: &str = include_str!("../../../scriptw-test262/src/lib.rs");
const KNOWN_FAILURES: &str = include_str!("../../../scriptw-cli/tests/known-failures.tsv");

macro_rules! witness {
    ($name:literal) => {
        (
            concat!("language/expressions/logical-assignment/", $name),
            include_str!(concat!(
                "../../../../test262/vendor/test262/test/language/expressions/logical-assignment/",
                $name
            )),
        )
    };
}

const EXACT_FALSE_SET_WITNESSES: [(&str, &str); 8] = [
    witness!("lgcl-and-assignment-operator-no-set-put.js"),
    witness!("lgcl-or-assignment-operator-no-set-put.js"),
    witness!("lgcl-nullish-assignment-operator-no-set-put.js"),
    witness!("lgcl-and-assignment-operator-non-writeable-put.js"),
    witness!("lgcl-or-assignment-operator-non-writeable-put.js"),
    witness!("lgcl-nullish-assignment-operator-non-writeable-put.js"),
    witness!("lgcl-or-assignment-operator-non-extensible.js"),
    witness!("lgcl-nullish-assignment-operator-non-extensible.js"),
];

const ORDER_CONTROLS: [(&str, &str); 3] = [
    witness!("lgcl-and-assignment-operator-lhs-before-rhs.js"),
    witness!("lgcl-or-assignment-operator-lhs-before-rhs.js"),
    witness!("lgcl-nullish-assignment-operator-lhs-before-rhs.js"),
];

const SHORT_CIRCUIT_CONTROLS: [(&str, &str); 3] = [
    witness!("lgcl-and-assignment-operator-no-set.js"),
    witness!("lgcl-or-assignment-operator-non-writeable.js"),
    witness!("lgcl-and-assignment-operator-non-extensible.js"),
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
fn ir_consumes_one_closed_ordinary_property_logical_reference() {
    let declaration = bounded(
        REFERENCE_SOURCE,
        "/// One logical assignment through an ordinary property Reference.",
        "impl OrdinaryPropertyLogicalAssignmentIr {",
    );
    assert!(declaration.contains("#[derive(Debug, Clone, PartialEq, Eq)]"));
    assert!(!declaration.contains("Copy"));

    let carrier = bounded(
        REFERENCE_SOURCE,
        "pub struct OrdinaryPropertyLogicalAssignmentIr {",
        "/// One fused numeric update of an ordinary property Reference.",
    );
    for field in [
        "base_and_receiver: Box<TypedExpr>",
        "referenced_name: PropertyKeyIr",
        "rhs: Box<TypedExpr>",
        "op: LogicalBinaryOp",
        "strictness: Strictness",
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
        "rhs",
        "op",
        "strictness",
        "possible_getters",
        "possible_setters",
    ] {
        assert!(
            carrier.contains(&format!("pub fn {accessor}(&self)")),
            "carrier lost {accessor} accessor"
        );
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
            "pub(crate) fn logical_assignment(",
            "self,",
            "op: LogicalBinaryOp",
            "rhs: TypedExpr",
            "possible_getters: PropertyHookTargets",
            "possible_setters: PropertyHookTargets",
            "dynamic_value_info()",
            "ExprIr::OrdinaryPropertyLogicalAssignment(",
            "self.base_and_receiver",
            "self.referenced_name",
            "Box::new(rhs)",
            "op",
            "self.strictness",
            "possible_getters",
            "possible_setters",
        ],
    );
    assert!(IR_SOURCE
        .contains("OrdinaryPropertyLogicalAssignment(OrdinaryPropertyLogicalAssignmentIr)"));
    assert!(REFERENCE_SOURCE.contains(
        "ExprIr::OrdinaryPropertyLogicalAssignment(assignment) => {\n            Some((assignment.strictness(), PutValueFailure::TypeErrorOnly))"
    ));
}

#[test]
fn lowering_intercepts_only_simple_property_logical_assignments() {
    let producer = bounded(
        REFERENCE_LOWERING_SOURCE,
        "pub(super) fn lower_ordinary_property_reference_plan(",
        "    pub(super) fn pre_write_global_property_value(",
    );
    positions_in_order(
        producer,
        &[
            "fn collect_receiver_accessors(",
            "possible_receiver_values: &mut Vec<ValueInfo>",
            "possible_receiver_values.push(receiver.value_info());",
            "ScriptLowerer::possible_shape_accessors(receiver.heap_shape.as_deref())",
            "let receiver_shapes_are_known = possible_receiver_values",
            "let mut possible_getters = PropertyHookTargets::from_known(known_getters);",
            "let mut possible_setters = PropertyHookTargets::from_known(known_setters);",
            "self.ordinary_property_mutation_authorities(&possible_receiver_values);",
            "let key_may_call_user_code = Self::property_key_may_call_user_code(&referenced_name);",
            "self.possible_unknown_accessor_functions()",
            "possible_getters.extend_targets(unknown_getters);",
            "possible_setters.extend_targets(unknown_setters);",
            "self.invalidate_unknown_user_code_effects();",
            "possible_getters.extend_known(self.dynamically_installed_getters.iter().cloned());",
            "possible_setters.extend_known(self.dynamically_installed_setters.iter().cloned());",
            "include_all_planned_source(self.analysis.planned_source_function_ids.clone())",
            "possible_receiver_values: possible_receiver_values.into_boxed_slice(),",
            "possible_mutation_authorities,",
            "unknown_property_hooks_possible:",
            "possible_getters,",
            "possible_setters,",
        ],
    );

    let authorities = bounded(
        REFERENCE_LOWERING_SOURCE,
        "    pub(super) fn ordinary_property_mutation_authorities(",
        "\n    }\n",
    );
    assert!(authorities.contains("possible_receivers: &[ValueInfo],"));
    assert!(authorities.contains("for receiver in possible_receivers {"));

    // Global-object and prototype-state invalidation is owned by the
    // authority projection that every possible write records first.
    let authority_effects = bounded(
        REFERENCE_LOWERING_SOURCE,
        "    pub(super) fn record_ordinary_property_mutation_authority_effects(",
        "    pub(super) fn record_ordinary_property_possible_write(",
    );
    for marker in [
        "self.invalidate_possible_global_property_value_info(name);",
        "self.invalidate_all_possible_global_property_value_infos();",
        "self.number_prototype_to_string_state = PrototypeToStringState::Unknown;",
        "self.boolean_prototype_to_string_state = PrototypeToStringState::Unknown;",
    ] {
        assert!(
            authority_effects.contains(marker),
            "mutation-authority invalidation lost {marker}"
        );
    }

    let possible_write = bounded(
        REFERENCE_LOWERING_SOURCE,
        "    pub(super) fn record_ordinary_property_possible_write(",
        "    fn possible_shape_accessors(",
    );
    for marker in [
        "self.record_ordinary_property_mutation_authority_effects(",
        "&metadata.possible_mutation_authorities,",
        "self.possible_ordinary_property_setters(metadata, intervening_user_code);",
        "let setter_may_call_user_code = metadata.unknown_property_hooks_possible",
        "self.invalidate_unknown_user_code_effects();",
        "for receiver in &metadata.possible_receiver_values",
        "self.invalidate_ordinary_property_shape_aliases(receiver);",
        "fn nested_aliases_edit(",
        "data_values(&shape.properties)",
        "value_alias_edit(value, alias, canonical_targets)",
        // Prototype links are judged by one owner, which reports whether the
        // remaining chain is still described.
        "fn prototype_link_edit(",
        "if alias.heap_shape.as_deref() != Some(linked) {",
        "nested_aliases_edit(linked, alias, canonical_targets)",
        "prototype_link_edit(prototype.as_deref(), alias, canonical_targets);",
        "HeapShape::Array(shape) => data_values(&shape.properties)",
        ".chain(&shape.elements)",
        // Only shapes on the path to a change are copied and refined.
        "fn refine_nested_aliases(",
        "data_values_mut(&mut shape.properties)",
        ".chain(&mut shape.elements)",
        "self.visit_live_heap_shape_roots(&OrdinaryPropertyAliases {",
    ] {
        assert!(
            possible_write.contains(marker),
            "possible-write invalidation lost {marker}"
        );
    }
    let accessors = bounded(
        REFERENCE_LOWERING_SOURCE,
        "    fn possible_shape_accessors(",
        "    /// Lower a source-level plain assignment",
    );
    assert!(accessors.contains("for property in properties.values()"));
    assert!(accessors.contains("collect(prototype, getters, setters);"));
    assert!(REFERENCE_LOWERING_SOURCE.contains("fn possible_unknown_accessor_functions("));
    assert!(REFERENCE_SOURCE.contains("pub struct PropertyHookTargets"));
    assert!(REFERENCE_SOURCE.contains("all_planned_source: Option<Arc<BTreeSet<FunctionId>>>"));
    assert!(REFERENCE_LOWERING_SOURCE.contains("self.analysis.planned_source_function_ids.clone()"));
    assert!(REFERENCE_LOWERING_SOURCE.contains("fn property_key_may_call_user_code("));
    assert!(LOWERING_SOURCE.contains("self.dynamically_installed_getters"));
    assert!(DEFINE_PROPERTY_CALL_SOURCE.contains("self.read_object_shape(descriptor, \"get\")"));
    assert!(LOWERING_SOURCE.contains("fn invalidate_unknown_user_code_effects(&mut self)"));
    assert!(LOWERING_SOURCE.contains(".extend(lowerer.dynamically_installed_getters)"));
    assert!(LOWERING_SOURCE.contains("unknown_user_code_effects_observed"));
    assert!(BUILTIN_CALL_INFO_SOURCE.contains("A Proxy is not an ordinary empty object"));
    assert!(BUILTIN_CALL_INFO_SOURCE.contains("StandardBuiltinId::ObjectSetPrototypeOf =>"));
    assert!(BUILTIN_CALL_INFO_SOURCE.contains("StandardBuiltinId::ObjectDefineProperties =>"));
    assert!(BUILTIN_CALL_INFO_SOURCE.contains("StandardBuiltinId::ReflectSetPrototypeOf =>"));
    assert!(
        LOWERING_SOURCE.contains("self.identifier_resolves_to_intrinsic_global(GLOBAL_THIS_NAME)")
    );
    assert!(LOWERING_SOURCE.contains("fn is_intrinsic_global_constructor(&self, name: &str)"));

    let helper = bounded(
        LOGICAL_LOWERING_SOURCE,
        "    pub(super) fn lower_ordinary_property_logical_assignment(",
        "\n    }\n}",
    );
    positions_in_order(
        helper,
        &[
            "self.lower_ordinary_property_reference_plan(access)",
            "self.record_ordinary_property_get(&metadata)",
            "let skipped_rhs = self.capture_conditional_flow_facts();",
            "let rhs_effect_accounting = self.prepare_potentially_effectful_expression(rhs);",
            "let rhs = self.lower_expression(rhs);",
            "let taken_rhs = self.capture_conditional_flow_facts();",
            "self.merge_conditional_flow_facts(skipped_rhs, taken_rhs);",
            "let rhs_info = rhs.value_info();",
            "self.pre_write_global_property_value(access, &referenced_name)",
            "let possible_getters = Self::possible_ordinary_property_getters(&metadata);",
            "self.possible_ordinary_property_setters(&metadata, rhs_may_have_intervening_effects)",
            "plan.logical_assignment(op, rhs, possible_getters, possible_setters)",
            "self.record_ordinary_property_possible_write(",
            "if !setter_may_call_user_code",
            "pre_write_global_value,",
            "rhs_info,",
        ],
    );
    assert!(!helper.contains("ExprIr::LogicalShortCircuit"));
    assert!(!helper.contains("ExprIr::PropertyRead"));
    assert!(!helper.contains("ExprIr::PropertyWrite"));

    for marker in [
        "struct ConditionalFlowFacts {",
        "current_this_binding: CurrentThisBinding",
        "current_construct_this_info: Option<ValueInfo>",
        "nested_script_global_value_infos: BTreeMap<String, ValueInfo>",
        "fn capture_conditional_flow_facts(&self) -> ConditionalFlowFacts",
        "fn merge_conditional_flow_facts(",
    ] {
        assert!(
            CONDITIONAL_FLOW_SOURCE.contains(marker),
            "flow join lost {marker}"
        );
    }
    assert!(LOWERING_SOURCE.contains("mod conditional_flow;"));
    assert!(LOWERING_SOURCE
        .contains("fn set_script_global_var_value_info(&mut self, name: &str, info: ValueInfo)"));

    let arm = bounded(
        ASSIGNMENT_SOURCE,
        "            AssignOp::BoolAnd | AssignOp::BoolOr | AssignOp::Coalesce => {",
        "            AssignOp::And\n            | AssignOp::Or",
    );
    positions_in_order(
        arm,
        &[
            "AssignOp::BoolAnd => LogicalBinaryOp::And",
            "AssignOp::BoolOr => LogicalBinaryOp::Or",
            "AssignOp::Coalesce => LogicalBinaryOp::Coalesce",
            "PropertyAccess::Simple(access) => self",
            ".lower_ordinary_property_logical_assignment(",
            "PropertyAccess::Super(_) | PropertyAccess::Private(_) => self",
            ".lower_property_reference_update(",
        ],
    );
    assert!(LOGICAL_LOWERING_SOURCE
        .contains("fn ordinary_property_logical_assignment_owns_one_reference_and_branch_rhs()"));
}

#[test]
fn exhaustive_consumers_and_budget_name_the_fused_lifecycle() {
    assert!(
        PLANNING_TESTS.contains("fn dynamic_property_keys_root_every_possible_shape_accessor()")
    );
    for marker in [
        "selection: ShapeAccessorReferenceSelection,",
        "any_accessor(shape, target, selection)",
        "assignment.possible_getters().contains(target)",
        "assignment.possible_setters().contains(target)",
    ] {
        assert!(PLANNING_SOURCE.contains(marker), "planning lost {marker}");
    }
    // Joined getter/setter provenance is now consumed directly by the shared
    // visitor. Pin its actual edges, not names of retired planning fixtures.
    let visitor = bounded(PLANNING_SOURCE, "fn expr_references_function(", "\n}\n");
    for (variant, binding, end, selection) in [
        (
            "OrdinaryPropertyAssignment",
            "assignment",
            "OrdinaryPropertyLogicalAssignment",
            "Setter",
        ),
        (
            "OrdinaryPropertyLogicalAssignment",
            "assignment",
            "OrdinaryPropertyGetCapture",
            "GetterOrSetter",
        ),
        (
            "OrdinaryPropertyNumericUpdate",
            "update",
            "OrdinaryPropertyEagerCompoundAssignment",
            "GetterOrSetter",
        ),
        (
            "OrdinaryPropertyEagerCompoundAssignment",
            "mutation",
            "PropertyWrite",
            "GetterOrSetter",
        ),
    ] {
        let arm = bounded(
            visitor,
            &format!("ExprIr::{variant}({binding}) => {{"),
            &format!("ExprIr::{end}"),
        );
        assert!(arm.contains(&format!("{binding}.possible_setters().contains(target)")));
        if selection == "GetterOrSetter" {
            assert!(arm.contains(&format!("{binding}.possible_getters().contains(target)")));
        }
        assert!(arm.contains(&format!(
            "{binding}.base_and_receiver().heap_shape.as_deref()"
        )));
        assert!(arm.contains(&format!("ShapeAccessorReferenceSelection::{selection}")));
    }
    assert!(
        EARLY_ERRORS_SOURCE.contains("ExprIr::OrdinaryPropertyLogicalAssignment(assignment) => {")
    );

    let throw_inference = bounded(
        THROW_INFERENCE_SOURCE,
        "            ExprIr::OrdinaryPropertyLogicalAssignment(assignment) => {",
        "            ExprIr::OrdinaryPropertyNumericUpdate(update) =>",
    );
    assert!(throw_inference.contains("Some(unknown_runtime_value_info())"));
}

#[test]
fn durable_fixture_covers_reference_order_false_set_and_primitive_receiver() {
    for marker in [
        "complete logical property Reference lifecycle",
        "sole ToPropertyKey",
        "same canonical key for Get and Set",
        "short-circuit skips RHS",
        "nullish before ToPropertyKey and RHS",
        "abrupt raw key precedes nullish validation",
        "RHS throw nonpublication",
        "Set throw nonpublication",
        "sloppy false Set publishes RHS",
        "strict false Set nonpublication",
        "strict no-set and TypeError",
        "strict no-set or TypeError",
        "strict no-set nullish TypeError",
        "strict non-writable and TypeError",
        "strict non-writable or TypeError",
        "strict non-writable nullish TypeError",
        "strict non-extensible or TypeError",
        "strict non-extensible nullish TypeError",
        "primitive and taken",
        "primitive or taken",
        "primitive nullish taken",
        "primitive and short-circuit",
        "primitive or short-circuit",
        "primitive nullish short-circuit",
        "primitive Receiver on every Get",
        "primitive Receiver on taken Sets",
        "logical global write invalidates stale type fact",
        "logical global merge retains pre-RHS fact",
        "logical merge retains taken RHS mutation before failed outer Set",
        "logical global write updates script var mirror",
        "skipped logical RHS preserves unrelated global fact",
        "logical write through globalThis alias invalidates canonical fact",
        "logical write through joined globalThis alias invalidates canonical fact",
        "dynamic logical global key invalidates every canonical fact",
        "shadowed globalThis write preserves canonical global fact",
        "skipped logical RHS preserves constructor receiver shape",
        "logical getter observes property base receiver",
        "dynamic logical getter observes property base receiver",
        "joined-shape logical getter observes property base receiver",
        "dynamic logical setter observes property base receiver",
        "joined-shape logical setter observes property base receiver",
        "logical primitive getter observes primitive receiver",
        "sloppy primitive getter receives boxed this",
        "sloppy primitive setter receives boxed this",
        "logical object-key coercion runs before Get",
        "logical key coercion invalidates global facts",
        "logical key coercion widens getter receiver shape",
        "logical getter invalidates global facts",
        "logical getter invalidates prototype guards",
        "logical setter invalidates global facts",
        "logical RHS widens setter receiver shape",
        "logical Proxy trap invalidates global facts",
        "joined Proxy provenance widens trap arguments",
        "nested descriptor getter observes known-shape receiver",
        "logical RHS-installed setter observes receiver",
        "logical RHS prototype setter observes receiver",
        "logical getter arbitrary throw catch type",
        "logical setter arbitrary throw catch type",
        "Object.setPrototypeOf getter invalidates global facts",
        "Object.defineProperties getter invalidates global facts",
        "logical write invalidates nested object alias shape",
        "logical write invalidates nested array alias shape",
        "plain setter arbitrary throw catch type",
        "eager getter arbitrary throw catch type",
        "numeric getter arbitrary throw catch type",
        "property Set widens a directly called setter parameter",
        "delete invalidates own shape before inherited getter",
        "direct global delete invalidates globalThis alias shape",
        "destructuring property write invalidates later shape",
        "logical Number.prototype alias invalidates toString fast path",
        "joined Number.prototype alias invalidates toString fast path",
        "dynamic Number.prototype key invalidates toString fast path",
        "logical Boolean.prototype alias invalidates toString fast path",
        "shadowed Number write preserves intrinsic toString fast path",
        "logical Array.prototype write disables builtin fast path",
    ] {
        assert!(FIXTURE.contains(marker), "fixture lost oracle {marker}");
    }
    assert!(CLI_SOURCE.contains(
        "fn run_wasm_backend_preserves_ordinary_property_logical_assignment_reference()"
    ));
    assert!(CLI_SOURCE.contains("wasm_ordinary_property_logical_assignment_reference.js"));
}

#[test]
fn exact_current_pin_false_set_cohort_and_independent_controls_stay_unmasked() {
    assert_eq!(EXACT_FALSE_SET_WITNESSES.len(), 8);
    for (path, source) in EXACT_FALSE_SET_WITNESSES {
        assert!(source.contains("flags: [onlyStrict]"), "{path}");
        assert!(
            source.contains("features: [logical-assignment-operators]"),
            "{path}"
        );
        assert!(source.contains("assert.throws(TypeError"), "{path}");
        assert!(!RUNNER_SOURCE.contains(path), "runner masks {path}");
        assert!(!KNOWN_FAILURES.contains(path), "known failures mask {path}");
    }

    assert_eq!(ORDER_CONTROLS.len(), 3);
    for (path, source) in ORDER_CONTROLS {
        assert!(
            source.contains("LeftHandSideExpression is evaluated before"),
            "{path}"
        );
        assert!(source.contains("property key evaluated"), "{path}");
        assert!(
            source.contains("right-hand side expression evaluated"),
            "{path}"
        );
        assert!(!RUNNER_SOURCE.contains(path), "runner masks {path}");
        assert!(!KNOWN_FAILURES.contains(path), "known failures mask {path}");
    }

    assert_eq!(SHORT_CIRCUIT_CONTROLS.len(), 3);
    for (path, source) in SHORT_CIRCUIT_CONTROLS {
        assert!(
            source.contains("PutValue step is not reached")
                || source.contains("object whose [[Extensible]] internal property is false"),
            "{path}"
        );
        assert!(source.contains("flags: [onlyStrict]"), "{path}");
        assert!(!RUNNER_SOURCE.contains(path), "runner masks {path}");
        assert!(!KNOWN_FAILURES.contains(path), "known failures mask {path}");
    }
}
