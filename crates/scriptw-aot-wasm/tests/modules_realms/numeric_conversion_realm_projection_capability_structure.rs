use std::fs;
use std::path::Path;

const ERROR_SOURCE: &str = include_str!("../../src/builtins/errors/runtime_error.rs");
const EMIT_SOURCE: &str = include_str!("../../src/emit.rs");
const HELPERS_SOURCE: &str = include_str!("../../src/runtime_helpers.rs");
const OPERATIONS_SOURCE: &str = include_str!("../../src/operations.rs");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/numeric-conversion-realm-projection-capability.md"
);

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn normalized(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn count_in_rust_sources(dir: &Path, needle: &str) -> usize {
    fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .map(|path| {
            if path.is_dir() {
                return count_in_rust_sources(&path, needle);
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return 0;
            }
            fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
                .matches(needle)
                .count()
        })
        .sum()
}

#[test]
fn numeric_realm_authority_is_a_rooted_realm_reference() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for retired in [
        "NumericConversionRealmAccess",
        "OutlinedNumericRealmArgument",
        "NumericConversionErrorRealm",
    ] {
        assert_eq!(count_in_rust_sources(&src, retired), 0);
    }
    let projection = normalized(bounded(
        ERROR_SOURCE,
        "pub(crate) fn emit_execution_realm(",
        "pub(crate) fn emit_runtime_error_object(",
    ));
    assert!(projection.contains(")->GcLocal<RealmRecord>"));
    assert!(!projection.contains("->u32"));
    assert!(!projection.contains("I64Const(0)"));
    assert!(!projection.contains("load_main_realm"));
    assert!(projection.contains("realm.load(schema,function).require_non_null(function)"));
    assert!(projection.contains("realm.clear(function);cursor.clear(function);resolved"));
}

#[test]
fn explicit_helper_callable_and_environment_sources_share_one_checked_projection() {
    let projection = normalized(bounded(
        ERROR_SOURCE,
        "pub(crate) fn emit_execution_realm(",
        "pub(crate) fn emit_runtime_error_object(",
    ));
    let explicit = projection
        .find("ifletSome(realm)=self.helper_execution_realm()")
        .unwrap();
    let callable = projection
        .find("ifletSome(context)=self.current_function_context()")
        .unwrap();
    let environment = projection
        .find("self.current_environment().load(schema,function)")
        .unwrap();
    let fallback = projection
        .find("letactive=self.load_current_realm(function)")
        .unwrap();
    assert!(explicit < callable && callable < environment && environment < fallback);
    for field in [
        "FunctionContextSchema::REALM",
        "EnvironmentSchema::DEFINING_REALM",
        "EnvironmentSchema::PARENT",
    ] {
        assert_eq!(projection.matches(field).count(), 1);
    }
    let parameters = normalized(bounded(
        EMIT_SOURCE,
        "pub(crate) fn helper_parameters<",
        "fn new_main(",
    ));
    for method in [
        "caller_program()",
        "caller_environment()",
        "caller_function_context()",
        "caller_execution_realm()",
    ] {
        assert_eq!(parameters.matches(method).count(), 1);
    }
    assert!(parameters.contains(
        "self.helper_execution_realm.is_none()&&self.current_function_context().is_none()"
    ));
    assert!(parameters.contains(
        "self.current_environment.replace(environment.load(self.schema,function),function)"
    ));
    assert!(parameters.contains(".initialize(realm.load(self.schema,function),function)"));
    let helpers = normalized(HELPERS_SOURCE);
    for (operation, name) in [
        ("ValueToNumber", "value_to_number"),
        ("ValueToNumeric", "value_to_numeric"),
        ("ValueToPrimitiveDefault", "value_to_primitive_default"),
        ("ValueToPrimitiveNumber", "value_to_primitive_number"),
        ("ValueToPrimitiveString", "value_to_primitive_string"),
    ] {
        assert!(helpers.contains(&format!("{operation}/{operation}Arguments/{operation}Parameters/\"{name}\"{{input:Value,caller_environment:(RefEnvironmentNullable),caller_program:Program}}=>Completion;")));
    }
    let operations = normalized(OPERATIONS_SOURCE);
    for operation in [
        "ValueToNumber",
        "ValueToNumeric",
        "ValueToPrimitiveDefault",
        "ValueToPrimitiveNumber",
        "ValueToPrimitiveString",
    ] {
        assert_eq!(
            operations.matches(&format!(
                "{operation}Arguments::new(input,self.current_environment(),self.current_program(),)"
            )).count(),
            1,
            "{operation} forwards its actual environment and P in declaration order"
        );
    }
    assert!(helpers.contains("caller_execution_realm:(RefRealmRecordNonNullable),caller_environment:(RefEnvironmentNullable),caller_program:Program}=>Completion;"));
}

