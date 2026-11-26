use scriptw_ir::{
    find_spec_operation, AbruptCapability, BackendSpecOperation, CompletionAbruptKind,
    NormalResult, OperationDomain, OperationLoweringStatus, RowSource, SpecOperationFamily,
    TrackedGapReason, SPEC_OPERATION_CATALOG,
};

const CALLBACK_ITERATION_SOURCE: &str =
    include_str!("../../src/builtins/array/callback_iteration.rs");
const FLAT_SOURCE: &str = include_str!("../../src/builtins/array/flat.rs");
const ARRAY_SOURCE: &str = include_str!("../../src/builtins/array.rs");
const BACKEND_JOIN_SOURCE: &str = include_str!("../../src/backend_operation_evidence.rs");
const BACKEND_LIB_SOURCE: &str = include_str!("../../src/lib.rs");
const OPERATIONS_SOURCE: &str = include_str!("../../../scriptw-ir/src/operations.rs");
const TYPED_ARRAY_SPECIES_SOURCE: &str =
    include_str!("../../src/objects/typed_array_species_create.rs");
const SUBARRAY_SOURCE: &str = include_str!("../../src/builtins/binary_data/typed_array.rs");
const STANDARD_SOURCE: &str = include_str!("../../src/builtins/standard.rs");

const SPECIES_READ: &str = "WellKnownSymbol::Species";

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

#[test]
fn array_species_create_has_backend_owned_object_result_evidence() {
    let operation = BackendSpecOperation::ArraySpeciesCreate;
    let descriptor = operation.descriptor();
    assert_eq!(descriptor.name(), "ArraySpeciesCreate");
    assert_eq!(descriptor.family(), SpecOperationFamily::Invocation);
    assert_eq!(descriptor.domain(), OperationDomain::ValueAndInteger);
    assert_eq!(descriptor.normal_result(), NormalResult::Object);
    assert_eq!(descriptor.abrupt(), AbruptCapability::MayThrow);

    let row = find_spec_operation("ArraySpeciesCreate").expect("ArraySpeciesCreate catalog row");
    assert_eq!(row.source(), RowSource::DerivedFromBackendOperation);
    assert_eq!(row.normal_result(), NormalResult::Object);
    assert_eq!(row.abrupt(), &[CompletionAbruptKind::Throw]);
    assert!(matches!(
        row.lowering_status(),
        OperationLoweringStatus::SharedBackendEmitter(evidence)
            if evidence.operation() == operation
    ));

    let gaps = bounded(
        OPERATIONS_SOURCE,
        "pub(crate) const TRACKED_GAP_ROWS: &[TrackedGapRow] = &[",
        "pub const SPEC_OPERATION_ROW_COUNT",
    );
    assert!(!gaps.contains("name: \"ArraySpeciesCreate\""));

    let species_constructor =
        find_spec_operation("SpeciesConstructor").expect("SpeciesConstructor catalog row");
    assert_eq!(species_constructor.source(), RowSource::TrackedGapTable);
    assert!(matches!(
        species_constructor.lowering_status(),
        OperationLoweringStatus::TrackedGap {
            reason: TrackedGapReason::NoImplementation,
            ..
        }
    ));
    assert!(gaps.contains("name: \"SpeciesConstructor\""));
}

#[test]
fn operation_catalog_census_separates_backend_evidence_from_other_rows() {
    let mut expression_emitters = 0;
    let mut backend_emitters = 0;
    let mut statement_emitters = 0;
    let mut tracked_gaps = 0;

    for row in &SPEC_OPERATION_CATALOG {
        match row.lowering_status() {
            OperationLoweringStatus::SharedWasmEmitter(_) => expression_emitters += 1,
            OperationLoweringStatus::SharedBackendEmitter(_) => backend_emitters += 1,
            OperationLoweringStatus::StatementEmission(_) => statement_emitters += 1,
            OperationLoweringStatus::TrackedGap { .. } => tracked_gaps += 1,
        }
    }

    assert_eq!(BackendSpecOperation::ALL.len(), 2);
    assert_eq!(
        (
            expression_emitters,
            backend_emitters,
            statement_emitters,
            tracked_gaps,
            SPEC_OPERATION_CATALOG.len(),
        ),
        (32, 2, 5, 10, 49)
    );
}

