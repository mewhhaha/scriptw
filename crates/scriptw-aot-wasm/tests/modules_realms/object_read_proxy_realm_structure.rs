const EMIT_SOURCE: &str = include_str!("../../src/emit.rs");
const ENTRY_SOURCE: &str = include_str!("../../src/emit/body_entry.rs");
const HELPER_SOURCE: &str = include_str!("../../src/objects/runtime_helpers.rs");
const HELPER_DOMAIN: &str = include_str!("../../src/runtime_helpers.rs");
const ERROR_SOURCE: &str = include_str!("../../src/builtins/errors/runtime_error.rs");
const PROXY_REALM_SOURCE: &str = include_str!("../../src/functions/proxy_execution_realm.rs");
const OBJECTS_SOURCE: &str = include_str!("../../src/objects.rs");
const ALLOCATION_SOURCE: &str = include_str!("../../src/objects/allocation.rs");
const TO_OBJECT_SOURCE: &str = include_str!("../../src/operations/to_object.rs");
const CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/object.rs");
const CLI_FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_object_prototype_to_string_proxy_array.js"
);

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end after {start}: {end}"))
        .0
}

#[test]
fn object_read_realm_source_exhaustively_projects_every_helper_body() {
    // The registered whole-value rows carry the caller Environment together
    // with target, receiver and key; helper entry cannot choose another row.
    for helper in ["ObjectRead", "ObjectReadProxy"] {
        let row = HELPER_DOMAIN
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("{helper} /")))
            .expect("registered Get helper row");
        assert!(row.contains(
            "target:Value,receiver:Value,key:Key,caller_environment:(Ref Environment Nullable)"
        ));
        assert!(row.ends_with("=> Completion;"));
    }
    let entry = bounded(
        ENTRY_SOURCE,
        "pub(crate) fn begin_helper_body(",
        "    pub(super) fn init_current_env(",
    );
    assert!(entry.contains("FunctionModuleState::RuntimeOperation(actual) if actual == helper"));
    let parameters = bounded(
        EMIT_SOURCE,
        "pub(crate) fn helper_parameters<",
        "    fn new_main(",
    );
    assert!(parameters.contains("FunctionModuleState::RuntimeOperation(actual) if actual == P::ID"));
    assert!(parameters.contains("parameters.caller_environment()"));
    assert!(parameters.contains(".replace(environment.load(self.schema, function), function)"));
    assert!(parameters.contains("parameters.caller_function_context()"));
    assert!(parameters.contains("context.load(self.schema, function)"));
    let context = bounded(
        EMIT_SOURCE,
        "pub(crate) fn current_function_context(",
        "pub(crate) fn clear_helper_function_context(",
    );
    assert!(context.contains("entry.function_context()"));
    assert!(context.contains(".or(self.helper_function_context.as_ref())"));
    assert!(!context.contains("load_current_realm"));

    // Real error allocation follows a defining FunctionContext or the retained
    // caller's Environment chain. The typed null case uses the active Realm.
    let realm = bounded(
        ERROR_SOURCE,
        "pub(crate) fn emit_execution_realm(",
        "pub(crate) fn emit_runtime_error_object(",
    );
    for authority in [
        "self.current_function_context()",
        "FunctionContextSchema::REALM",
        "self.current_environment().load(schema, function)",
        "EnvironmentSchema::DEFINING_REALM",
        "EnvironmentSchema::PARENT",
        "self.load_current_realm(function)",
    ] {
        assert!(
            realm.contains(authority),
            "missing actual Realm authority: {authority}"
        );
    }
    assert!(PROXY_REALM_SOURCE.contains("self.emit_throw_runtime_error("));
}

