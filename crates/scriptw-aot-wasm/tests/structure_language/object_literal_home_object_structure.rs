const FUNCTION_PROTOCOL_SOURCE: &str = include_str!("../../../scriptw-ir/src/function_protocol.rs");
const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/ir.rs");
const ANALYSIS_SOURCE: &str = include_str!("../../../scriptw-ir/src/analysis.rs");
const LOWERING_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering.rs");
const OBJECT_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/object_literal.rs");
const SUPER_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/super_property_mutation.rs");
const FUNCTION_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/function_definition.rs");
const LOWERING_HELPERS_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering_helpers.rs");
const REFERENCE_SOURCE: &str = include_str!("../../../scriptw-ir/src/reference.rs");
const FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_object_literal_home_object.js");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/object-literal-home-object.md");

macro_rules! witness {
    ($path:literal) => {
        (
            $path,
            include_str!(concat!("../../../../test262/vendor/test262/test/", $path)),
        )
    };
}

const SELECTED_WITNESSES: [(&str, &str); 5] = [
    witness!("language/expressions/object/method.js"),
    witness!("language/expressions/object/method-definition/name-super-prop-body.js"),
    witness!("language/expressions/object/method-definition/name-super-prop-param.js"),
    witness!("language/expressions/object/getter-super-prop.js"),
    witness!("language/expressions/object/setter-super-prop.js"),
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
    let earlier_index = source
        .find(earlier)
        .unwrap_or_else(|| panic!("missing earlier marker: {earlier}"));
    let later_index = source
        .find(later)
        .unwrap_or_else(|| panic!("missing later marker: {later}"));
    assert!(
        earlier_index < later_index,
        "`{earlier}` must precede `{later}`"
    );
}