#[test]
fn backend_operation_join_is_exhaustive_and_names_the_real_emitter() {
    assert!(BACKEND_LIB_SOURCE.contains("mod backend_operation_evidence;"));
    assert!(BACKEND_JOIN_SOURCE
        .contains("fn backend_spec_operations_are_backed(operation: BackendSpecOperation)"));
    assert!(BACKEND_JOIN_SOURCE.contains("match operation {"));
    assert_eq!(
        BACKEND_JOIN_SOURCE
            .matches("BackendSpecOperation::ArraySpeciesCreate")
            .count(),
        1
    );
    assert_eq!(
        BACKEND_JOIN_SOURCE
            .matches("FunctionBuilder::emit_array_species_create")
            .count(),
        1
    );
    assert!(!BACKEND_JOIN_SOURCE.contains("_ =>"));
}

#[test]
fn array_species_create_emitter_has_all_seven_array_callers() {
    assert_eq!(
        ARRAY_SOURCE
            .matches("pub(crate) fn emit_array_species_create(")
            .count(),
        1
    );
    assert_eq!(
        ARRAY_SOURCE
            .matches("pub(super) fn emit_array_species_create_with_default_flag(")
            .count(),
        1
    );
    assert_eq!(
        ARRAY_SOURCE.matches("emit_array_species_create(").count(),
        3
    );
    assert_eq!(
        ARRAY_SOURCE
            .matches("self.emit_array_species_create_with_default_flag(")
            .count(),
        2
    );

    assert_eq!(
        FLAT_SOURCE
            .matches("self.emit_array_species_create(")
            .count(),
        1
    );

    assert_eq!(
        CALLBACK_ITERATION_SOURCE
            .matches("self.emit_array_species_create_with_default_flag(")
            .count(),
        2
    );
    for kind in ["Map", "Filter"] {
        assert!(CALLBACK_ITERATION_SOURCE.contains(&format!(
            "ArrayCallbackIterationKind::{kind} => self.emit_array_species_create_with_default_flag("
        )));
    }
    assert_eq!(CALLBACK_ITERATION_SOURCE.matches(SPECIES_READ).count(), 0);
    // Flat and flatMap now share one physical FlattenIntoArray owner.
    for kind in ["Flat", "FlatMap"] {
        assert!(FLAT_SOURCE.contains(&format!(
            "self.emit_array_flatten_builtin(FlattenMethod::{kind}, f)"
        )));
    }
    let flatten = bounded(
        FLAT_SOURCE,
        "    fn emit_array_flatten_builtin(",
        "\n    }\n}",
    );
    assert_eq!(
        flatten.matches("self.emit_array_species_create(").count(),
        1
    );
    let slice = bounded(
        ARRAY_SOURCE,
        "    pub(crate) fn compile_array_prototype_slice_builtin(",
        "    pub(crate) fn compile_typed_array_prototype_slice_builtin(",
    );
    let splice = bounded(
        ARRAY_SOURCE,
        "    fn compile_array_splice_output(",
        "    pub(crate) fn compile_array_prototype_join_builtin(",
    );
    assert_eq!(
        slice
            .matches("self.emit_array_species_create_with_default_flag(")
            .count(),
        1
    );
    assert_eq!(splice.matches("self.emit_array_species_create(").count(), 1);
    assert!(splice.contains(
        "ArraySpliceOutput::Receiver => {\n                self.emit_array_species_create("
    ));
    assert!(
        ARRAY_SOURCE.contains("self.compile_array_splice_output(ArraySpliceOutput::Receiver, f)")
    );
}