#[test]
fn outlined_and_inline_proxy_reads_consume_only_the_typed_realm_projection() {
    let read = bounded(
        OBJECTS_SOURCE,
        "pub(crate) fn emit_dynamic_property_read_with_key_locals(",
        "    fn compile_property_key_operand(",
    );
    assert!(read.contains("ObjectReadArguments::new("));
    assert!(read.contains("self.current_environment(),"));
    assert!(read.contains(".store(result, function)"));
    for (start, end, parameters, operation) in [
        (
            "fn compile_object_read_helper(",
            "fn compile_object_get_prototype_of_helper(",
            "ObjectReadParameters",
            "self.emit_object_read_kernel(",
        ),
        (
            "fn compile_object_read_proxy_helper(",
            "\n}",
            "ObjectReadProxyParameters",
            "self.emit_dynamic_property_read_with_key_locals(",
        ),
    ] {
        let body = bounded(HELPER_SOURCE, start, end);
        assert!(body.contains(parameters));
        assert!(body.contains(operation));
        assert!(body.contains("&parameters.target,"));
        assert!(body.contains("&parameters.receiver,"));
        assert!(body.contains("&parameters.key,"));
        assert!(!body.contains("self.emit_object_read_with_throw_routing("));
        let publish = body
            .find("result.emit(&mut function)")
            .expect("whole completion publication");
        let release = body
            .find("parameters.release(&mut function)")
            .expect("root release");
        assert!(
            publish < release,
            "helper arguments stay rooted until completion publication"
        );
    }
    let ordinary = bounded(
        OBJECTS_SOURCE,
        "    fn emit_object_read_kernel(",
        "    fn emit_non_proxy_object_get(",
    );
    let validate = ordinary
        .find("self.emit_load_live_proxy_slots(")
        .expect("validated Proxy slots");
    let get = ordinary
        .find("builder.emit_proxy_get(")
        .expect("Get follows slot validation");
    assert!(validate < get);
    assert!(ordinary.contains("ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler"));
    let routes = bounded(
        OBJECTS_SOURCE,
        "        match route {",
        "        function.instruction(&Instruction::Else);",
    );
    assert!(!routes.contains("_ =>"));
    assert!(routes.contains("ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler => self"));
    assert!(routes.contains(".emit_proxy_execution_realm_type_error("));
    assert!(routes.contains("RuntimeErrorMessage::PROXY_HANDLER_IS_NULL"));
    let routed = bounded(
        HELPER_SOURCE,
        "pub(crate) fn emit_object_read_with_throw_routing(",
        "pub(crate) fn compile_object_read_helper(",
    );
    assert!(routed.contains("self.emit_dynamic_property_read_with_key_locals("));
    assert!(routed.contains("AccessorThrowRouting::BreakToOrdinaryReadExit =>"));
    assert!(routed.contains("AccessorThrowRouting::LeaveInCompletion =>"));
    assert!(routed.contains("self.completion().copy_from(result, function)"));
    assert!(routed.contains("self.emit_propagate_current_throw_if_needed(function)"));
    assert!(!routed.contains("emit_load_live_proxy_slots"));
    assert!(!routed.contains("_ =>"));
    let fallback = bounded(
        OBJECTS_SOURCE,
        "    fn emit_proxy_get(",
        "    fn emit_proxy_get_invariant_check(",
    );
    assert!(fallback.contains("slots.target(),"));
    assert!(fallback.contains("receiver,"));
    assert!(fallback.contains("key,"));
    assert!(fallback.contains("self.current_environment(),"));
}

#[test]
fn object_operation_facades_preserve_registered_abi_and_private_physical_owners() {
    for (operation, row, parameters, compiler, next_compiler) in [
        (
            "get_prototype_of",
            "ObjectGetPrototypeOf",
            "ObjectGetPrototypeOfParameters",
            "compile_object_get_prototype_of_helper",
            "compile_object_set_prototype_of_helper",
        ),
        (
            "set_prototype_of",
            "ObjectSetPrototypeOf",
            "ObjectSetPrototypeOfParameters",
            "compile_object_set_prototype_of_helper",
            "compile_object_delete_helper",
        ),
        (
            "delete",
            "ObjectDelete",
            "ObjectDeleteParameters",
            "compile_object_delete_helper",
            "compile_object_is_extensible_helper",
        ),
        (
            "is_extensible",
            "ObjectIsExtensible",
            "ObjectIsExtensibleParameters",
            "compile_object_is_extensible_helper",
            "compile_object_prevent_extensions_helper",
        ),
        (
            "prevent_extensions",
            "ObjectPreventExtensions",
            "ObjectPreventExtensionsParameters",
            "compile_object_prevent_extensions_helper",
            "compile_object_read_proxy_helper",
        ),
    ] {
        let facade_start = format!("pub(crate) fn emit_object_{operation}(");
        let facade = bounded(HELPER_SOURCE, &facade_start, &format!("fn {compiler}("));
        assert!(facade.contains(&format!("{row}Arguments::new(")));
        assert!(facade.contains("self.current_environment()"));
        assert!(facade.contains(".store(result, function)"));
        assert!(!facade.contains("emit_load_live_proxy_slots"));
        assert!(!facade.contains(&format!("self.emit_object_{operation}_kernel(")));
        assert!(!OBJECTS_SOURCE.contains(&format!("pub(crate) fn emit_object_{operation}_kernel(")));
        let body = bounded(
            HELPER_SOURCE,
            &format!("fn {compiler}("),
            &format!("fn {next_compiler}("),
        );
        assert!(body.contains(&format!("begin_helper_body(RuntimeHelperId::{row})")));
        assert!(body.contains(parameters));
        assert!(body.contains(&format!("self.emit_object_{operation}_kernel(")));
        assert!(!body.contains(&format!("self.emit_object_{operation}(")));
        assert!(
            body.find("result.emit(&mut function)").unwrap()
                < body.find("parameters.release(&mut function)").unwrap()
        );
    }
    assert!(!OBJECTS_SOURCE.contains("pub(crate) fn emit_object_read_kernel("));
    let get = bounded(
        HELPER_SOURCE,
        "fn compile_object_read_helper(",
        "fn compile_object_get_prototype_of_helper(",
    );
    assert!(get.contains("begin_helper_body(RuntimeHelperId::ObjectRead)"));
    assert!(get.contains("self.emit_object_read_kernel("));
    let proxy_forward = bounded(HELPER_SOURCE, "fn compile_object_read_proxy_helper(", "\n}");
    assert!(proxy_forward.contains("self.emit_dynamic_property_read_with_key_locals("));
    assert!(!proxy_forward.contains("self.emit_object_read_kernel("));
}

