use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::ControlFlow;

use boa_ast::operations::{
    annex_b_function_declarations, contains, lexically_declared_names, ContainsSymbol,
};
use boa_ast::property::{MethodDefinitionKind, PropertyName};
#[cfg(test)]
use boa_ast::scope::Scope;
use boa_ast::visitor::{VisitWith, Visitor};
use boa_ast::{
    declaration::{Binding, ExportDeclaration, LexicalDeclaration, VarDeclaration, Variable},
    expression::access::{
        PrivatePropertyAccess, PropertyAccess, PropertyAccessField, SuperPropertyAccess,
    },
    expression::literal::{
        ArrayLiteral, LiteralKind, ObjectLiteral, ObjectMethodDefinition, PropertyDefinition,
        TemplateElement, TemplateLiteral,
    },
    expression::operator::{
        assign::{AssignOp, AssignTarget},
        binary::{ArithmeticOp, BinaryInPrivate, BinaryOp, BitwiseOp, LogicalOp, RelationalOp},
        unary::UnaryOp,
        update::{UpdateOp, UpdateTarget},
    },
    expression::New,
    expression::{
        Call, Expression, Optional, OptionalOperationKind, RegExpLiteral, SuperCall, TaggedTemplate,
    },
    function::{
        ArrowFunction, AsyncArrowFunction, AsyncFunctionDeclaration, AsyncFunctionExpression,
        AsyncGeneratorDeclaration, AsyncGeneratorExpression, ClassDeclaration, ClassElement,
        ClassElementName, ClassExpression, ClassMethodDefinition, FormalParameter,
        FormalParameterList, FunctionBody, FunctionDeclaration, FunctionExpression, PrivateName,
        StaticBlockBody,
    },
    function::{GeneratorDeclaration, GeneratorExpression},
    pattern::{ArrayPatternElement, ObjectPatternElement, Pattern},
    statement::{
        iteration::{
            Break as AstBreak, Continue as AstContinue, DoWhileLoop, ForLoop, ForLoopInitializer,
            ForOfLoop, IterableLoopInitializer, WhileLoop,
        },
        Block, If, Labelled as AstLabelled, LabelledItem, Return as AstReturn, Statement,
        Switch as AstSwitch, Throw as AstThrow, Try as AstTry,
    },
    Declaration, ModuleItem, Script, Spanned, StatementListItem,
};
use boa_interner::Interner;
#[cfg(test)]
use boa_parser::{Parser, Source};
use num_bigint::{BigInt, BigUint, Sign};
use num_traits::ToPrimitive;
use scriptw_front::{ParseGoal, ParsedModule, ParsedScript, ParsedSource, SourceUnit};

