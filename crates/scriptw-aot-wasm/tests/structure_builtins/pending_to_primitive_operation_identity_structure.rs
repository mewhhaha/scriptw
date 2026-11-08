use std::fs;
use std::path::Path;

const OPERATIONS_SOURCE: &str = include_str!("../../src/operations.rs");
const HELPER_SOURCE: &str = include_str!("../../src/runtime_helpers.rs");
const HELPER_COMPILERS: &str = include_str!("../../src/emit/runtime_operations.rs");
const VALUE_SOURCE: &str = include_str!("../../src/gc_types/value.rs");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/pending-to-primitive-operation-identity.md"
);

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker after: {start}"))
        .0
}

fn normalized(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .flat_map(str::chars)
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn recursive_rust_source_count(root: &Path, needle: &str) -> usize {
    fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .map(|path| {
            if path.is_dir() {
                return recursive_rust_source_count(&path, needle);
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
fn named_operation_boundaries_replace_the_ignored_generic_marker() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        recursive_rust_source_count(&source_root, "MayThrowOperation"),
        0,
        "an ignored operation marker must not masquerade as a type proof",
    );

    let operations = normalized(bounded(
        OPERATIONS_SOURCE,
        "    pub(crate) fn compile_spec_operation_to_locals(",
        "    fn emit_spec_operation_abrupt_exit(",
    ));
    assert!(operations.contains("validate_spec_operation_operands(operation,operands.len())?"));
    assert!(operations.contains("SpecOperationIr::ToPrimitive(hint)=>{self.emit_tagged_to_primitive_locals_pending(hint,&inputs[0],&pending,function)?;"));
    assert!(operations.contains("SpecOperationIr::ToNumber=>{self.emit_value_to_number_payload(&inputs[0],&pending,function)?"));
    assert!(operations.contains("SpecOperationIr::GetV|SpecOperationIr::GetMethod=>{self.emit_value_to_object_locals(&inputs[0],&pending,function)?;"));
    assert!(operations.contains("self.emit_dynamic_property_read_with_key_locals(&lookup,&inputs[0],&key,&pending,function,"));
    assert!(operations.contains("SpecOperationResultLocals::new(operation,schema,function)"));
    assert!(operations.contains("result.project(self,output,inputs,function)"));
    let projection = normalized(bounded(
        OPERATIONS_SOURCE,
        "            Self::Expression(pending) => {",
        "            Self::Call(pending) => {",
    ));
    assert!(projection.contains("builder.completion().copy_from(&pending,function)"));
    assert!(projection.contains("output.copy_from(pending.value(),function)"));
    assert!(projection.contains("builder.emit_propagate_current_throw(function)"));
}

#[test]
fn pending_completion_represents_only_to_primitive() {
    // Operation identity belongs to the typed helper boundary; its result is
    // the complete JS completion, rather than a raw payload/tag pair.
    assert!(!OPERATIONS_SOURCE.contains("PendingToPrimitiveCompletion"));
    let fields = normalized(bounded(
        VALUE_SOURCE,
        "pub(crate) struct CompletionLocals {",
        "}\nimpl CompletionLocals",
    ));
    assert_eq!(fields, "value:ValueLocals,kind:I32Local,target:I32Local,");
    let completion = bounded(
        VALUE_SOURCE,
        "impl CompletionLocals {",
        "pub(crate) struct GcCallResult {",
    );
    let copy = normalized(bounded(
        completion,
        "    pub(crate) fn copy_from(&self, source: &Self, function: &mut Function) {",
        "    pub(crate) fn clear(self, function: &mut Function)",
    ));
    assert!(copy.contains("self.value.copy_from(&source.value,function)"));
    assert!(copy.contains("source.kind.load(function);self.kind.store(function)"));
    assert!(copy.contains("source.target.load(function);self.target.store(function)"));
    let helper_result = normalized(bounded(
        HELPER_SOURCE,
        "impl HelperCompletion {",
        "pub(crate) struct I32HelperResult",
    ));
    assert!(
        helper_result.contains("fnstore(self,destination:&CompletionLocals,function:&mutFunction)")
    );
    assert!(helper_result
        .contains(".store_call_result(destination.kind(),destination.target(),function)"));
    let routed = normalized(bounded(
        OPERATIONS_SOURCE,
        "    pub(crate) fn emit_tagged_to_primitive_locals(",
        "    pub(crate) fn emit_tagged_to_primitive_locals_pending(",
    ));
    assert!(routed.contains("self.emit_tagged_to_primitive_locals_pending(hint,input,result,function)?;self.finish_to_primitive_operation(route,result,function)"));
    let finisher = normalized(bounded(
        OPERATIONS_SOURCE,
        "    fn finish_to_primitive_operation(",
        "    pub(crate) fn emit_construct(",
    ));
    assert!(finisher.contains("result:&crate::gc_types::CompletionLocals"));
    assert!(finisher.contains("self.completion().copy_from(result,function)"));
    assert_eq!(finisher.matches("ToPrimitiveAbruptRoute::").count(), 2);
    assert!(!finisher.contains("_=>"));
}

#[test]
fn every_raw_producer_constructs_the_fixed_identity_token() {
    let facade = normalized(bounded(
        OPERATIONS_SOURCE,
        "    pub(crate) fn emit_tagged_to_primitive_locals_pending(",
        "    pub(crate) fn emit_tagged_to_primitive_locals_pending_inner(",
    ));
    let compilers = normalized(bounded(
        HELPER_COMPILERS,
        "    pub(super) fn compile_value_to_primitive_helper(",
        "    pub(super) fn compile_value_to_property_key_helper(",
    ));
    let rows = normalized(HELPER_SOURCE);
    for hint in ["Default", "Number", "String"] {
        assert!(facade.contains(&format!("ToPrimitiveHint::{hint}=>schema.call_helper(crate::runtime_helpers::ValueToPrimitive{hint}Arguments::new(input,self.current_environment(),self.current_program(),)")));
        assert!(compilers.contains(&format!("ToPrimitiveHint::{hint}=>{{letparameters=self.helper_parameters::<crate::runtime_helpers::ValueToPrimitive{hint}Parameters>")));
        assert!(rows.contains(&format!("ValueToPrimitive{hint}/ValueToPrimitive{hint}Arguments/ValueToPrimitive{hint}Parameters/\"value_to_primitive_{}\"{{input:Value,caller_environment:(RefEnvironmentNullable),caller_program:Program}}=>Completion;", hint.to_ascii_lowercase())));
    }
    assert_eq!(facade.matches(".store(result,function)").count(), 3);
    assert!(!facade.contains("_=>"));
    assert!(!facade.contains("emit_object_read_with_throw_routing"));
    assert!(!facade.contains("emit_object_read_kernel"));
    assert!(compilers.contains("self.begin_helper_body(RuntimeHelperId::helper_for(hint))"));
    assert_eq!(
        compilers
            .matches("self.emit_tagged_to_primitive_locals_pending_inner(")
            .count(),
        3
    );
    assert_eq!(compilers.matches("result.emit(&mutfunction)").count(), 3);
    assert_eq!(
        compilers
            .matches("parameters.release(&mutfunction)")
            .count(),
        3
    );
    assert!(!compilers.contains("self.emit_tagged_to_primitive_locals_pending("));
    assert!(!facade.contains("payload_local"));
    assert!(!facade.contains("tag_local"));

    for evidence in [CONTRACT] {
        assert!(evidence.contains("operation boundaries now own identity"));
        assert!(evidence.contains("ToPrimitive"));
        assert!(evidence.contains("unrepresentable"));
    }
}
