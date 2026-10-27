use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use scriptw_ir::{
    private_brand_key, private_data_key, AnnexBFunctionCopyTargetIr, ArithmeticBinaryOp,
    ArrayDestructuringElementIr, ArrayDestructuringPatternIr, BigIntBitwiseOp, BindingMode,
    BitwiseBinaryOp, BlockIr, CallableToStringRepresentation, CapturedOrdinaryPropertyWriteIr,
    ClassDefinitionIr, ClassElementDefinitionIr, ClassElementExecutionKind, ClassFieldKeyIr,
    ClassFieldNameIr, ClassFunctionKind, ClassHeritageKind, ClassInstanceElementIr,
    ClassInstanceElementPlanIr, ClassMethodPlacementIr, ClassNameInferenceIr, ClassStaticElementIr,
    DestructuringPropertyKeyIr, DestructuringTargetIr, DynamicFunctionKind, DynamicSourceIntrinsic,
    EqualityBinaryOp, ExprIr, ForInOfEnvironmentIr, ForInitIr, ForLexicalEnvironmentIr,
    ForOfIteratorHeadIr, FunctionExecutionKind, FunctionFlavor, FunctionId, FunctionIr,
    FunctionParamIr, FunctionProtocolIr, GeneratorResumeModeIr, GeneratorTryPlanIr,
    GlobalBindingPlan, GlobalPropertyInitializerIr, HeapShape, HostBuiltinId,
    IdentifierWriteDisposition, KindSet, LexicalEnvironmentIr, LogicalBinaryOp, NativeErrorKind,
    NumericUpdateOp, NumericUpdateValueKind, ObjectPropertyIr, ObjectShapeProperty,
    OrdinaryPropertyAssignmentIr, OrdinaryPropertyEagerCompoundAssignmentIr,
    OrdinaryPropertyGetCaptureIr, OrdinaryPropertyLogicalAssignmentIr,
    OrdinaryPropertyNumericUpdateIr, OwnedEnvBindingIr, PrivateNameId, PropertyKeyIr,
    RelationalBinaryOp, ScriptIr, SpecOperationIr, SpreadArgumentIr, StandardBuiltinId,
    StatementIr, Strictness, SuspendedPropertyReferenceIr, SuspendedPropertyReferenceUse,
    SwitchCaseIr, SyncDisposableResourcesIr, ToPrimitiveHint, TypedExpr, UnaryBitwiseOp,
    UpdateReturnMode, ValueInfo, ValueKind, VarDeclaratorIr, WellKnownSymbol, YieldForm,
    AGGREGATE_ERROR_NAME, ARRAY_BUFFER_NAME, ARRAY_NAME, ATOMICS_NAME, BOOLEAN_NAME,
    DATA_VIEW_NAME, DATE_NAME, ERROR_NAME, EVAL_ERROR_NAME, FLOAT16_ARRAY_NAME, FLOAT32_ARRAY_NAME,
    FLOAT64_ARRAY_NAME, FUNCTION_NAME, GLOBAL_THIS_NAME, HOST_PARSE_FLOAT_FUNCTION_ID,
    INT16_ARRAY_NAME, INT32_ARRAY_NAME, INT8_ARRAY_NAME, INTL_NAMESPACE_CONSTRUCTORS,
    INTL_NAMESPACE_METHODS, IS_CONSTRUCTOR_NAME, JSON_NAME, JS_STRING_SURROGATE_SENTINEL,
    LEXICAL_ARGUMENTS_NAME, LEXICAL_HOME_OBJECT_NAME, LEXICAL_NEW_TARGET_NAME, LEXICAL_THIS_NAME,
    MAP_NAME, MATH_NAME, NUMBER_NAME, OBJECT_NAME, PRINT_NAME, PROMISE_NAME, PROXY_NAME,
    RANGE_ERROR_NAME, REFERENCE_ERROR_NAME, REFLECT_NAME, REGEXP_NAME,
    SCRIPTW_GENERATOR_THROW_SLOT, SET_NAME, SHARED_ARRAY_BUFFER_NAME, STRING_NAME,
    SUPPRESSED_ERROR_NAME, SYMBOL_NAME, SYNTAX_ERROR_NAME, TEMPORAL_NAMESPACE_CONSTRUCTORS,
    TEMPORAL_NOW_NAME, TEMPORAL_NOW_NAMESPACE_MEMBERS, TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_METHODS,
    TYPE_ERROR_NAME, UINT16_ARRAY_NAME, UINT32_ARRAY_NAME, UINT8_ARRAY_NAME,
    UINT8_CLAMPED_ARRAY_NAME, URI_ERROR_NAME,
};
use scriptw_ir::{
    FunctionTargetKnowledge, PreparedScript, PreparedScriptKind, PreparedScriptOutcome,
    PreparedScriptUnit,
};
use scriptw_ir::{SuperPropertyMutationIr, SuperPropertyMutationOperationIr};
// `Function` is deliberately absent from this list. The name is bound below to
// `code_sink::Function`, the wrapper that counts real Wasm label depth, and
// every submodule of this crate reaches `Function` through this one binding
// (`use super::*` / `use super::super::*`). That is what lets ~600 `&mut
// Function` signatures and ~77,000 `function.instruction(..)` calls keep their
// exact text while the branch arithmetic underneath them becomes correct.
// See `code_sink.rs`.
use wasm_encoder::{BlockType, Ieee64, Instruction, MemArg, ValType};

