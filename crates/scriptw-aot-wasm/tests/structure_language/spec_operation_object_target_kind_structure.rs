const OPERATIONS_SOURCE: &str = include_str!("../../src/operations.rs");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/spec-operation-object-target-kind.md");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker after {start}: {end}"))
        .0
}

#[test]
fn object_target_projection_classifies_the_complete_runtime_value() {
    let projection = bounded(
        OPERATIONS_SOURCE,
        "pub(crate) fn emit_is_heap_object_like_tag_i32(",
        "fn emit_numeric_bigint_operation(",
    );
    assert!(projection.contains("tag: I32Local"));
    for kind in ["Object", "Array", "Arguments", "Function"] {
        assert_eq!(
            projection
                .matches(&format!("WasmRuntimeValueTag::{kind}"))
                .count(),
            1
        );
    }
    for primitive in [
        "Undefined",
        "Null",
        "Boolean",
        "Number",
        "BigInt",
        "Symbol",
        "String",
    ] {
        assert!(!projection.contains(&format!("WasmRuntimeValueTag::{primitive}")));
    }
    assert!(projection.contains("Instruction::I32Eq"));
    assert!(projection.contains("Instruction::I32Or"));
}

#[test]
fn all_six_object_only_operations_consume_one_shared_runtime_admission() {
    let body = bounded(
        OPERATIONS_SOURCE,
        "            SpecOperationIr::Get\n            | SpecOperationIr::GetV",
        "            SpecOperationIr::CopyDataProperties => {",
    );
    let admission = bounded(
        body,
        "                match operation {",
        "                self.emit_value_to_property_key_completion(",
    );
    for operation in [
        "Get",
        "HasProperty",
        "HasOwnProperty",
        "DeletePropertyOrThrow",
        "Set",
        "CreateDataPropertyOrThrow",
    ] {
        assert!(admission.contains(&format!("SpecOperationIr::{operation}")));
    }
    assert_eq!(
        admission
            .matches("self.emit_is_heap_object_like_tag_i32(inputs[0].tag(), function);")
            .count(),
        1
    );
    let classify = admission
        .find("self.emit_is_heap_object_like_tag_i32(")
        .unwrap();
    let reject = admission.find("self.emit_throw_runtime_error(").unwrap();
    let exit = admission[reject..]
        .find("self.emit_branch_to_target(property_exit, function);")
        .unwrap()
        + reject;
    let publish = admission
        .find("lookup.copy_from(&inputs[0], function);")
        .unwrap();
    assert!(classify < reject && reject < exit && exit < publish);
    assert!(admission.contains("NativeErrorKind::TypeError,"));
    assert_eq!(
        admission
            .matches("self.emit_value_to_object_locals(&inputs[0], &pending, function)?;")
            .count(),
        1,
        "only GetV/GetMethod box the lookup target"
    );
    assert!(!body.contains("match target.kind"));
    assert!(!body.contains("if target.kind == ValueKind::Dynamic"));
}

#[test]
fn property_operations_keep_completed_key_and_original_receiver_authorities() {
    let body = bounded(
        OPERATIONS_SOURCE,
        "            SpecOperationIr::Get\n            | SpecOperationIr::GetV",
        "            SpecOperationIr::CopyDataProperties => {",
    );
    assert_eq!(
        body.matches("let key = PropertyKeyLocals::from_converted_value(key_value);")
            .count(),
        1
    );
    assert!(body.contains("key_result.kind().load(function);"));
    assert!(body.contains("pending.copy_from(&key_result, function);"));
    let convert = body
        .find("self.emit_value_to_property_key_completion(")
        .unwrap();
    let reject = body
        .find("pending.copy_from(&key_result, function);")
        .unwrap();
    let proof = body
        .find("let key = PropertyKeyLocals::from_converted_value(key_value);")
        .unwrap();
    let dispatch = body
        .find("self.emit_dynamic_property_read_with_key_locals(")
        .unwrap();
    assert!(convert < reject && reject < proof && proof < dispatch);
    assert!(body[reject..proof].contains("self.emit_branch_to_target(property_exit, function);"));
    assert!(body.contains("Some(key)"));
    let cleanup = body.find("if let Some(key) = key {").unwrap();
    let close_exit = body
        .rfind("self.pop_control(ControlFrameKind::Block);")
        .unwrap();
    assert!(
        close_exit < cleanup,
        "all abrupt exits share the key root cleanup"
    );
    assert!(body.contains("&lookup, &inputs[0], &key, &pending, function,"));
    assert!(body.contains("crate::runtime_helpers::OrdinarySetArguments::new("));
    assert!(body.contains("self.emit_create_data_property_or_throw("));
}

#[test]
fn contract_records_shared_object_target_ownership() {
    for source in [CONTRACT] {
        assert!(source.contains("SpecOperationObjectTargetKind"));
        assert!(source.contains("StaticallyObjectLike"));
        assert!(source.contains("RuntimeDynamic"));
        assert!(source.contains("StaticallyPrimitive"));
        assert!(source.contains("six") || source.contains("Six"));
    }
}