#[test]
fn borrowed_created_realm_array_to_string_pins_revoked_get_error_realm() {
    for marker in [
        "other.Array.prototype.toString.call(revocable.proxy)",
        "Object.getPrototypeOf(otherArrayToStringError) === other.TypeError.prototype",
    ] {
        assert!(
            CLI_FIXTURE.contains(marker),
            "missing fixture marker: {marker}"
        );
    }
    assert!(CLI_TESTS.contains(
        "fn object_prototype_tostring_classifies_proxy_arrays_and_rejects_revoked_proxies()"
    ));
    assert!(CLI_TESTS.contains("wasm_object_prototype_to_string_proxy_array.js"));
}

#[test]
fn ordinary_allocation_and_to_object_keep_one_private_kernel_and_explicit_operands() {
    let allocation_row = HELPER_DOMAIN
        .lines()
        .find(|line| line.trim_start().starts_with("OrdinaryObjectAllocate /"))
        .expect("ordinary allocation helper row");
    assert!(allocation_row.contains("prototype:Value,immutable_prototype:I32"));
    assert!(allocation_row.ends_with("=> (Ref OrdinaryObject NonNullable);"));
    let boxing_row = HELPER_DOMAIN
        .lines()
        .find(|line| line.trim_start().starts_with("ValueToObject /"))
        .expect("explicit Realm ToObject helper row");
    assert!(boxing_row.contains("realm:(Ref RealmRecord NonNullable),input:Value"));
    assert!(boxing_row.ends_with("=> Completion;"));

    let allocation = bounded(
        ALLOCATION_SOURCE,
        "fn emit_allocate_object_header(",
        "fn compile_ordinary_object_allocate_helper(",
    );
    assert!(allocation.contains("ObjectPrototypeMutability"));
    assert!(allocation.contains("ScalarValue::Null"));
    let allocation_arguments: String = allocation
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    assert!(allocation_arguments
        .contains("OrdinaryObjectAllocateArguments::new(prototype,immutable_prototype"));
    assert!(allocation.contains("helper_reference_on_stack"));
    assert!(!allocation.contains(".construct("));
    let allocation_body = bounded(
        ALLOCATION_SOURCE,
        "fn compile_ordinary_object_allocate_helper(",
        "fn emit_alloc_object_header_kernel(",
    );
    assert!(allocation_body.contains("begin_helper_body(RuntimeHelperId::OrdinaryObjectAllocate)"));
    assert!(allocation_body.contains("self.emit_alloc_object_header_kernel("));
    assert!(!allocation_body.contains("self.emit_allocate_object_header("));
    assert!(!ALLOCATION_SOURCE.contains("pub(crate) fn emit_alloc_object_header_kernel("));

    let routes = bounded(
        TO_OBJECT_SOURCE,
        "fn emit_value_to_object_locals(",
        "fn emit_value_to_object_in_realm_locals(",
    );
    for authority in [
        "self.load_current_realm(function)",
        "self.emit_current_function_realm(function)",
    ] {
        assert!(
            routes.contains(authority),
            "missing original Realm selection: {authority}"
        );
    }
    let call = bounded(
        TO_OBJECT_SOURCE,
        "fn emit_value_to_object_in_realm_locals(",
        "fn compile_value_to_object_helper(",
    );
    assert!(call.contains("ValueToObjectArguments::new(realm, input)"));
    assert!(call.contains(".store(result, function)"));
    let boxing_body = bounded(
        TO_OBJECT_SOURCE,
        "fn compile_value_to_object_helper(",
        "fn emit_value_to_object_in_realm_kernel(",
    );
    assert!(boxing_body.contains("begin_helper_body(RuntimeHelperId::ValueToObject)"));
    for operand in ["&parameters.realm", "&parameters.input", "&result"] {
        assert!(boxing_body.contains(operand));
    }
    assert!(!boxing_body.contains("self.emit_value_to_object_locals("));
    assert!(
        boxing_body.find("result.emit(&mut function)").unwrap()
            < boxing_body
                .find("parameters.release(&mut function)")
                .unwrap()
    );
    let kernel = bounded(
        TO_OBJECT_SOURCE,
        "fn emit_value_to_object_in_realm_kernel(",
        "\n}",
    );
    assert!(kernel.contains("result.set_normal(input, function)"));
    assert!(kernel
        .contains("self.emit_load_non_array_realm_intrinsic(realm, slot, &prototype, function)"));
    assert!(kernel.contains("self.emit_throw_runtime_error_with_prototype("));
    assert!(!kernel.contains("load_current_realm"));
    assert!(!kernel.contains("emit_current_function_realm"));
    assert!(!TO_OBJECT_SOURCE.contains("pub(crate) fn emit_value_to_object_in_realm_kernel("));
}