mod analysis;
pub use analysis::{ProvenBindingCellIr, ProvenBindingCellOrigin};
pub mod foreign;
mod language_features;
pub use language_features::{LanguageFeaturePolicy, PrivateElementExtensibility};
mod async_array_destructuring;
mod async_generator_array_destructuring;
mod async_pattern_source;
pub use async_generator_array_destructuring::AsyncGeneratorArrayDestructuringIr;
mod async_generator_for_in;
mod async_generator_for_of;
pub use async_generator_for_of::{AsyncGeneratorForOfIr, AsyncGeneratorIteratorProtocolIr};
mod async_generator_loop_control;
mod resumable_region;
pub use resumable_region::{ResumableExpressionIr, ResumableRegionIr, ResumableRegionProtocolIr};
mod async_generator_resource;
pub use async_generator_resource::{
    AsyncGeneratorForOfResourceIr, AsyncGeneratorResourceCapabilityIr,
    AsyncGeneratorResourceRegistrationIr, AsyncGeneratorResourceScopeIr,
    AsyncGeneratorScopedResourceIr, ResourceDisposalHintIr,
};
mod async_generator_source;
mod async_generator_switch;
mod async_generator_with;
mod async_with;
mod async_with_source;
mod for_in_storage;
pub use async_with::AsyncFunctionWithIr;
mod async_for_of_body;
mod for_in_initialization;
pub use async_array_destructuring::AsyncFunctionArrayDestructuringIr;
mod async_if;
mod async_labelled;
mod async_switch;
mod async_while;
mod generator_array_destructuring;
mod generator_for_of_body;
mod generator_for_of_iterator;
mod generator_loop_control;
mod generator_loop_source;
mod generator_switch;
mod generator_value_branch_source;
mod generator_with;
pub use generator_array_destructuring::OrdinaryGeneratorArrayDestructuringIr;
pub use generator_with::OrdinaryGeneratorWithIr;
mod resumable_for_of_control;
mod synchronous_loop_body;
pub use synchronous_loop_body::{SynchronousLoopBodyError, SynchronousLoopBodyIr};
/// Environment Record binding lifecycle (ECMA-262 9.1.1.1): the `Initialization`
/// state that lives on `BindingInfo`, and the `$tdz.` name domain. See
/// `docs/rust-rewrite/contracts/environment-record-tdz.md`.
mod binding_lifecycle;
mod binding_names;
mod builtins;
mod captured_arguments;
mod prepared_super_construct;
pub use prepared_super_construct::PreparedSuperConstructIr;
mod class_name_source;
pub use captured_arguments::{
    ArgumentListCaptureIr, CapturedArgumentListIr, CapturedCallReceiverIr,
    OptionalCallReferenceCaptureIr,
};
pub(crate) use class_name_source::SourceClassName;
mod diagnostics;
mod dynamic_source;
mod runtime_semantics;
// The `EarlyErrorCode` -> rejection-stage map. UNRELATED to `early_errors`
// below, despite the adjacency: this is the diagnostic taxonomy, that is
// derived-constructor validation over `ExprIr` arms.
mod array_destructuring_operation;
mod delete_optional_property_chain;
mod direct_eval_context;
mod import_call_source_origin;
pub use import_call_source_origin::ImportCallSourceOriginIr;
mod environment_identifier;
mod object_destructuring_operation;
pub use array_destructuring_operation::{
    ArrayDestructuringOperationIr, ArrayDestructuringOperationKindIr,
    ArrayDestructuringOperationView, ArrayIteratorStorageIr,
};
mod object_property_definition;
pub use delete_optional_property_chain::{
    DeleteOptionalPropertyChainIr, InvalidDeleteOptionalPropertyChainIr,
};
pub use environment_identifier::{
    CapturedIdentifierReferenceIr, EnvironmentCompoundOperationIr, EnvironmentIdentifierIr,
    EnvironmentIdentifierOperationIr, EnvironmentIdentifierResolutionStart,
    IdentifierReferenceCaptureAccess, IdentifierReferenceCaptureDisposition,
    IdentifierReferenceCaptureIr, IdentifierReferenceFallbackDisposition,
};
pub use object_destructuring_operation::{
    ObjectDestructuringKeyIr, ObjectDestructuringOperationIr, ObjectDestructuringOperationView,
    ObjectDestructuringSourceIr,
};
pub use object_property_definition::{ObjectPropertyDefinitionError, ObjectPropertyDefinitionIr};
mod early_error_code;
mod early_errors;
mod eval_environment;
pub use direct_eval_context::{DirectEvalContextIr, DirectEvalDerivedOwner};
mod function_protocol;
mod ir;
mod iterator_obligations;
mod lowering;
mod lowering_helpers;
mod modules;
mod names;
mod native_error;
mod operations;
mod prepared_function;
mod runtime_direct_eval;
mod runtime_global_script;
pub use runtime_direct_eval::{
    lower_runtime_direct_eval, EmptyRuntimeDirectEval, PrimitiveRuntimeDirectEval,
    RuntimeDirectEvalCaller, RuntimeDirectEvalCallerDescriptor, RuntimeDirectEvalCallerError,
    RuntimeDirectEvalCompileError, RuntimeDirectEvalDerivedOwner, RuntimeDirectEvalImportOrigin,
    RuntimeDirectEvalPrimitive, RuntimeDirectEvalPrivateScope, RuntimeDirectEvalProgram,
    StaticRegExpRuntimeDirectEval,
};
pub use runtime_global_script::{
    lower_runtime_global_script, RuntimeGlobalScriptCompileError, RuntimeGlobalScriptKind,
    RuntimeGlobalScriptProgram,
};
mod runtime_function;
pub use runtime_function::{
    lower_runtime_function, RuntimeFunctionCompileError, RuntimeFunctionProgram,
};
mod robustness;
pub use eval_environment::{
    EvalBindingDeclarationIr, EvalDeclarativeEnvironmentKindIr, EvalEnvironmentRoleIr,
    EvalVisibleBindingIr,
};
mod ownership;
mod prepared_script;
mod prepared_source_cache;
/// The Property Descriptor lattice (ECMA-262 6.2.6). See
/// `docs/rust-rewrite/contracts/property-descriptor-lattice.md`.
pub mod property_descriptor;
mod regexp;
mod regexp_unicode17;
mod source_call_flow_proof;
mod switch_storage;
mod template_site;
pub use template_site::{TemplateSiteId, TemplateSourceId, TemplateSourceIr};
mod well_known;
mod with_object_environment;
pub(crate) use analysis::*;
pub(crate) use async_for_of_body::AsyncFunctionForOfBodyError;
pub use async_for_of_body::AsyncFunctionForOfBodyIr;
pub use async_generator_for_in::AsyncGeneratorForInIr;
pub use async_generator_loop_control::{
    AsyncGeneratorIfIr, AsyncGeneratorLoopExpressionIr, AsyncGeneratorLoopIr,
    AsyncGeneratorLoopRegionIr,
};
pub use async_generator_switch::{AsyncGeneratorSwitchCaseIr, AsyncGeneratorSwitchIr};
pub use async_generator_with::AsyncGeneratorWithIr;
pub use async_if::AsyncFunctionIfPlanIr;
pub use async_labelled::AsyncFunctionLabelledPlanIr;
pub use async_switch::{
    AsyncFunctionSwitchCaseIr, AsyncFunctionSwitchIr, AsyncFunctionSwitchSelectorContinuationIr,
    AsyncFunctionSwitchSelectorIr,
};
pub use async_while::AsyncFunctionWhileConditionIr;
pub(crate) use binding_lifecycle::*;
pub use builtins::{
    CallableToStringRepresentation, HostBuiltinExposure, HostBuiltinId, HostBuiltinSurface,
    HostSurfacePolicy, StandardBuiltinId, StandardBuiltinInstaller,
};
pub use diagnostics::{
    IrDiagnostic, IrDiagnosticKind, IrDiagnosticPhase, LoweringStage, UnsupportedFeature,
};
pub use dynamic_source::{
    DynamicFunctionKind, DynamicSourceGap, DynamicSourceIntrinsic, DynamicSourceKind,
    DynamicSourceRequirement, DynamicSourceRuntimeOperation,
};
pub(crate) use early_errors::validate_derived_constructor_body;
pub use function_protocol::FunctionProtocolIr;
pub(crate) use function_protocol::LexicalSuperOwnerRole;
pub use generator_for_of_body::GeneratorForOfBodyIr;
pub use generator_for_of_iterator::GeneratorForOfIteratorPlanIr;
pub use generator_loop_control::{
    GeneratorLoopExpressionIr, GeneratorLoopKindIr, GeneratorLoopRegionIr, OrdinaryGeneratorIfIr,
    OrdinaryGeneratorLoopIr,
};
pub use generator_switch::{
    EmptyStatementCompletionIr, OrdinaryGeneratorSwitchCaseIr, OrdinaryGeneratorSwitchIr,
};
pub use ir::*;
pub(crate) use ir::{read_heap_shape_property, summarize_block};
/// The iterator-protocol obligations of 7.4 and the witness a for-of
/// specialization carries. See
/// `docs/rust-rewrite/contracts/iterator-protocol.md`.
pub use iterator_obligations::{
    ArrayPatternProtocol, ArraySpreadProtocol, EmissionSite, GeneratorDelegationProtocol,
    GetIteratorDischarge, IntactnessPremise, IteratorCloseDischarge, IteratorObligation,
    IteratorProtocolWitness, IteratorStepDischarge, IteratorValueDischarge, ObligationDischarge,
    PremiseKind, SpreadArgumentProtocol,
};
pub use lowering::{lower, lower_complete_module_catalog, lower_module_graph, lower_script_graph};
pub use lowering::{
    lower_module_graph_with_host_surface_policy, lower_module_graph_with_prelude,
    lower_script_graph_with_host_surface_policy, lower_with_host_surface_policy,
};
pub(crate) use lowering_helpers::*;
pub use modules::{
    classify_outer_script_module_dependency, evaluation_components, parse_module_record,
    scan_module_requests, scan_script_module_requests, scan_script_realm_module_requests,
    script_has_dynamic_import, source_writes_dynamic_import, DuplicateImportAttributeKeyIr,
    DynamicComponentIr, DynamicImportAttributesIr, DynamicImportSiteIr, ImportAttributeIr,
    ImportEntryIr, ImportNameIr, ImportPhaseIr, IndirectExportEntryIr, LinkedProgram,
    LocalExportEntryIr, ModuleBindingKindIr, ModuleBindingNameIr, ModuleEnvBindingIr,
    ModuleEvaluationModeIr, ModuleGraphIr, ModuleGraphSources, ModuleKey, ModuleKindIr,
    ModuleLinkErrorIr, ModuleNamespaceExportIr, ModuleNamespaceIr, ModuleRecordIr, ModuleRecordKey,
    ModuleRequestAttributesIr, ModuleRequestIr, ModuleRequestKeyIr, ModuleSourceIr, ModuleUnitId,
    ModuleUnitIr, OuterScriptModuleDependency, RealmModuleResolutionIr, ResolvedBindingIr,
    StarExportEntryIr, ANONYMOUS_MODULE_KEY,
};
pub use modules::{
    BytesModuleValueIr, DeferredModuleEvaluationIr, JsonModuleValueIr, JsonValue,
    ModuleActivationIr, ModuleActivationKindIr, ModuleCellIr, ModuleEntryEvaluationIr,
    ModuleEntryEvaluationKindIr, ModuleEvaluationIr, ModuleExecutionGraphIr,
    ModuleExecutionRequestIr, ModuleImportBindingIr, ModuleRequestPhaseIr, ModuleSourceRecordIr,
};
pub use operations::{
    completion_abi_slot, completion_abi_slots, find_spec_operation, spec_operation_catalog,
    AbruptCapability, ArithmeticBinaryOp, BackendEmitterEvidence, BackendSpecOperation,
    BigIntBitwiseOp, BindingMode, BitwiseBinaryOp, CompletionAbiSlot, CompletionAbruptKind,
    CompletionKindIr, CompletionRecordIr, DoneSlot, EcmaLanguageType, EmitterEvidence,
    EqualityBinaryOp, IteratorRecordIr, IteratorSlot, LogicalBinaryOp, NextMethodSlot,
    NormalResult, NumericUpdateOp, NumericUpdateValueKind, OperationDescriptor, OperationDomain,
    OperationLoweringStatus, RelationalBinaryOp, RowSource, SpecOperationCatalogEntry,
    SpecOperationFamily, SpecOperationIr, ToPrimitiveHint, TrackedGapReason, UnaryBitwiseOp,
    UpdateReturnMode, COMPLETION_ABI_SLOTS, SPEC_OPERATION_CATALOG, SPEC_OPERATION_ROW_COUNT,
};
pub use ownership::{ConformanceOwner, ParseConformanceOwnerError};
pub(crate) use prepared_function::DynamicFunctionSource;
pub use prepared_function::{PreparedDynamicFunction, PreparedDynamicFunctionOutcome};
pub use prepared_script::{
    AnnexBGlobalDeclarationIr, GlobalFunctionDeclarationIr, GlobalScriptEntryIr, PreparedScript,
    PreparedScriptAdmission, PreparedScriptKind, PreparedScriptOutcome, PreparedScriptUnit,
    RuntimeGlobalDeclarationPlan, StaticScriptId,
};
pub(crate) use prepared_script::{DynamicScriptSource, ScriptInstantiation};
pub use prepared_source_cache::LoweringSession;
pub use regexp::{
    escape_regexp_pattern_utf16, regexp_character_escape, regexp_hex_digit_value,
    regexp_unicode_property_catalog, CaseFolding as RegExpCaseFolding, RegExpCompileError,
    RegExpCompileErrorKind, RegExpControlFlow, RegExpFlags, RegExpIdentifierPosition,
    RegExpInputProgress, RegExpInstruction, RegExpModifierOverride, RegExpNamedGroup,
    RegExpNatural, RegExpOpcode, RegExpOperandRule, RegExpProgram, RegExpProgramValidationError,
    RegExpProgramWord, RegExpRepeatBoundWord, RegExpRepeatBounds, RegExpRepeatMaximum,
    RegExpRepeatMaximumKind, RegExpRepeatStateWord, RegExpScopedModifier, RegExpUnicodeMode,
    RegExpUnicodePropertyCatalogEntry, RegExpUnicodePropertyCatalogValue, ValidatedRegExpProgram,
    REGEXP_BACKREFERENCE_IGNORE_CASE, REGEXP_BACKREFERENCE_NONEMPTY, REGEXP_CHARACTER_ESCAPES,
    REGEXP_DIGIT_RANGES, REGEXP_EMPTY_SOURCE, REGEXP_HEX_DIGIT_RANGES, REGEXP_INSTRUCTION_WIDTH,
    REGEXP_LEGACY_THREE_DIGIT_OCTAL_LAST, REGEXP_MAX_INSTRUCTIONS, REGEXP_MAX_RANGE_ENTRIES,
    REGEXP_NAMED_GROUP_TABLE_MAGIC_VERSION, REGEXP_OPCODE_ACCEPT, REGEXP_OPCODE_ASSERT_END,
    REGEXP_OPCODE_ASSERT_START, REGEXP_OPCODE_CAPTURE_END, REGEXP_OPCODE_CAPTURE_START,
    REGEXP_OPCODE_CLEAR_CAPTURE_RANGE, REGEXP_OPCODE_DOT, REGEXP_OPCODE_JUMP,
    REGEXP_OPCODE_LITERAL_ASCII, REGEXP_OPCODE_LITERAL_CODE_POINT, REGEXP_OPCODE_LOOKAROUND_END,
    REGEXP_OPCODE_LOOKAROUND_FAILURE, REGEXP_OPCODE_LOOKAROUND_START,
    REGEXP_OPCODE_NAMED_BACKREFERENCE, REGEXP_OPCODE_NEGATIVE_ASCII_CLASS,
    REGEXP_OPCODE_NOT_WHITESPACE, REGEXP_OPCODE_NUMBERED_BACKREFERENCE,
    REGEXP_OPCODE_POSITIVE_ASCII_CLASS, REGEXP_OPCODE_PROGRESS_CHECK, REGEXP_OPCODE_PROGRESS_SPLIT,
    REGEXP_OPCODE_REPEAT_BEGIN, REGEXP_OPCODE_REPEAT_END, REGEXP_OPCODE_REPEAT_EXIT,
    REGEXP_OPCODE_REPEAT_GUARD, REGEXP_OPCODE_SPLIT, REGEXP_OPCODE_UNICODE_PROPERTY,
    REGEXP_OPCODE_WHITESPACE, REGEXP_OPCODE_WORD_BOUNDARY, REGEXP_PROGRAM_HEADER_SIZE,
    REGEXP_PROGRAM_MAGIC_VERSION, REGEXP_RANGE_ENTRY_WIDTH, REGEXP_REPEAT_BOUND_RECORD_SIZE,
    REGEXP_REPEAT_COUNTER_DECIMAL_DIGITS, REGEXP_REPEAT_COUNTER_LIMB_WIDTH,
    REGEXP_REPEAT_COUNTER_RADIX, REGEXP_REPEAT_STATE_HEADER_SIZE, REGEXP_SOURCE_LINE_ESCAPES,
    REGEXP_SOURCE_SLASH_ESCAPE, REGEXP_WHITESPACE_RANGES, REGEXP_WORD_RANGES,
};
pub use robustness::{IrRobustnessError, IrRobustnessInput};
pub use runtime_semantics::{
    RuntimeSemanticGap, RuntimeSemanticRejection, RuntimeUnavailableCapability,
    WasmWeakReachabilityCapability, PRODUCT_WASM_WEAK_REACHABILITY,
};