#[test]
fn symbol_species_reads_have_exact_array_and_typed_array_owners() {
    assert_eq!(ARRAY_SOURCE.matches(SPECIES_READ).count(), 1);
    assert_eq!(TYPED_ARRAY_SPECIES_SOURCE.matches(SPECIES_READ).count(), 1);
    assert_eq!(SUBARRAY_SOURCE.matches(SPECIES_READ).count(), 1);

    assert_eq!(FLAT_SOURCE.matches(SPECIES_READ).count(), 0);
    assert_eq!(
        FLAT_SOURCE
            .matches("self.emit_array_species_create(")
            .count(),
        1
    );
    let migrated_array_callers = [(
        "    pub(crate) fn compile_array_prototype_concat_builtin(",
        "    pub(crate) fn compile_array_prototype_splice_builtin(",
    )];
    for (start, end) in migrated_array_callers {
        let caller = bounded(ARRAY_SOURCE, start, end);
        assert_eq!(caller.matches(SPECIES_READ).count(), 0);
        assert_eq!(caller.matches("self.emit_array_species_create(").count(), 1);
    }

    let typed_array_copies = [
        (
            "    pub(crate) fn compile_typed_array_prototype_slice_builtin(",
            "    pub(super) fn compile_array_prototype_for_each_builtin(",
        ),
        (
            "    pub(crate) fn compile_typed_array_prototype_map_builtin(",
            "    pub(crate) fn compile_typed_array_prototype_filter_builtin(",
        ),
        (
            "    pub(crate) fn compile_typed_array_prototype_filter_builtin(",
            "    pub(crate) fn compile_typed_array_prototype_every_builtin(",
        ),
    ];
    assert_eq!(typed_array_copies.len(), 3);
    for (start, end) in typed_array_copies {
        let caller = bounded(ARRAY_SOURCE, start, end);
        assert_eq!(caller.matches(SPECIES_READ).count(), 0);
        assert_eq!(
            caller
                .matches("self.emit_typed_array_species_create_with_length(")
                .count(),
            1
        );
    }
    let subarray = bounded(
        STANDARD_SOURCE,
        "StandardBuiltinId::TypedArrayPrototypeSubarray => {",
        "StandardBuiltinId::DateNow => {",
    );
    assert_eq!(subarray.matches(SPECIES_READ).count(), 0);
    assert_eq!(
        subarray
            .matches("self.emit_typed_array_subarray_builtin(function)?;")
            .count(),
        1
    );
    let subarray_owner = bounded(
        SUBARRAY_SOURCE,
        "fn emit_typed_array_subarray_builtin(",
        "\n    }",
    );
    assert_eq!(subarray_owner.matches(SPECIES_READ).count(), 1);
    assert_eq!(
        subarray_owner
            .matches("self.emit_binary_construct_typed_array(")
            .count(),
        2
    );
    assert!(subarray_owner.contains("&[&buffer, &offset_arg]"));
    assert!(subarray_owner.contains("&[&buffer, &offset_arg, &count_arg]"));

    let array_quantifiers = [
        (
            "    pub(crate) fn compile_array_prototype_every_builtin(",
            "    pub(crate) fn compile_array_prototype_some_builtin(",
        ),
        (
            "    pub(crate) fn compile_array_prototype_some_builtin(",
            "    pub(crate) fn compile_array_prototype_filter_builtin(",
        ),
    ];
    for (start, end) in array_quantifiers {
        let body = bounded(ARRAY_SOURCE, start, end);
        assert_eq!(body.matches(SPECIES_READ).count(), 0);
    }

    let array_species_create_emitter = bounded(
        ARRAY_SOURCE,
        "    pub(crate) fn emit_array_species_create(",
        "    fn emit_array_literal_object(",
    );
    assert_eq!(
        array_species_create_emitter.matches(SPECIES_READ).count(),
        1
    );
}