#[test]
fn numeric_errors_preserve_kind_and_use_the_execution_realm_error_factory() {
    let errors = normalized(ERROR_SOURCE);
    let slots = bounded(&errors, "fnprototype_slot(", "implFunctionBuilder<'_>{");
    for kind in ["TypeError", "RangeError", "SyntaxError"] {
        assert!(slots.contains(&format!(
            "NativeErrorKind::{kind}=>NonArrayRealmIntrinsicSlot::{kind}Prototype"
        )));
    }
    assert!(!slots.contains("_=>"));
    let allocation = bounded(
        &errors,
        "pub(crate)fnemit_runtime_error_object(",
        "fnemit_fresh_native_error_object(",
    );
    assert!(allocation.contains("letrealm=self.emit_execution_realm(function);"));
    assert!(allocation.contains(
        "self.emit_load_non_array_realm_intrinsic(&realm,prototype_slot(kind),&prototype,function,"
    ));
    let thrown = bounded(
        &errors,
        "pub(crate)fnemit_throw_runtime_error(",
        "pub(crate)fnemit_throw_current_function_realm_error(",
    );
    assert!(thrown.contains("letrealm=self.emit_execution_realm(function);"));
    assert!(thrown.contains(
        "self.emit_load_non_array_realm_intrinsic(&realm,prototype_slot(kind),&prototype,function,"
    ));
    assert!(thrown.contains(
        "self.emit_throw_runtime_error_with_prototype(kind,message,&prototype,result,function)?;"
    ));
    let resolved = bounded(
        &errors,
        "pub(crate)fnemit_throw_runtime_error_with_prototype(",
        "pub(crate)fnemit_throw_current_function_realm_type_error(",
    );
    assert!(resolved.contains("RuntimeErrorForThrowArguments::new(kind,prototype,&message)"));
    assert!(resolved.contains("result.set_throw(&value,function);"));
    assert!(!resolved.contains("load_current_realm"));
    assert!(!resolved.contains("emit_execution_realm"));
    let operations = normalized(OPERATIONS_SOURCE);
    for marker in [
        "self.emit_throw_runtime_error(NativeErrorKind::TypeError,RuntimeErrorMessage::CANNOT_CONVERT_SYMBOL_TO_NUMBER,result,function,)",
        "self.emit_throw_runtime_error(NativeErrorKind::RangeError,error_message,result,function,)",
        "self.emit_throw_runtime_error(NativeErrorKind::SyntaxError,RuntimeErrorMessage::CANNOT_CONVERT_VALUE_TO_BIGINT,result,function,)",
    ] { assert!(operations.contains(marker), "lost {marker}"); }
    for forbidden in [
        "fn emit_numeric_conversion_type_error(",
        "fn emit_numeric_conversion_range_error(",
        "fn emit_numeric_conversion_syntax_error(",
    ] {
        assert!(!OPERATIONS_SOURCE.contains(forbidden));
    }
}

#[test]
fn contract_and_t04_record_the_source_equivalent_capability_closure() {
    for marker in [
        "one private, non-derived",
        "helper ABI parameter 6",
        "does not claim a completion-ABI redesign",
    ] {
        assert!(
            CONTRACT.contains(marker),
            "missing contract marker `{marker}`"
        );
    }
}