#[test]
fn object_method_protocol_and_private_carrier_close_the_ir_domain() {
    let protocol = bounded(
        FUNCTION_PROTOCOL_SOURCE,
        "pub enum FunctionProtocolIr {",
        "impl FunctionProtocolIr {",
    );
    for marker in [
        "ObjectMethod(FunctionExecutionKind)",
        "ObjectGetter",
        "ObjectSetter",
    ] {
        assert!(
            protocol.contains(marker),
            "missing protocol variant: {marker}"
        );
    }

    let flavor = bounded(
        FUNCTION_PROTOCOL_SOURCE,
        "    pub const fn flavor(self)",
        "    pub const fn execution_kind(self)",
    );
    let execution = bounded(
        FUNCTION_PROTOCOL_SOURCE,
        "    pub const fn execution_kind(self)",
        "    pub const fn is_constructable(self)",
    );
    let constructability = bounded(
        FUNCTION_PROTOCOL_SOURCE,
        "    pub const fn is_constructable(self)",
        "    pub const fn class_kind(self)",
    );
    let class = bounded(
        FUNCTION_PROTOCOL_SOURCE,
        "    pub const fn class_kind(self)",
        "    pub const fn is_object_literal_method(self)",
    );
    for query in [flavor, class] {
        for marker in [
            "Self::ObjectMethod(_)",
            "Self::ObjectGetter",
            "Self::ObjectSetter",
        ] {
            assert!(query.contains(marker), "missing protocol query: {marker}");
        }
        assert!(!query.contains("_ =>"));
    }
    for marker in [
        "Self::ObjectMethod(kind) => kind",
        "Self::ObjectGetter",
        "Self::ObjectSetter",
    ] {
        assert!(
            execution.contains(marker),
            "missing execution-kind query: {marker}"
        );
    }
    assert!(!execution.contains("_ =>"));
    assert!(!constructability.contains("Self::ObjectMethod"));
    assert!(!constructability.contains("Self::ObjectGetter"));
    assert!(!constructability.contains("Self::ObjectSetter"));
    let object_role = bounded(
        FUNCTION_PROTOCOL_SOURCE,
        "    pub const fn is_object_literal_method(self)",
        "\n    }\n}",
    );
    for marker in [
        "Self::ObjectMethod(_)",
        "Self::ObjectGetter",
        "Self::ObjectSetter",
    ] {
        assert!(
            object_role.contains(marker),
            "missing object-role query: {marker}"
        );
    }

    let carrier = bounded(
        IR_SOURCE,
        "/// Exact function identity for an object-literal method",
        "#[derive(Debug, Clone, PartialEq, Eq)]\npub enum ObjectPropertyIr",
    );
    for marker in [
        "#[must_use = \"an object-method function must be materialized with its HomeObject\"]",
        "pub struct ObjectMethodFunctionIr {",
        "function_id: FunctionId",
        "protocol: FunctionProtocolIr",
        "pub(crate) enum ObjectMethodProtocolIr {",
        "Method(FunctionExecutionKind)",
        "pub(crate) fn new(function_id: FunctionId, protocol: ObjectMethodProtocolIr)",
        "pub fn function_id(&self) -> &FunctionId",
        "pub const fn protocol(&self) -> FunctionProtocolIr",
    ] {
        assert!(carrier.contains(marker), "missing carrier marker: {marker}");
    }
    assert!(!carrier.contains("pub function_id:"));
    assert!(!carrier.contains("pub protocol:"));
    assert!(!carrier.contains("pub fn new("));

    let properties = bounded(
        IR_SOURCE,
        "pub enum ObjectPropertyIr {",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum PrivateElementKindIr",
    );
    assert_eq!(
        properties
            .matches("function: ObjectMethodFunctionIr")
            .count(),
        6
    );
    assert!(!properties.contains("function: TypedExpr"));

    let mapper = bounded(
        LOWERING_HELPERS_SOURCE,
        "pub(crate) const fn object_method_protocol(",
        "pub(crate) fn for_in_loop_binding_storage_name(",
    );
    for marker in [
        "MethodDefinitionKind::Ordinary",
        "MethodDefinitionKind::Generator",
        "MethodDefinitionKind::Async",
        "MethodDefinitionKind::AsyncGenerator",
        "MethodDefinitionKind::Get",
        "MethodDefinitionKind::Set",
        "ObjectMethodProtocolIr::Method(FunctionExecutionKind::Ordinary)",
        "ObjectMethodProtocolIr::Getter",
        "ObjectMethodProtocolIr::Setter",
    ] {
        assert!(
            mapper.contains(marker),
            "missing exhaustive mapper: {marker}"
        );
    }
    assert!(!mapper.contains("_ =>"));

    let analysis = bounded(
        ANALYSIS_SOURCE,
        "PropertyDefinition::MethodDefinition(method) => {",
        "PropertyDefinition::IdentifierReference(identifier) =>",
    );
    assert!(analysis.contains("protocol: object_method_protocol(method.kind())"));
    assert!(analysis.contains(".function_protocol()"));

    let producer = bounded(
        OBJECT_LOWERING_SOURCE,
        "    fn lower_object_method_function(",
        "    pub(super) fn lower_object_literal(",
    );
    assert!(producer.contains("ObjectMethodFunctionIr::new("));
    assert!(producer.contains("object_method_protocol(method.kind())"));
    assert!(!producer.contains("ExprIr::FunctionValue"));
}

