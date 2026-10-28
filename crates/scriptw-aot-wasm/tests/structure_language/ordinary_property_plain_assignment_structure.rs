const REFERENCE_SOURCE: &str = include_str!("../../../scriptw-ir/src/reference.rs");
const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/ir.rs");
const ASSIGNMENT_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering/assignment.rs");
const ORDINARY_PROPERTY_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/ordinary_property_compound.rs");
const FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_ordinary_property_assignment_reference.js"
);
const CLI_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/language_numerics.rs");
const RUNNER_SOURCE: &str = include_str!("../../../scriptw-test262/src/lib.rs");
const KNOWN_FAILURES: &str = include_str!("../../../scriptw-cli/tests/known-failures.tsv");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/ordinary-property-plain-assignment-reference.md"
);

const EXACT_TEST262: &[(&str, &str)] = &[
    (
        "target-member-computed-reference-null.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/expressions/assignment/target-member-computed-reference-null.js"
        ),
    ),
    (
        "target-member-identifier-reference-null.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/expressions/assignment/target-member-identifier-reference-null.js"
        ),
    ),
    (
        "target-member-identifier-reference-undefined.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/expressions/assignment/target-member-identifier-reference-undefined.js"
        ),
    ),
];

const CONTROL_TEST262: &[(&str, &str)] = &[
    (
        "target-member-computed-reference-undefined.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/expressions/assignment/target-member-computed-reference-undefined.js"
        ),
    ),
    (
        "target-member-computed-reference.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/expressions/assignment/target-member-computed-reference.js"
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
fn ir_owns_one_closed_plain_assignment_reference() {
    let carrier = bounded(
        REFERENCE_SOURCE,
        "pub struct OrdinaryPropertyAssignmentIr {",
        "/// One fused numeric update of an ordinary property Reference.",
    );
    for field in [
        "base_and_receiver: Box<TypedExpr>",
        "referenced_name: PropertyKeyIr",
        "rhs: Box<TypedExpr>",
        "strictness: Strictness",
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
        "strictness",
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
            "pub(crate) fn plain_assignment(",
            "rhs: TypedExpr",
            "possible_setters: PropertyHookTargets",
            "rhs.value_info()",
            "ExprIr::OrdinaryPropertyAssignment(OrdinaryPropertyAssignmentIr::new(",
            "self.base_and_receiver",
            "self.referenced_name",
            "Box::new(rhs)",
            "self.strictness",
            "possible_setters",
        ],
    );
    assert!(IR_SOURCE.contains("OrdinaryPropertyAssignment(OrdinaryPropertyAssignmentIr)"));
}

#[test]
fn lowering_builds_the_reference_before_rhs_and_intercepts_the_closed_ast_arm() {
    let helper = bounded(
        ORDINARY_PROPERTY_LOWERING_SOURCE,
        "    pub(super) fn lower_ordinary_property_plain_assignment(",
        "    /// Lower one ordinary property Reference directly into its fused eager",
    );
    positions_in_order(
        helper,
        &[
            "self.lower_ordinary_property_reference_plan(access)",
            "let rhs_value = self.lower_expression(rhs);",
            "self.possible_ordinary_property_setters(&metadata, rhs_may_have_intervening_effects)",
            "plan.plain_assignment(rhs_value, possible_setters)",
        ],
    );
    assert!(!helper.contains("ExprIr::PropertyWrite"));

    let assign_target = bounded(
        ASSIGNMENT_SOURCE,
        "                AssignTarget::Access(access) => match access {",
        "                AssignTarget::Pattern(pattern)",
    );
    positions_in_order(
        assign_target,
        &[
            "PropertyAccess::Simple(access) => {",
            "self.lower_ordinary_property_plain_assignment(access, rhs)",
            "PropertyAccess::Private(_) | PropertyAccess::Super(_) => {",
            "self.lower_property_assign(access, rhs)",
        ],
    );
    assert!(ORDINARY_PROPERTY_LOWERING_SOURCE
        .contains("fn ordinary_property_plain_assignment_retains_base_key_rhs_and_strictness()"));
}

#[test]
fn fixture_observes_the_plain_put_value_lifecycle() {
    for marker in [
        "complete plain property Reference lifecycle",
        "base,raw-key,rhs,to-key,proxy-set:p:true:7,setter:true:7",
        "sole ToPropertyKey",
        "abrupt base order",
        "abrupt raw key order",
        "nullishReference(null, \"null base\")",
        "nullishReference(undefined, \"undefined base\")",
        "staticNullishReference(null, \"null base\")",
        "staticNullishReference(undefined, \"undefined base\")",
        "RHS throw identity",
        "RHS mutation precedes ToPropertyKey",
        "mutated raw key sole ToPropertyKey",
        "abrupt Set nonpublication",
        "sloppy false Set result",
        "strict false Set nonpublication",
        "sloppy primitive assignment result",
        "strict primitive Set nonpublication",
    ] {
        assert!(FIXTURE.contains(marker), "fixture lost oracle {marker}");
    }
    assert!(FIXTURE.contains("\"use strict\";\n  return falseSetProxy.p = 13;"));
    assert!(FIXTURE.contains("\"use strict\";\n  return (1).p = 15;"));
    assert!(CLI_SOURCE
        .contains("fn run_wasm_backend_preserves_ordinary_property_plain_assignment_reference()"));
    assert!(CLI_SOURCE.contains("wasm_ordinary_property_assignment_reference.js"));
}

#[test]
fn exact_inventory_is_raw_unmasked_and_controls_remain_separate() {
    assert_eq!(EXACT_TEST262.len(), 3);
    assert_eq!(CONTROL_TEST262.len(), 2);
    for (path, source) in EXACT_TEST262.iter().chain(CONTROL_TEST262) {
        assert!(
            !source.contains("flags:"),
            "{path} must execute in sloppy and strict modes"
        );
        assert!(!RUNNER_SOURCE.contains(path), "runner masks {path}");
        assert!(!KNOWN_FAILURES.contains(path), "known failures mask {path}");
        assert!(source.contains("sec-assignment-operators"));
    }
    assert!(EXACT_TEST262[0].1.contains("property key evaluated"));
    assert!(EXACT_TEST262[0]
        .1
        .contains("right-hand side expression evaluated"));
    for (_, source) in &EXACT_TEST262[1..] {
        assert!(source.contains("count += 1"));
        assert!(source.contains("assert.sameValue(count, 1)"));
    }
}

#[test]
fn verified_status_records_the_exact_baseline_results_and_nonclaims() {
    for marker in [
        "three physical files produce six",
        "evaluate `rhs`",
        "apply `ToObject`",
        "apply `ToPropertyKey` exactly once",
        "perform `[[Set]]`",
        "publish `rhs` only after PutValue completes normally",
        "This batch does not change compound or logical assignment",
        "resumable property",
    ] {
        assert!(CONTRACT.contains(marker), "contract lost {marker}");
    }
}