pub use names::*;
pub(crate) use names::{
    MAX_ARRAY_INDEX, MAX_STATIC_ARRAY_SHAPE_INDEX, SCRIPT_OWNER_ID, TDZ_BINDING_STORAGE_PREFIX,
};

/// The three module binding-name domains. See
/// `docs/rust-rewrite/contracts/module-binding-names.md`.
pub use binding_names::*;
pub(crate) use binding_names::{
    DEFAULT_BINDING_ASSIGN, DEFAULT_BINDING_LET, DEFAULT_BINDING_VAR, IMPORT_META_HEAD,
    IMPORT_META_TAIL,
};

/// The two closed spec name domains. See
/// `docs/rust-rewrite/contracts/closed-name-domains.md`.
pub use native_error::NativeErrorKind;

/// The closed domain of pre-evaluation rejection codes, re-exported from
/// `scriptw-front` so consumers of `IrDiagnostic::code` have one path to it. See
/// `docs/rust-rewrite/contracts/early-error-taxonomy.md`.
pub use early_error_code::{EarlyErrorCode, ParseClassified};
pub use well_known::{
    is_symbol_description, shape_namespace_key, SymbolDescription, SymbolMemberName,
    WellKnownSymbol,
};

/// The Property Descriptor lattice: one closed 6.2.6 type, one derived
/// classification, and the two carriers that share them. See
/// `docs/rust-rewrite/contracts/property-descriptor-lattice.md`.
pub use crate::property_descriptor::{
    classify, complete_property_descriptor, AccessorSide, BothDataAndAccessor, CompleteDescriptor,
    CompletionDefaults, DataSide, DescriptorCarrier, DescriptorClassification, DescriptorField,
    DescriptorSide, DescriptorSideMarker, DescriptorSourceText, KindTerms, KnownPresence,
    PartialDescriptor, Presence, PropertyDescriptorKind, SourceText, ValidateError,
    ValidatedDescriptor, TO_PROPERTY_DESCRIPTOR_ORDER,
};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod global_script_prelude_tests;
