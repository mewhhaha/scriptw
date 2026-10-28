const DEFINE: &str = include_str!("../../src/objects/define_property.rs");
const OBJECTS: &str = include_str!("../../src/objects.rs");
const CARRIER: &str = include_str!("../../src/objects/descriptor_object.rs");
const ERROR: &str = include_str!("../../src/builtins/errors/runtime_error.rs");
fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}
fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing {end}"))
        .0
}
fn ordered(source: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        cursor += source[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing {marker}"))
            + marker.len();
    }
}

#[test]
fn compatibility_exhaustively_projects_absent_known_and_runtime_presence() {
    let presence = compact(between(DEFINE, "fn emit_presence<T>(", "fn emit_flag("));
    assert!(presence.contains("presence:&Presence<T,I32Local>"));
    for marker in [
        "Presence::Absent=>",
        "Instruction::I32Const(0)",
        "Presence::Present(_)=>",
        "Instruction::I32Const(1)",
        "Presence::Runtime{present,..}=>present.load(function)",
    ] {
        assert!(presence.contains(marker));
    }
    assert!(!presence.contains("_=>"));
}

#[test]
fn compatibility_exhaustively_projects_known_and_runtime_flags() {
    let flag = compact(between(DEFINE, "fn emit_flag(", "fn emit_mask("));
    assert!(flag.contains("DescriptorFlag::Known(value)=>"));
    assert!(flag.contains("Instruction::I32Const(i32::from(value))"));
    assert!(flag.contains("DescriptorFlag::BooleanPayload(value)=>value.load(function)"));
    assert!(!flag.contains("_=>"));
}

#[test]
fn compatibility_borrows_complete_value_roles_without_raw_word_authority() {
    let carrier = compact(between(
        CARRIER,
        "impl<'v> DescriptorCarrier",
        "pub(crate) type DescriptorObjectFields",
    ));
    assert!(carrier.contains("typeValue=&'vValueLocals;"));
    assert!(carrier.contains("typeFlag=DescriptorFlag;"));
    assert!(carrier.contains("typeRuntimeFlag=I32Local;"));
    let validator = compact(between(
        DEFINE,
        "pub(crate) fn emit_validate_stored_descriptor(",
        "fn emit_descriptor_reject_if_true(",
    ));
    assert!(validator.contains(
        "current:&GcLocal<PropertyDescriptor>,incoming:&WasmDescriptor<'_>,valid:I32Local,"
    ));
    assert!(!validator.contains("current:u32"));
    assert!(OBJECTS.contains("type WasmDescriptor<'v> = ValidatedDescriptor<WasmLocals<'v>>"));
}

#[test]
fn ordinary_and_indexed_definitions_share_the_complete_compatibility_check() {
    for (start, end) in [
        (
            "fn emit_ordinary_define_own_property(",
            "fn emit_copy_complete_descriptor(",
        ),
        (
            "fn emit_define_indexed_descriptor(",
            "fn emit_define_array_indexed_descriptor(",
        ),
    ] {
        let body = compact(between(DEFINE, start, end));
        ordered(
            &body,
            &[
                "OrdinaryObjectSchema::EXTENSIBLE",
                "self.emit_validate_stored_descriptor(",
                "valid.load(function);",
                "self.emit_merge_property_descriptor(",
            ],
        );
        assert_eq!(
            body.matches("self.emit_validate_stored_descriptor(")
                .count(),
            1
        );
    }
    let validator = compact(between(
        DEFINE,
        "pub(crate) fn emit_validate_stored_descriptor(",
        "fn emit_descriptor_reject_if_true(",
    ));
    for field in ["configurable", "enumerable", "writable", "value"] {
        assert!(validator.contains(&format!("emit_presence(&descriptor.{field},function)")));
    }
    assert!(validator.contains("(DescriptorField::Get,&descriptor.get)"));
    assert!(validator.contains("(DescriptorField::Set,&descriptor.set)"));
    assert!(validator.contains("emit_presence(presence,function);"));
    assert_eq!(
        validator
            .matches("self.emit_tagged_payload_same_value_i32(")
            .count(),
        2
    );
    assert!(!validator.contains("emit_object_define_entry_validated("));
}

#[test]
fn static_and_runtime_kind_terms_share_the_same_projection() {
    let terms = compact(between(DEFINE, "fn emit_terms(", "impl FunctionBuilder"));
    ordered(
        &terms,
        &[
            "i32::from(terms.statically_true)",
            "forflaginterms.runtime_flags(){",
            "flag.load(function);",
            "Instruction::I32Or",
        ],
    );
    let validation = compact(between(
        DEFINE,
        "pub(crate) fn emit_validate_stored_descriptor(",
        "fn emit_descriptor_reject_if_true(",
    ));
    ordered(
        &validation,
        &[
            "classify(incoming)",
            "classification.terms(DescriptorSide::Data)",
            "classification.terms(DescriptorSide::Accessor)",
            "emit_terms(accessor_terms,function);",
            "emit_terms(data_terms,function);",
        ],
    );
    let merge = compact(between(
        DEFINE,
        "fn emit_merge_property_descriptor(",
        "pub(crate) fn emit_object_define_data_with_flag_locals(",
    ));
    assert!(merge.contains("emit_terms(classification.terms(DescriptorSide::Data),function)"));
    assert!(merge.contains("emit_terms(classification.terms(DescriptorSide::Accessor),function)"));
}

#[test]
fn fresh_runtime_errors_append_message_without_recursive_validation() {
    let helper = compact(between(
        ERROR,
        "pub(crate) fn compile_runtime_error_object_helper(",
        "pub(crate) fn compile_runtime_error_for_throw_helper(",
    ));
    assert!(helper.contains("self.begin_helper_body(RuntimeHelperId::RuntimeErrorObject)"));
    assert_eq!(
        helper
            .matches("emit_object_append_data_property_with_flags(")
            .count(),
        1
    );
    assert!(helper.contains("self.emit_runtime_error_key(\"message\",&mutfunction)?"));
    assert!(helper.contains("&value,true,false,true,&mutfunction,"));
    assert!(!helper.contains("emit_object_define_data("));
    assert!(!helper.contains("emit_object_define_entry_validated("));
    // name is inherited from the selected Realm prototype; the fresh error owns message.
    let call = compact(between(
        ERROR,
        "fn emit_fresh_native_error_object(",
        "pub(crate) fn compile_runtime_error_object_helper(",
    ));
    assert!(call.contains("RuntimeErrorObjectArguments::new(prototype,&message)"));
    let throwing = compact(between(
        ERROR,
        "pub(crate) fn emit_throw_runtime_error_with_prototype(",
        "pub(crate) fn emit_throw_current_function_realm_type_error(",
    ));
    assert!(throwing.contains("RuntimeErrorForThrowArguments::new(kind,prototype,&message)"));
    assert!(!throwing.contains("self.emit_fresh_native_error_object("));
    assert!(!call.contains("emit_object_append_data_property_with_flags("));
}