#[test]
fn super_references_carry_receiver_and_parameter_initializers_gain_context_first() {
    let expressions = bounded(IR_SOURCE, "    SuperPropertyRead {", "    PrivateRead {");
    assert_eq!(expressions.matches("receiver: Box<TypedExpr>").count(), 2);

    let reference = bounded(
        REFERENCE_SOURCE,
        "pub(crate) enum ReferenceBase {",
        "impl ReferenceBase {",
    );
    assert!(
        reference.contains("Super {\n        key: PropertyKeyIr,\n        receiver: TypedExpr,")
    );

    let read_write = bounded(
        REFERENCE_SOURCE,
        "impl ReferenceBase {",
        "/// Why a lowered read is not usable as a Reference.",
    );
    assert!(read_write.contains("Self::Super { key, receiver } => ExprIr::SuperPropertyRead"));
    assert!(read_write.contains("Self::Super { key, receiver } => ExprIr::SuperPropertyWrite"));
    assert!(read_write.contains("receiver: Box::new(receiver.clone())"));
    assert!(read_write.contains("receiver: Box::new(receiver)"));

    let function_lowering = bounded(
        FUNCTION_LOWERING_SOURCE,
        "        let lexical_derived_activation =",
        "        if let Some(self_binding_name) = function.activation_self_binding_name() {",
    );
    assert_before(
        function_lowering,
        "function.protocol.is_object_literal_method()",
        "lowerer.lower_function_parameters(",
    );

    let super_read = bounded(
        LOWERING_SOURCE,
        "    fn lower_super_property_access(",
        "    fn lower_private_in(",
    );
    assert_before(
        super_read,
        "self.lower_super_property_reference_parts(access)",
        "ExprIr::SuperPropertyRead { key, receiver }",
    );
    let reference_parts = bounded(
        SUPER_LOWERING_SOURCE,
        "    pub(super) fn lower_super_property_reference_parts(",
        "    fn lower_super_property_reference_plan(",
    );
    assert_before(
        reference_parts,
        "lower_super_property_receiver()?",
        "lower_super_property_key",
    );
    assert!(
        reference_parts.contains("let receiver = Box::new(self.lower_super_property_receiver()?);")
    );
    let receiver_gate = bounded(
        SUPER_LOWERING_SOURCE,
        "pub(super) fn lower_super_property_receiver(",
        "pub(super) fn lower_super_property_reference_parts(",
    );
    assert!(receiver_gate.contains("Some(self.lower_current_this())"));
    assert!(receiver_gate.contains("self.class_context.is_none()"));
    assert!(reference_parts.contains("Some((key, receiver, info))"));

    // `lower_property_assign_value` (for-in/of heads) delegates to the shared
    // reference-parts owner above, so it is not this plain-assignment arm.
    let property_assign = bounded(
        LOWERING_SOURCE,
        "    fn lower_property_assign(&mut self, access: &PropertyAccess, rhs: &Expression) -> TypedExpr {",
        "\n    /// 13.4 `++`/`--` on a property Reference.",
    );
    let super_write = bounded(
        property_assign,
        "            PropertyAccess::Super(access) => {",
        "\n        }\n    }",
    );
    assert_before(
        super_write,
        "lower_super_property_key",
        "lower_current_this",
    );
    assert_before(super_write, "lower_current_this", "lower_expression(rhs)");
    assert!(super_write.contains("receiver: Box::new(receiver)"));
}

#[test]
fn durable_fixture_and_exact_current_failure_inventory_bound_the_claim() {
    assert_eq!(SELECTED_WITNESSES.len(), 5);
    assert!(SELECTED_WITNESSES
        .iter()
        .all(|(_, source)| source.contains("super")));
    assert!(SELECTED_WITNESSES
        .iter()
        .all(|(_, source)| !source.contains("flags:")));
    assert!(SELECTED_WITNESSES[0]
        .1
        .contains("Object.setPrototypeOf(object, proto)"));
    assert!(SELECTED_WITNESSES[2]
        .1
        .contains("method(x = super.toString)"));
    assert!(SELECTED_WITNESSES[3].1.contains("get x()"));
    assert!(SELECTED_WITNESSES[4].1.contains("set x(v)"));

    for marker in [
        "method(suffix)",
        "parameterMethod(value = super.parameterValue)",
        "get namedAccessor()",
        "set namedAccessor(value)",
        "[computedKey(\"m\", \"computedMethod\")]",
        "get [computedKey(\"g\", \"computedAccessor\")]()",
        "set [computedKey(\"s\", \"computedAccessor\")](value)",
        "keyTrace === \"mgs\"",
        "method.call(alien, \"first\")",
        "Object.setPrototypeOf(literal, prototypeB)",
        "secondMethod === \"alien:B:second\"",
        "isNonConstructable(method)",
    ] {
        assert!(
            FIXTURE.contains(marker),
            "missing fixture contract: {marker}"
        );
    }
    assert!(!FIXTURE.contains("async "));
    assert!(!FIXTURE.contains("function*"));
    assert!(!FIXTURE.contains("=>"));

    for marker in [
        "At clean commit `304e4bbad3`",
        "physical files and ten sloppy/strict Script executions",
        "The existing Wasm binary reports `0/10`",
        "unsupported in scriptw wasm-aot first slice: object literal method",
        "Generator, async, and async-generator object methods remain explicit protocol",
        "Nested arrows using an enclosing object method's\n`super`",
        "keys share the closed IR carrier",
    ] {
        assert!(
            CONTRACT.contains(marker),
            "missing evidence boundary: {marker}"
        );
    }
}