mod abi;
mod arguments_protocol;
mod backend_operation_evidence;
mod bigint;
mod builtins;
mod call_rows;
mod code_sink;
mod control_flow;
mod data;
mod embedding_bridge;
mod emission_sites;
mod emit;
pub use embedding_bridge::{
    EMBEDDING_BYTES, EMBEDDING_BYTES_READ, EMBEDDING_CALL, EMBEDDING_CANCEL, EMBEDDING_CHECKPOINT,
    EMBEDDING_EXPORT, EMBEDDING_PROMISE, EMBEDDING_READ, EMBEDDING_STRING,
};
mod emitted_function;
mod environments;
mod expressions;
mod function_entry;
mod function_layout;
mod functions;
mod gc_types;
pub use gc_types::{
    check_gc_main_completion, snapshot_array_storage_mode, snapshot_descriptor_flags,
    snapshot_function_protocol, snapshot_intrinsic_index, snapshot_symbol_index,
    ArrayIndexedStorageMode, GcHostField, GcHostFieldDefinition, GcHostImport, GcHostLayout,
    GcHostStorage, GcHostValue, GcHostValueStorage, GcSnapshotField, GcSnapshotLayout,
    SNAPSHOT_ENTRY_REALM_EXPORT, SNAPSHOT_REALMS_EXPORT, SNAPSHOT_RESOLVE_PROPERTY_VALUE_EXPORT,
    SNAPSHOT_SYMBOLS_EXPORT,
};
mod generator_delegation;
mod generator_reference;
mod heap;
mod intrinsics;
mod module;
pub use module::WasmArtifact;
mod module_entry_completion;
pub use module_entry_completion::{WasmModuleEvaluationStatus, MODULE_EVALUATION_STATUS_EXPORT};
mod modules;
mod objects;
mod operations;
mod planning;
mod prepared_script;
mod program_hooks;
mod promise_rejection_policy;
mod runtime_abi;
mod runtime_artifact;

/// Program-local execution phase; this never extends the shared runtime layout.
pub const GLOBAL_SCRIPT_PRELUDE_PHASE_EXPORT: &str = "scriptw.global_prelude_phase";

pub use runtime_artifact::{
    admit_portable_runtime_package, build_runtime_artifact,
    build_runtime_artifact_with_source_compilation_profile, encode_portable_runtime_package,
    runtime_artifact, runtime_artifact_with_cache, runtime_artifact_with_inputs, RuntimeArtifact,
    RuntimeArtifactCache, RuntimeArtifactCacheKey, RuntimeArtifactInputs, RuntimeArtifactKey,
    RuntimeBuildArtifact, RuntimeSourceCompilationProfile, ValidatedRuntimeArtifact,
    NATIVE_REGEXP_LITERAL_FACTORY_EXPORT, RUNTIME_IMPORT_NAMESPACE,
    RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION,
};
mod runtime_direct_eval;
mod runtime_function;
mod runtime_global_script;
pub use runtime_direct_eval::{
    emit_runtime_direct_eval, emit_runtime_direct_eval_with_validated_runtime,
    RuntimeDirectEvalArtifact,
};
pub use runtime_function::{
    emit_runtime_function, emit_runtime_function_with_validated_runtime, RuntimeFunctionArtifact,
};
pub use runtime_global_script::{
    emit_runtime_global_script, emit_runtime_global_script_with_validated_runtime,
    RuntimeGlobalScriptArtifact,
};
mod runtime_helpers;
use abi::*;
use arguments_protocol::*;
use bigint::BigIntHelperOp;
use builtins::*;
use code_sink::{Function, LabelDepth, LocalDeclarations};
use data::*;
pub use emit::emit;
pub use emit::emit_with_intl_profile;
pub use emit::emit_with_promise_rejection_policy;
pub use emit::emit_with_rooted_snapshot;
pub use emit::{
    emit_with_intl_profile_and_runtime_cache, emit_with_intl_profile_and_runtime_inputs,
};
pub use emit::{
    emit_with_rooted_snapshot_and_runtime_cache, emit_with_rooted_snapshot_and_runtime_inputs,
};
pub(crate) use emit::{
    AccessorThrowRouting, BindingStorage, CompletionKind, ControlFrameKind, ControlTarget,
    FunctionBuilder, LabelTargets, LoopTargets, PropagateCallThrow, ReturnAbi,
};
pub use promise_rejection_policy::PromiseRejectionPolicy;
// `FunctionBodySize` and `FunctionLocalCount` are part of the public face
// because `EmittedFunctionSummary` carries them: a `pub` struct whose fields
// name crate-private types is a `private_interfaces` warning, and flattening
// them back to `u32` at the boundary would give the two figures the same type
// again — which is exactly what this module's newtypes exist to prevent.
pub(crate) use emitted_function::{
    emit_size_report_requested, write_size_report_file_if_requested, EmittedFunction,
    FunctionBodyBudget, FunctionIdentity, ModuleCode,
};
pub use emitted_function::{EmittedFunctionSummary, FunctionBodySize, FunctionLocalCount};
use function_layout::{FunctionIndexLayout, SourceFunctions};
mod foreign;
pub use foreign::{foreign_registry_from_module, FOREIGN_MODULES_CUSTOM_SECTION};
pub(crate) use functions::NonArrayRealmIntrinsicSlot;
use heap::*;
use intrinsics::*;
use module::*;
use modules::module_unit_guard_count;
pub(crate) use operations::ToPrimitiveAbruptRoute;
use planning::*;
use program_hooks::ProvidedHooks;
pub use runtime_abi::WasmRuntimeValueTag;
pub(crate) use runtime_helpers::{RuntimeHelperEmission, RuntimeHelperFact, RuntimeHelperId};

fn read_static_heap_shape_property(shape: &HeapShape, key: &str) -> Option<ObjectShapeProperty> {
    match shape {
        HeapShape::Object(object) => object.properties.get(key).cloned().or_else(|| {
            object
                .prototype
                .as_deref()
                .and_then(|prototype| read_static_heap_shape_property(prototype, key))
        }),
        HeapShape::Array(array) => array.properties.get(key).cloned().or_else(|| {
            array
                .prototype
                .as_deref()
                .and_then(|prototype| read_static_heap_shape_property(prototype, key))
        }),
    }
}

static EMPTY_BLOCK: LazyLock<BlockIr> = LazyLock::new(|| BlockIr {
    statements: Vec::new(),
    result_kind: ValueKind::Undefined,
    lexical_environment: None,
});

#[cfg(test)]
mod tests;

mod native_program_contexts;
mod native_program_template;
mod program_imports;
mod program_literal_operands;
pub use native_program_template::{
    NativeProgramArrayConstruction, NativeProgramArrayFactory, NativeProgramArrayWidth,
    NativeProgramDataImage, NativeProgramLiteralOperand, NativeProgramPayload,
    NativeProgramPayloadDirectory, NativeProgramTemplate, NativeProgramTemplateError,
    NATIVE_PROGRAM_DATA_MODULE,
};
