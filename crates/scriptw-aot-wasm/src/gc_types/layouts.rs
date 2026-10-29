//! One declaration authority for fields, arrays, recursive indices and access.
//! Semantic declarations are encoded now; allocation waits for the atomic ABI.

use super::*;
use wasm_encoder::{ArrayType, CompositeInnerType, CompositeType, FieldType, StructType, SubType};

/// The stored value's closed tag determines which strong GC reference is live.
/// This is a reference storage marker, never a linear address or integer root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnyGcRef {}
impl sealed::Sealed for AnyGcRef {}

/// Opaque host resource identity, distinct from the internal GC graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExternalResourceRef {}
impl sealed::Sealed for ExternalResourceRef {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum I8Value {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum I16Value {}
impl sealed::Sealed for I8Value {}
impl sealed::Sealed for I16Value {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GcStorageGet {
    Unpacked,
    UnsignedPacked,
}

macro_rules! scalar_storage {
    ($value:ty, $($storage:tt)+) => {
        impl<Owner: GcHeapType> GcFieldValue<Owner, NonNullable> for $value {
            const GET_KIND: GcStorageGet = match $($storage)+ {
                StorageType::I8 | StorageType::I16 => GcStorageGet::UnsignedPacked,
                StorageType::Val(_) => GcStorageGet::Unpacked,
            };
            fn storage_type(_: &GcLayoutRegistry) -> StorageType {
                $($storage)+
            }
        }
        scalar_storage!(@marker $value, $($storage)+);
    };
    (@marker $value:ty, StorageType::Val(ValType::I32)) => { impl GcI32Field for $value {} };
    (@marker $value:ty, StorageType::Val(ValType::I64)) => { impl GcI64Field for $value {} };
    (@marker $value:ty, StorageType::Val(ValType::F64)) => { impl GcF64Field for $value {} };
    (@marker $value:ty, StorageType::I8) => { impl GcI32Field for $value {} };
    (@marker $value:ty, StorageType::I16) => { impl GcI32Field for $value {} };
}
scalar_storage!(I32Value, StorageType::Val(ValType::I32));
scalar_storage!(I64Value, StorageType::Val(ValType::I64));
scalar_storage!(F64Value, StorageType::Val(ValType::F64));
scalar_storage!(I8Value, StorageType::I8);
scalar_storage!(I16Value, StorageType::I16);

/// The sole active ordinary-Array backing. Only the indexed mutation owners
/// publish this mode; logical length never chooses or sizes a representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayIndexedStorageMode {
    DenseDescriptors,
    SparseDescriptors,
    PackedNumbers,
    PackedValues,
}

// Reuse the closed domains already validated by the actual builtin/provider
// producers. Optional scalar domains describe inactive slots, not nullable
// Wasm references. Their value encoding belongs to the atomic value ABI.
impl<T: sealed::Sealed> sealed::Sealed for Option<T> {}
macro_rules! closed_scalar_storage {
    ($($domain:ty),+ $(,)?) => { $(
        impl sealed::Sealed for $domain {}
        scalar_storage!($domain, StorageType::Val(ValType::I32));
        scalar_storage!(Option<$domain>, StorageType::Val(ValType::I32));
    )+ };
}
closed_scalar_storage!(
    ArrayIndexedStorageMode,
    bool,
    crate::builtins::AsyncDisposableStackDisposeCompletionKind,
    scriptw_ir::FunctionProtocolIr,
    scriptw_ir::ClassElementExecutionKind,
    scriptw_ir::ClassHeritageKind,
    scriptw_ir::StandardBuiltinId,
    scriptw_ir::HostBuiltinId,
    scriptw_intl::CollatorUsage,
    scriptw_intl::CollatorSensitivity,
    scriptw_intl::CollatorCaseFirst,
    scriptw_intl::CollatorCollationKind,
    scriptw_intl::ListType,
    scriptw_intl::ListStyle,
    scriptw_intl::DisplayNamesType,
    scriptw_intl::DisplayNamesStyle,
    scriptw_intl::DisplayNamesFallback,
    scriptw_intl::DisplayNamesLanguageDisplay,
    scriptw_intl::RelativeStyle,
    scriptw_intl::RelativeNumeric,
    scriptw_intl::RelativeUnit,
    scriptw_intl::DurationStyle,
    scriptw_intl::DurationUnitStyle,
    scriptw_intl::DurationDisplay,
    scriptw_intl::DurationFractionalDigits,
    scriptw_intl::SegmenterGranularity,
    scriptw_intl::PluralType,
    scriptw_intl::NumberPrecisionKind,
    scriptw_intl::NumberNumericKind,
    scriptw_intl::number_format::options::LocaleMatcher,
    scriptw_intl::number_format::options::RoundingPriority,
    scriptw_intl::number_format::options::StyleOption,
    scriptw_intl::number_format::options::CurrencyDisplay,
    scriptw_intl::number_format::options::CurrencySign,
    scriptw_intl::number_format::options::UnitDisplay,
    scriptw_intl::number_format::options::NotationOption,
    scriptw_intl::number_format::options::CompactDisplay,
    scriptw_intl::number_format::options::Grouping,
    scriptw_intl::number_format::options::SignDisplay,
    scriptw_intl::number_format::options::RoundingMode,
    scriptw_intl::number_format::options::TrailingZeroDisplay,
    scriptw_intl::number_format::options::IntegerDigitCount,
    scriptw_intl::number_format::options::FractionDigitCount,
    scriptw_intl::number_format::options::SignificantDigitCount,
    scriptw_intl::DateTimeCalendar,
    scriptw_intl::DateTimeHourCyclePreference,
    scriptw_intl::DateTimeLocaleMatcher,
    scriptw_intl::DateTimeFormatMatcher,
    scriptw_intl::DateTimeRequired,
    scriptw_intl::DateTimeDefaults,
    scriptw_intl::DateTimeValueKind,
    scriptw_intl::DateTimePartKind,
    scriptw_intl::DateTimeRangeSource,
    scriptw_intl::DateTimeHourCycle,
    scriptw_intl::DateTimeNumericWidth,
    scriptw_intl::DateTimeTextWidth,
    scriptw_intl::DateTimeMonthWidth,
    scriptw_intl::DateTimeFractionalDigits,
    scriptw_intl::DateTimeStyle,
    scriptw_intl::TimeZoneNameStyle,
    scriptw_intl::TimeZoneKind,
    crate::module::TypedArrayElementKind,
    crate::heap::GeneratorState,
    crate::heap::GeneratorResumeKind,
    crate::heap::AsyncGeneratorExecutionState,
    crate::heap::AsyncGeneratorBodyStatus,
    crate::heap::AsyncGeneratorResumeKind,
    crate::heap::AsyncGeneratorRequestCompletionKind,
    crate::heap::PromiseState,
    crate::heap::PromiseReactionType,
    crate::heap::PromiseReactionCallbackKind,
    crate::heap::TemporalInitializationState,
    crate::heap::IntlInitializationState,
    crate::heap::DisposableStackState,
    crate::heap::AsyncDisposableStackState,
    crate::heap::DisposableStackEntryKind,
    crate::heap::AsyncDisposableStackEntryKind,
    crate::heap::PromiseJobKind,
    crate::heap::ActivationAsyncDisposeCapabilityState,
    crate::heap::ActivationAsyncDisposeEntryKind,
    crate::emit::CompletionKind,
    crate::heap::ModuleEvaluationState,
    crate::heap::ModuleEvaluationCompletion,
    crate::heap::ModuleBodyState,
    crate::heap::ModuleActivationKind,
    crate::heap::ModuleRequestPhase,
    crate::heap::AsyncModuleEntryMode,
    crate::builtins::JsonParseFrameState,
    crate::builtins::JsonReviverFrameState,
    crate::builtins::JsonReviverPropertyRole,
    scriptw_intl::number_format::options::RoundingIncrement,
    scriptw_intl::PluralCategorySet,
);
impl sealed::Sealed for crate::heap::DescriptorWord {}
scalar_storage!(crate::heap::DescriptorWord, StorageType::Val(ValType::I32));
impl sealed::Sealed for scriptw_intl::DateTimeFormatAvailability {}
scalar_storage!(
    scriptw_intl::DateTimeFormatAvailability,
    StorageType::Val(ValType::I64)
);

// Closed target state domains whose current emitter definitions are private
// to builtin bodies. These are storage contracts, not copied wire offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapIterationKind {
    Key,
    Value,
    KeyAndValue,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetIterationKind {
    Value,
    KeyAndValue,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrayIterationKind {
    Key,
    Value,
    KeyAndValue,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrivateElementKind {
    Brand,
    Field,
    Method,
    Accessor,
}
/// The complete virtual wrapper-method family consumed by for-await and
/// async yield-star. Its methods return the actual intrinsic Promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncFromSyncIteratorMethod {
    Next,
    Return,
    Throw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AwaitCompletionKind {
    Normal,
    Throw,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncGeneratorReturnStage {
    Unawaited,
    Awaited,
}
/// Method result currently awaited by an async yield-star/transparent loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeneratorDelegatePending {
    Normal,
    Return,
    Throw,
    MissingThrowClose,
    ReturnValue,
    YieldValue,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BufferOwnerKind {
    ArrayBuffer,
    SharedArrayBuffer,
}
closed_scalar_storage!(
    MapIterationKind,
    SetIterationKind,
    ArrayIterationKind,
    PrivateElementKind,
    scriptw_ir::LanguageFeaturePolicy,
    AwaitCompletionKind,
    AsyncGeneratorReturnStage,
    GeneratorDelegatePending,
    BufferOwnerKind
);

/// Validated chunk/window modes remain closed in every persisted helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IteratorBufferMode {
    Chunks,
    WindowsOnlyFull,
    WindowsAllowPartial,
}

/// Validated Iterator.zip/zipKeyed options become this closed state domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IteratorZipMode {
    Shortest,
    Longest,
    Strict,
}
/// Call/construct capability survives Proxy revocation independently of target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProxyCallCapability {
    ObjectOnly,
    CallOnly,
    CallAndConstruct,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutableCodeKind {
    JavaScript,
    StandardBuiltin,
    HostBuiltin,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinClosureCaptureKind {
    IntlCollator,
    IntlNumberFormat,
    IntlDateTimeFormat,
    PromiseResolving,
    PromiseCapabilityExecutor,
    PromiseElement,
    PromiseKeyedElement,
    PromiseFinally,
    PromiseFinallyValue,
    ArrayFromAsync,
    ProxyRevocation,
    AsyncDisposableStackDisposal,
    AsyncDisposableStackSyncDispose,
    RegExpLegacyAccessor,
    ShadowRealmWrappedFunction,
    ShadowRealmImport,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegExpLegacySlot {
    Input,
    LastMatch,
    LastParen,
    LeftContext,
    RightContext,
    Paren1,
    Paren2,
    Paren3,
    Paren4,
    Paren5,
    Paren6,
    Paren7,
    Paren8,
    Paren9,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrayFromAsyncStage {
    InputValue,
    MappedValue,
    AsyncIteratorResult,
    AsyncCloseResult,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrayFromAsyncSourceMode {
    ArrayLike,
    AsyncIterator,
    SyncIterator,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostResourceKind {
    SharedBuffer,
    WeakMap,
    WeakSet,
    WeakRef,
    FinalizationCell,
}
closed_scalar_storage!(
    HostResourceKind,
    IteratorZipMode,
    IteratorBufferMode,
    ProxyCallCapability,
    ExecutableCodeKind,
    BuiltinClosureCaptureKind,
    RegExpLegacySlot,
    ArrayFromAsyncStage,
    ArrayFromAsyncSourceMode
);

macro_rules! abstract_reference_storage {
    ($value:ty, $reference:expr) => {
        impl<Owner: GcHeapType, N: GcFieldNullability> GcFieldValue<Owner, N> for $value {
            const GET_KIND: GcStorageGet = GcStorageGet::Unpacked;
            fn storage_type(_: &GcLayoutRegistry) -> StorageType {
                StorageType::Val(ValType::Ref(($reference).nullable(N::NULLABLE)))
            }
        }
    };
}
abstract_reference_storage!(AnyGcRef, RefType::EQREF);
abstract_reference_storage!(ExternalResourceRef, RefType::EXTERNREF);

impl<Owner: GcHeapType, Target: GcHeapType, N: GcFieldNullability> GcFieldValue<Owner, N>
    for GcRef<Target>
{
    const GET_KIND: GcStorageGet = GcStorageGet::Unpacked;
    fn storage_type(layouts: &GcLayoutRegistry) -> StorageType {
        StorageType::Val(ValType::Ref(RefType {
            nullable: N::NULLABLE,
            heap_type: HeapType::Concrete(layouts.index::<Target>().raw()),
        }))
    }
}

/// Struct and array owners are disjoint: an array cannot name a struct field.
pub(crate) trait GcStructHeapType: GcHeapType + sealed::Struct {
    const FIELD_COUNT: u32;
}
pub(crate) trait GcArrayHeapType: GcHeapType + sealed::Array {
    type Element: GcFieldValue<Self, Self::Nullability>;
    type Mutability: GcFieldMutability;
    type Nullability: GcFieldNullability;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GcStructType<Owner: GcStructHeapType> {
    type_index: GcTypeIndex<Owner>,
}

impl<Owner: GcStructHeapType> GcStructType<Owner> {
    const fn new(type_index: GcTypeIndex<Owner>) -> Self {
        Self { type_index }
    }
    pub(super) const fn type_index(self) -> GcTypeIndex<Owner> {
        self.type_index
    }

    pub(super) fn emit_new(self, function: &mut Function) {
        function.instruction(&Instruction::StructNew(self.type_index.raw()));
    }

    pub(crate) fn field<V, M, N>(
        self,
        field: GcField<Owner, V, M, N>,
    ) -> GcFieldAccessor<Owner, V, M, N>
    where
        V: GcFieldValue<Owner, N>,
        M: GcFieldMutability,
        N: GcFieldNullability,
    {
        GcFieldAccessor {
            owner: self.type_index,
            field,
        }
    }
}

/// Declaration-generated field identity bound to its registered owner.
pub(crate) struct GcFieldAccessor<Owner, V, M, N>
where
    Owner: GcStructHeapType,
    V: GcFieldValue<Owner, N>,
    M: GcFieldMutability,
    N: GcFieldNullability,
{
    owner: GcTypeIndex<Owner>,
    field: GcField<Owner, V, M, N>,
}

impl<Owner, V, M, N> GcFieldAccessor<Owner, V, M, N>
where
    Owner: GcStructHeapType,
    V: GcFieldValue<Owner, N>,
    M: GcFieldMutability,
    N: GcFieldNullability,
{
    pub(super) fn emit_get(&self, function: &mut Function) {
        let instruction = match V::GET_KIND {
            GcStorageGet::Unpacked => Instruction::StructGet {
                struct_type_index: self.owner.raw(),
                field_index: self.field.ordinal().raw(),
            },
            GcStorageGet::UnsignedPacked => Instruction::StructGetU {
                struct_type_index: self.owner.raw(),
                field_index: self.field.ordinal().raw(),
            },
        };
        function.instruction(&instruction);
    }
}

impl<Owner, V, N> GcFieldAccessor<Owner, V, Mutable, N>
where
    Owner: GcStructHeapType,
    V: GcFieldValue<Owner, N>,
    N: GcFieldNullability,
{
    pub(super) fn emit_set(&self, function: &mut Function) {
        function.instruction(&Instruction::StructSet {
            struct_type_index: self.owner.raw(),
            field_index: self.field.ordinal().raw(),
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GcArrayType<Owner, V, M, N>
where
    Owner: GcArrayHeapType,
    V: GcFieldValue<Owner, N>,
    M: GcFieldMutability,
    N: GcFieldNullability,
{
    type_index: GcTypeIndex<Owner>,
    element: PhantomData<fn() -> (V, M, N)>,
}

impl<Owner, V, M, N> GcArrayType<Owner, V, M, N>
where
    Owner: GcArrayHeapType,
    V: GcFieldValue<Owner, N>,
    M: GcFieldMutability,
    N: GcFieldNullability,
{
    const fn new(type_index: GcTypeIndex<Owner>) -> Self {
        Self {
            type_index,
            element: PhantomData,
        }
    }

    pub(super) const fn type_index(&self) -> GcTypeIndex<Owner> {
        self.type_index
    }

    pub(super) fn emit_get(&self, function: &mut Function) {
        match V::GET_KIND {
            GcStorageGet::UnsignedPacked => {
                function.instruction(&Instruction::ArrayGetU(self.type_index.raw()));
            }
            GcStorageGet::Unpacked => {
                function.instruction(&Instruction::ArrayGet(self.type_index.raw()));
            }
        }
    }
}

impl<Owner, V, N> GcArrayType<Owner, V, Mutable, N>
where
    Owner: GcArrayHeapType,
    V: GcFieldValue<Owner, N>,
    N: GcFieldNullability,
{
    pub(super) fn emit_set(&self, function: &mut Function) {
        function.instruction(&Instruction::ArraySet(self.type_index.raw()));
    }
}

/// Concrete function references retain the registry that owns their signature.
/// Program hooks reuse their helper row; they do not copy it into StaticSignature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GcFunctionSignature {
    Entry(crate::module::StaticSignature),
    Scalar(crate::module::StaticSignature),
    ProgramHook(crate::runtime_helpers::ProgramHook),
    ContextHook(crate::runtime_helpers::RuntimeHelperId),
}
impl GcFunctionSignature {
    pub(crate) const fn type_index(self) -> u32 {
        match self {
            Self::Entry(signature) | Self::Scalar(signature) => signature.type_index(),
            Self::ProgramHook(hook) => hook.helper().type_index(),
            Self::ContextHook(helper) => helper.type_index(),
        }
    }
}
pub(crate) trait GcFunctionRole: sealed::Sealed + 'static {
    const SIGNATURE: GcFunctionSignature;
}
/// Only JavaScript/native entry roles produce the ordinary Completion ABI.
/// Hook roles retain their registered helper-specific result shape instead.
pub(crate) trait GcEntryFunctionRole: GcFunctionRole {}
macro_rules! callable_roles {
    ($($role:ident => $signature:ident),+ $(,)?) => { $(
        pub(crate) enum $role {}
        impl sealed::Sealed for $role {}
        impl GcFunctionRole for $role { const SIGNATURE: GcFunctionSignature = GcFunctionSignature::Entry(crate::module::StaticSignature::$signature); }
        impl GcEntryFunctionRole for $role {}
    )+ };
}
callable_roles!(OrdinaryCallable => JavaScriptFunction, GeneratorCallable => GeneratorFunction,
    AsyncCallable => AsyncFunction, AsyncGeneratorCallable => AsyncGeneratorFunction);

/// A scalar attempt has no Completion ABI and cannot pass through call_entry.
pub(crate) enum BinaryNumericCallback {}
impl sealed::Sealed for BinaryNumericCallback {}
impl GcFunctionRole for BinaryNumericCallback {
    const SIGNATURE: GcFunctionSignature =
        GcFunctionSignature::Scalar(crate::module::StaticSignature::BinaryNumericCallback);
}

/// Only the five closed hook roles can select one ProgramInstance hook field.
pub(crate) trait GcProgramHookRole: GcFunctionRole + Sized {
    const HOOK: crate::runtime_helpers::ProgramHook;
    fn field() -> GcField<ProgramInstance, GcFunctionRef<Self>, Immutable, Nullable>;
}
macro_rules! program_hook_roles {
    ($($role:ident => $hook:ident, $field:ident);+ $(;)?) => { $(
        pub(crate) enum $role {}
        impl sealed::Sealed for $role {}
        impl GcFunctionRole for $role {
            const SIGNATURE: GcFunctionSignature = GcFunctionSignature::ProgramHook(crate::runtime_helpers::ProgramHook::$hook);
        }
        impl GcProgramHookRole for $role {
            const HOOK: crate::runtime_helpers::ProgramHook = crate::runtime_helpers::ProgramHook::$hook;
            fn field() -> GcField<ProgramInstance, GcFunctionRef<Self>, Immutable, Nullable> {
                ProgramInstanceSchema::$field
            }
        }
    )+ };
}
program_hook_roles! {
    PreparedDynamicFunctionHook => PreparedDynamicFunction, PREPARED_DYNAMIC_FUNCTION;
    PreparedScriptHook => PreparedScript, PREPARED_SCRIPT;
    RealmModuleImportHook => RealmModuleImport, REALM_MODULE_IMPORT;
    ModuleBodyReactionHook => ModuleBodyReaction, MODULE_BODY_REACTION;
    RegExpProgramCandidateHook => RegExpProgramCandidate, REGEXP_PROGRAM_CANDIDATE;
}
/// A constructor retains this P-owned initializer independently of its caller.
pub(crate) enum InitializeInstanceElementsHook {}
impl sealed::Sealed for InitializeInstanceElementsHook {}
impl GcFunctionRole for InitializeInstanceElementsHook {
    const SIGNATURE: GcFunctionSignature = GcFunctionSignature::ContextHook(
        crate::runtime_helpers::RuntimeHelperId::InitializeInstanceElements,
    );
}
pub(crate) struct GcFunctionRef<R: GcFunctionRole>(PhantomData<fn() -> R>);
impl<R: GcFunctionRole> sealed::Sealed for GcFunctionRef<R> {}
impl<O: GcHeapType, R: GcFunctionRole, N: GcFieldNullability> GcFieldValue<O, N>
    for GcFunctionRef<R>
{
    const GET_KIND: GcStorageGet = GcStorageGet::Unpacked;
    fn storage_type(_: &GcLayoutRegistry) -> StorageType {
        StorageType::Val(ValType::Ref(RefType {
            nullable: N::NULLABLE,
            heap_type: HeapType::Concrete(R::SIGNATURE.type_index()),
        }))
    }
}

/// A row generates the marker, registered schema, field ordinals and encoder.
/// No layout or field can be added without its complete storage contract.
fn object_header_field<T: GcHeapType>(
    field: GcField<T, GcRef<OrdinaryObject>, Immutable, NonNullable>,
) -> GcFieldOrdinal {
    field.ordinal()
}

// Every object projection must have the actual-object supertype at compile time.
fn require_javascript_object_subtype<T: GcSubtype<JavaScriptObject>>() {}

macro_rules! gc_layout_registry {
    ($($kind:ident $owner:ident $(: $parent:ident)? => $schema:ident {
        $($field:ident: $value:ty, $mutability:ident, $nullability:ty;)*
    })+) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum GcLayout { $($owner),+ }
        impl GcLayout {
            pub(crate) const ALL: &'static [Self] = &[$(Self::$owner),+];
            const fn ordinal(self) -> u32 { self as u32 }
            pub(super) fn object_projection(self) -> Option<ObjectHeaderProjection> {
                match self { $(Self::$owner => gc_layout_registry!(@object_projection $kind $owner; $($field:$value,$mutability,$nullability;)*)),+ }
            }
            pub(crate) fn javascript_tag(self) -> Option<crate::WasmRuntimeValueTag> {
                match self { $(Self::$owner => gc_layout_registry!(@value_tag $kind $owner; $($field:$value,$mutability,$nullability;)*)),+ }
            }
            pub(super) fn definition(self, layouts: &GcLayoutRegistry) -> SubType {
                let inner = match self {
                    $(Self::$owner => gc_layout_registry!(@definition $kind $owner
                        layouts; $($field: $value, $mutability, $nullability;)*)),+
                };
                let is_final = !matches!(self, Self::ObjectPropertyStorage | Self::CompactDataStorage | Self::PropertyValue | Self::JavaScriptObject);
                let supertype_idx = match self {
                    $(Self::$owner => gc_layout_registry!(@parent layouts; $($parent)?)),+
                };
                SubType { is_final, supertype_idx,
                    composite_type: CompositeType { inner, shared: false,
                        descriptor: None, describes: None } }
            }
        }
        $(
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum $owner {}
            impl sealed::Sealed for $owner {}
            impl GcHeapType for $owner { const LAYOUT: GcLayout = GcLayout::$owner; }
            gc_layout_registry!(@subtype $owner; $($parent)?);
            gc_layout_registry!(@schema $kind $owner $schema;
                $($field: $value, $mutability, $nullability;)*);
            gc_layout_registry!(@javascript_reference $kind $owner; $($field:$value,$mutability,$nullability;)*);
            gc_layout_registry!(@object_subtype_proof $kind $owner; $($field:$value,$mutability,$nullability;)*);
        )+
    };
    (@parent $layouts:ident;) => { None };
    (@parent $layouts:ident; $parent:ident) => { Some($layouts.index::<$parent>().raw()) };
    (@subtype $owner:ident;) => {};
    (@subtype $owner:ident; $parent:ident) => {
        impl sealed::Subtype<$parent> for $owner {}
        impl GcSubtype<$parent> for $owner {}
    };
    (@object_subtype_proof struct OrdinaryObject; $($fields:tt)+) => {
        const _: fn() = require_javascript_object_subtype::<OrdinaryObject>;
    };
    (@object_subtype_proof struct $owner:ident; OBJECT:$value:ty,$mutable:ident,$null:ty; $($rest:tt)*) => {
        const _: fn() = require_javascript_object_subtype::<$owner>;
    };
    (@object_subtype_proof $kind:ident $owner:ident; $($fields:tt)*) => {};
    (@object_projection struct OrdinaryObject; $($fields:tt)+) => { Some(ObjectHeaderProjection::Own) };
    (@object_projection struct $owner:ident; OBJECT:$value:ty,$mutable:ident,$null:ty; $($rest:tt)*) => {
        Some(ObjectHeaderProjection::Field(object_header_field(GcStructType::<$owner>::OBJECT)))
    };
    (@object_projection $kind:ident $owner:ident; $($fields:tt)*) => { None };
    (@value_tag struct StringValue; $($fields:tt)+) => { Some(crate::WasmRuntimeValueTag::String) };
    (@value_tag struct BigIntValue; $($fields:tt)+) => { Some(crate::WasmRuntimeValueTag::BigInt) };
    (@value_tag struct SymbolValue; $($fields:tt)+) => { Some(crate::WasmRuntimeValueTag::Symbol) };
    (@value_tag struct OrdinaryObject; $($fields:tt)+) => { Some(crate::WasmRuntimeValueTag::Object) };
    (@value_tag struct ArrayObject; $($fields:tt)+) => { Some(crate::WasmRuntimeValueTag::Array) };
    (@value_tag struct FunctionObject; $($fields:tt)+) => { Some(crate::WasmRuntimeValueTag::Function) };
    (@value_tag struct BoundFunction; $($fields:tt)+) => { Some(crate::WasmRuntimeValueTag::Function) };
    (@value_tag struct ArgumentsObject; $($fields:tt)+) => { Some(crate::WasmRuntimeValueTag::Arguments) };
    (@value_tag struct $owner:ident; OBJECT:$value:ty,$mutable:ident,$null:ty; $($rest:tt)*) => { Some(crate::WasmRuntimeValueTag::Object) };
    (@value_tag $kind:ident $owner:ident; $($fields:tt)*) => { None };
    (@javascript_reference struct StringValue; $($fields:tt)+) => { impl JavaScriptReference for StringValue { const TAG: crate::WasmRuntimeValueTag = crate::WasmRuntimeValueTag::String; } };
    (@javascript_reference struct BigIntValue; $($fields:tt)+) => { impl JavaScriptReference for BigIntValue { const TAG: crate::WasmRuntimeValueTag = crate::WasmRuntimeValueTag::BigInt; } };
    (@javascript_reference struct SymbolValue; $($fields:tt)+) => { impl JavaScriptReference for SymbolValue { const TAG: crate::WasmRuntimeValueTag = crate::WasmRuntimeValueTag::Symbol; } };
    (@javascript_reference struct OrdinaryObject; $($fields:tt)+) => { impl JavaScriptReference for OrdinaryObject { const TAG: crate::WasmRuntimeValueTag = crate::WasmRuntimeValueTag::Object; } };
    (@javascript_reference struct ArrayObject; $($fields:tt)+) => { impl JavaScriptReference for ArrayObject { const TAG: crate::WasmRuntimeValueTag = crate::WasmRuntimeValueTag::Array; } };
    (@javascript_reference struct FunctionObject; $($fields:tt)+) => { impl JavaScriptReference for FunctionObject { const TAG: crate::WasmRuntimeValueTag = crate::WasmRuntimeValueTag::Function; } };
    (@javascript_reference struct BoundFunction; $($fields:tt)+) => { impl JavaScriptReference for BoundFunction { const TAG: crate::WasmRuntimeValueTag = crate::WasmRuntimeValueTag::Function; } };
    (@javascript_reference struct ArgumentsObject; $($fields:tt)+) => { impl JavaScriptReference for ArgumentsObject { const TAG: crate::WasmRuntimeValueTag = crate::WasmRuntimeValueTag::Arguments; } };
    (@javascript_reference struct $owner:ident; OBJECT:$value:ty,$mutable:ident,$null:ty; $($rest:tt)*) => { impl JavaScriptReference for $owner { const TAG: crate::WasmRuntimeValueTag = crate::WasmRuntimeValueTag::Object; } };
    (@javascript_reference $kind:ident $owner:ident; $($fields:tt)*) => {};
    (@schema struct ObjectPropertyStorage ObjectPropertyStorageSchema;) => {
        impl sealed::Struct for ObjectPropertyStorage {}
        impl GcStructHeapType for ObjectPropertyStorage { const FIELD_COUNT: u32 = 0; }
        // The non-final base is fieldless and has no allocating constructor.
        pub(crate) type ObjectPropertyStorageSchema = GcStructType<ObjectPropertyStorage>;
    };
    (@schema struct JavaScriptObject JavaScriptObjectSchema;) => {
        impl sealed::Struct for JavaScriptObject {}
        impl GcStructHeapType for JavaScriptObject { const FIELD_COUNT: u32 = 0; }
        // The object sum cannot allocate an object without its actual payload.
        pub(crate) type JavaScriptObjectSchema = GcStructType<JavaScriptObject>;
    };
    (@schema struct PropertyValue PropertyValueSchema;) => {
        impl sealed::Struct for PropertyValue {}
        impl GcStructHeapType for PropertyValue { const FIELD_COUNT: u32 = 0; }
        // The fieldless sum has no allocating constructor.
        pub(crate) type PropertyValueSchema = GcStructType<PropertyValue>;
    };
    (@schema struct CompactDataStorage CompactDataStorageSchema; $($fields:tt)+) => {
        impl sealed::Struct for CompactDataStorage {}
        impl GcStructHeapType for CompactDataStorage {
            const FIELD_COUNT: u32 = gc_layout_registry!(@field_count $($fields)+);
        }
        // The common shape-bearing sum has fields but no allocating constructor.
        pub(crate) type CompactDataStorageSchema = GcStructType<CompactDataStorage>;
        impl GcStructType<CompactDataStorage> {
            gc_layout_registry!(@fields CompactDataStorage 0; $($fields)+);
        }
    };
    (@schema struct $owner:ident $schema:ident; $($fields:tt)+) => {
        impl sealed::Struct for $owner {}
        impl GcStructHeapType for $owner {
            const FIELD_COUNT: u32 = gc_layout_registry!(@field_count $($fields)+);
        }
        pub(crate) type $schema = GcStructType<$owner>;
        impl GcStructType<$owner> {
            gc_layout_registry!(@fields $owner 0; $($fields)+);
            gc_layout_registry!(@construct $owner; $($fields)+);
        }
    };
    (@schema array $owner:ident $schema:ident;
        $field:ident: $value:ty, $mutability:ident, $nullability:ty;) => {
        impl sealed::Array for $owner {}
        impl GcArrayHeapType for $owner {
            type Element = $value;
            type Mutability = $mutability;
            type Nullability = $nullability;
        }
        pub(crate) type $schema = GcArrayType<$owner, $value, $mutability, $nullability>;
        gc_layout_registry!(@writable_array $owner, $mutability);
    };
    (@writable_array CodeUnitArray, $mutability:ident) => {};
    (@writable_array BigIntLimbArray, $mutability:ident) => {};
    (@writable_array PropertyKeyTable, $mutability:ident) => {};
    (@writable_array ArrayIndexKeyTable, $mutability:ident) => {};
    (@writable_array IteratorConcatEntries, $mutability:ident) => {};
    (@writable_array IteratorZipEntries, $mutability:ident) => {};
    (@writable_array SegmentBoundaryTable, $mutability:ident) => {};
    (@writable_array ImmutableByteArray, $mutability:ident) => {};
    (@writable_array $owner:ident, Mutable) => {
        impl sealed::WritableArray for $owner {}
        impl GcWritableArray for $owner {}
    };
    (@writable_array $owner:ident, Immutable) => {};
    (@construct ArrayIndexStorage; $($field:ident: $value:ty, $mutability:ident, $nullability:ty;)+) => {
        #[allow(non_snake_case)]
        pub(super) fn construct(self, fields: ($(GcOperand<'_, $value, $nullability>,)+), function: &mut Function) -> GcStackReference<ArrayIndexStorage> {
            let ($($field,)+) = fields;
            $($field.emit(function);)+ self.emit_new(function); self.construction_result()
        }
    };
    (@construct ExecutableCode; $($field:ident: $value:ty, $mutability:ident, $nullability:ty;)+) => {
        #[allow(non_snake_case)]
        pub(super) fn construct(self, fields: ($(GcOperand<'_, $value, $nullability>,)+), function: &mut Function) -> GcStackReference<ExecutableCode> {
            let ($($field,)+) = fields;
            $($field.emit(function);)+ self.emit_new(function); self.construction_result()
        }
    };
    (@construct StringValue; $($field:ident: $value:ty, $mutability:ident, $nullability:ty;)+) => {
        #[allow(non_snake_case)]
        pub(super) fn construct(self, fields: ($(GcOperand<'_, $value, $nullability>,)+), function: &mut Function) -> GcStackReference<StringValue> {
            let ($($field,)+) = fields;
            $($field.emit(function);)+ self.emit_new(function); self.construction_result()
        }
    };
    (@construct BigIntValue; $($field:ident: $value:ty, $mutability:ident, $nullability:ty;)+) => {
        #[allow(non_snake_case)]
        pub(super) fn construct(self, fields: ($(GcOperand<'_, $value, $nullability>,)+), function: &mut Function) -> GcStackReference<BigIntValue> {
            let ($($field,)+) = fields;
            $($field.emit(function);)+ self.emit_new(function); self.construction_result()
        }
    };
    (@construct StoredValue; $($field:ident: $value:ty, $mutability:ident, $nullability:ty;)+) => {
        #[allow(non_snake_case)]
        pub(super) fn construct(self, fields: ($(GcOperand<'_, $value, $nullability>,)+), function: &mut Function) -> GcStackReference<StoredValue> {
            let ($($field,)+) = fields;
            $($field.emit(function);)+ self.emit_new(function); self.construction_result()
        }
    };
    (@construct HostResource; $($field:ident: $value:ty, $mutability:ident, $nullability:ty;)+) => {
        #[allow(non_snake_case)]
        pub(super) fn construct(self, fields: ($(GcOperand<'_, $value, $nullability>,)+), function: &mut Function) -> GcStackReference<HostResource> {
            let ($($field,)+) = fields;
            $($field.emit(function);)+ self.emit_new(function); self.construction_result()
        }
    };
    (@construct BuiltinClosureCapture; $($field:ident: $value:ty, $mutability:ident, $nullability:ty;)+) => {
        #[allow(non_snake_case)]
        pub(super) fn construct(self, fields: ($(GcOperand<'_, $value, $nullability>,)+), function: &mut Function) -> GcStackReference<BuiltinClosureCapture> {
            let ($($field,)+) = fields;
            $($field.emit(function);)+ self.emit_new(function); self.construction_result()
        }
    };
    (@construct $owner:ident; $($field:ident: $value:ty, $mutability:ident, $nullability:ty;)+) => {
        #[allow(non_snake_case)]
        pub(crate) fn construct(self, fields: ($(GcOperand<'_, $value, $nullability>,)+), function: &mut Function) -> GcStackReference<$owner> {
            let ($($field,)+) = fields;
            $($field.emit(function);)+
            self.emit_new(function);
            self.construction_result()
        }
    };
    (@field_count $($field:ident: $value:ty, $mutability:ident, $nullability:ty;)+) => {
        [$(stringify!($field)),+].len() as u32
    };
    (@fields $owner:ident $ordinal:expr;) => {};
    (@fields $owner:ident $ordinal:expr;
        $field:ident: $value:ty, $mutability:ident, $nullability:ty; $($rest:tt)*) => {
        pub(crate) const $field: GcField<$owner, $value, $mutability, $nullability> =
            GcField::new(GcFieldOrdinal::new($ordinal));
        gc_layout_registry!(@fields $owner ($ordinal + 1); $($rest)*);
    };
    (@definition struct $owner:ident $layouts:ident;
        $($field:ident: $value:ty, $mutability:ident, $nullability:ty;)*) => {
        CompositeInnerType::Struct(StructType { fields: vec![$(
            FieldType { element_type: <$value as GcFieldValue<$owner, $nullability>>
                ::storage_type($layouts), mutable: <$mutability as GcFieldMutability>::MUTABLE }
        ),*].into_boxed_slice() })
    };
    (@definition array $owner:ident $layouts:ident;
        $field:ident: $value:ty, $mutability:ident, $nullability:ty;) => {
        CompositeInnerType::Array(ArrayType(FieldType {
            element_type: <$value as GcFieldValue<$owner, $nullability>>::storage_type($layouts),
            mutable: <$mutability as GcFieldMutability>::MUTABLE,
        }))
    };
}

gc_layout_registry! {
        struct JavaScriptObject => JavaScriptObjectSchema {}
        struct PropertyValue => PropertyValueSchema {}
        struct StoredValue: PropertyValue => StoredValueSchema {
            // Keep the 64-bit payload first: the copying collector can then
            // fit all fields after its header without an extra alignment unit.
            SCALAR: I64Value, Immutable, NonNullable;
            TAG: I32Value, Immutable, NonNullable;
            REFERENCE: AnyGcRef, Immutable, Nullable;
        }
        struct DeferredIntrinsicValue: PropertyValue => DeferredIntrinsicValueSchema {
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            BUILTIN: scriptw_ir::StandardBuiltinId, Immutable, NonNullable;
            CACHED: GcRef<StoredValue>, Mutable, Nullable;
        }
        struct DeferredTemporalNamespace: PropertyValue => DeferredTemporalNamespaceSchema {
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
        }
        struct DeferredIntlNamespace: PropertyValue => DeferredIntlNamespaceSchema {
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
        }
        struct OrdinaryObject: JavaScriptObject => OrdinaryObjectSchema {
            PROTOTYPE: GcRef<JavaScriptObject>, Mutable, Nullable;
            PROPERTIES: GcRef<ObjectPropertyStorage>, Mutable, NonNullable;
            PRIVATE_ELEMENTS: GcRef<PrivateElementTable>, Mutable, NonNullable;
            EXTENSIBLE: bool, Mutable, NonNullable;
            IMMUTABLE_PROTOTYPE: bool, Immutable, NonNullable;
            KEY_HASH_ID: I64Value, Mutable, NonNullable;
        }
        struct PropertyDescriptor => PropertyDescriptorSchema {
            FLAGS: crate::heap::DescriptorWord, Mutable, NonNullable;
            VALUE: GcRef<PropertyValue>, Mutable, NonNullable;
            GETTER: GcRef<StoredValue>, Mutable, NonNullable;
            SETTER: GcRef<StoredValue>, Mutable, NonNullable;
        }
        // Native DefineProperties retains converted partial descriptors until
        // every enumerable bag entry has completed conversion.
        struct PartialPropertyDescriptor => PartialPropertyDescriptorSchema {
            VALUE_PRESENT: bool, Immutable, NonNullable;
            WRITABLE_PRESENT: bool, Immutable, NonNullable;
            GETTER_PRESENT: bool, Immutable, NonNullable;
            SETTER_PRESENT: bool, Immutable, NonNullable;
            ENUMERABLE_PRESENT: bool, Immutable, NonNullable;
            CONFIGURABLE_PRESENT: bool, Immutable, NonNullable;
            VALUE: GcRef<StoredValue>, Immutable, NonNullable;
            GETTER: GcRef<StoredValue>, Immutable, NonNullable;
            SETTER: GcRef<StoredValue>, Immutable, NonNullable;
            WRITABLE: bool, Immutable, NonNullable;
            ENUMERABLE: bool, Immutable, NonNullable;
            CONFIGURABLE: bool, Immutable, NonNullable;
        }
        struct PropertyDefinition => PropertyDefinitionSchema {
            KEY: GcRef<StoredValue>, Immutable, NonNullable;
            DESCRIPTOR: GcRef<PartialPropertyDescriptor>, Immutable, NonNullable;
        }
        array PropertyDefinitionTable => PropertyDefinitionTableSchema { ELEMENT: GcRef<PropertyDefinition>, Mutable, Nullable; }
        struct PropertyEntry => PropertyEntrySchema {
            KEY: GcRef<StoredValue>, Immutable, NonNullable;
            DESCRIPTOR: GcRef<PropertyDescriptor>, Mutable, NonNullable;
            HASH: I64Value, Immutable, NonNullable;
            POSITION: I32Value, Immutable, NonNullable;
        }
        array PropertyTable => PropertyTableSchema { ELEMENT: GcRef<PropertyEntry>, Mutable, Nullable; }
        // Logical insertion extent is independent of spare capacity and holes.
        // Existing descriptors stay in place; deletion never rewinds LENGTH.
        // One object model: these sealed GC subtypes are alternate storage
        // states of the same ordinary header. The base cannot be allocated.
        struct ObjectPropertyStorage => ObjectPropertyStorageSchema {}
        struct CompactDataStorage: ObjectPropertyStorage => CompactDataStorageSchema {
            SHAPE: GcRef<CompactDataShape>, Immutable, NonNullable;
        }
        // Concrete subtypes repeat the inherited prefix in their physical layout.
        struct ArrayCompactDataStorage: CompactDataStorage => ArrayCompactDataStorageSchema {
            SHAPE: GcRef<CompactDataShape>, Immutable, NonNullable;
            TAGS: GcRef<CompactTagArray>, Immutable, NonNullable;
            SCALARS: GcRef<CompactScalarArray>, Mutable, Nullable;
            // Null backing means every slot has a null reference payload.
            REFERENCES: GcRef<CompactReferenceArray>, Mutable, Nullable;
        }
        struct InlineCompactDataStorage: CompactDataStorage => InlineCompactDataStorageSchema {
            SHAPE: GcRef<CompactDataShape>, Immutable, NonNullable;
            PACKED_TAGS: I32Value, Mutable, NonNullable;
            SCALAR0: I64Value, Mutable, NonNullable;
            SCALAR1: I64Value, Mutable, NonNullable;
            REFERENCE0: AnyGcRef, Mutable, Nullable;
            REFERENCE1: AnyGcRef, Mutable, Nullable;
        }
        // Shape graphs retain only immutable primitive string metadata.
        struct CompactDataShape => CompactDataShapeSchema {
            KEYS: GcRef<CompactShapeKeys>, Immutable, NonNullable;
            ATTRIBUTES: crate::heap::DescriptorWord, Immutable, NonNullable;
        }
        array CompactShapeTable => CompactShapeTableSchema { ELEMENT: GcRef<CompactDataShape>, Mutable, Nullable; }
        array CompactShapePageTable => CompactShapePageTableSchema { ELEMENT: GcRef<CompactShapeTable>, Mutable, Nullable; }
        array CompactShapeKeys => CompactShapeKeysSchema { ELEMENT: GcRef<StringValue>, Immutable, NonNullable; }
        array CompactTagArray => CompactTagArraySchema { ELEMENT: I8Value, Mutable, NonNullable; }
        array CompactScalarArray => CompactScalarArraySchema { ELEMENT: I64Value, Mutable, NonNullable; }
        array CompactReferenceArray => CompactReferenceArraySchema { ELEMENT: AnyGcRef, Mutable, Nullable; }
        struct OwnDataReadCache => OwnDataReadCacheSchema {
            KEY: GcRef<StringValue>, Immutable, NonNullable;
            SHAPE: GcRef<CompactDataShape>, Mutable, Nullable;
            SLOT: I32Value, Mutable, NonNullable;
        }
        array OwnDataReadCacheTable => OwnDataReadCacheTableSchema { ELEMENT: GcRef<OwnDataReadCache>, Mutable, Nullable; }
        array OwnDataReadCachePageTable => OwnDataReadCachePageTableSchema { ELEMENT: GcRef<OwnDataReadCacheTable>, Mutable, Nullable; }
        struct OrdinaryPropertyStorage: ObjectPropertyStorage => OrdinaryPropertyStorageSchema {
            ENTRIES: GcRef<PropertyTable>, Mutable, NonNullable;
            LENGTH: I32Value, Mutable, NonNullable;
            INDEX: GcRef<OrdinaryPropertyIndex>, Mutable, NonNullable;
        }
        // Zero is empty; other buckets are ordered entry positions plus one.
        array OrdinaryPropertyIndex => OrdinaryPropertyIndexSchema { ELEMENT: I32Value, Mutable, NonNullable; }
        array IndexedTable => IndexedTableSchema { ELEMENT: GcRef<PropertyDescriptor>, Mutable, Nullable; }
        // Arrays retain occupied indices independently of observable length.
        // Only the storage constructor and registered mutation bodies own the
        // closed mode and occupied-entry count. Packed Number values use raw
        // bits plus a hole-presence bitmap; full descriptors remain available
        // after one-way transitions. General ordinary data uses nullable immutable
        // StoredValue slots; inactive VALUES is null and other backings empty.
        struct ArrayIndexStorage => ArrayIndexStorageSchema {
            BUCKETS: GcRef<ArrayIndexBucketTable>, Mutable, NonNullable;
            COUNT: I64Value, Mutable, NonNullable;
            DENSE: GcRef<IndexedTable>, Mutable, NonNullable;
            MODE: ArrayIndexedStorageMode, Mutable, NonNullable;
            NUMBERS: GcRef<NumberBitsI64Array>, Mutable, NonNullable;
            NUMBER_PRESENT: GcRef<NumberPresenceU64Array>, Mutable, NonNullable;
            VALUES: GcRef<PackedValueTable>, Mutable, Nullable;
        }
        array PackedValueTable => PackedValueTableSchema { ELEMENT: GcRef<StoredValue>, Mutable, Nullable; }
        array NumberBitsI64Array => NumberBitsI64ArraySchema { ELEMENT: I64Value, Mutable, NonNullable; }
        array NumberPresenceU64Array => NumberPresenceU64ArraySchema { ELEMENT: I64Value, Mutable, NonNullable; }
        struct ArrayIndexEntry => ArrayIndexEntrySchema {
            INDEX: I64Value, Immutable, NonNullable;
            DESCRIPTOR: GcRef<PropertyDescriptor>, Mutable, NonNullable;
            NEXT: GcRef<ArrayIndexEntry>, Mutable, Nullable;
        }
        array ArrayIndexBucketTable => ArrayIndexBucketTableSchema { ELEMENT: GcRef<ArrayIndexEntry>, Mutable, Nullable; }
        // Private mutable construction; published snapshots expose reads only.
        array ArrayIndexKeyTable => ArrayIndexKeyTableSchema { ELEMENT: I64Value, Mutable, NonNullable; }

        array ValueArray => ValueArraySchema { ELEMENT: GcRef<StoredValue>, Mutable, NonNullable; }
        // Compiler-owned exact-sum scratch; never a JavaScript value or a
        // published BigInt magnitude.
        array MathSumPreciseLimbArray => MathSumPreciseLimbArraySchema { ELEMENT: I64Value, Mutable, NonNullable; }
        struct ArrayFlattenFrame => ArrayFlattenFrameSchema {
            SOURCE: GcRef<StoredValue>, Immutable, NonNullable;
            INDEX: I64Value, Mutable, NonNullable;
            LENGTH: I64Value, Immutable, NonNullable;
            DEPTH: F64Value, Immutable, NonNullable;
            MAPPING: bool, Immutable, NonNullable;
            PARENT: GcRef<ArrayFlattenFrame>, Immutable, Nullable;
        }
        // Private argument vectors retained by suspended source temporaries.
        array PrivateArgumentListTable => PrivateArgumentListTableSchema { ELEMENT: GcRef<ValueArray>, Mutable, Nullable; }
        // Internal ArgumentListEvaluation construction; never a JS value.
        struct ArgumentListNode => ArgumentListNodeSchema {
            VALUE: GcRef<StoredValue>, Immutable, NonNullable;
            NEXT: GcRef<ArgumentListNode>, Mutable, Nullable;
        }
        struct PrivateElement => PrivateElementSchema {
            NAME: GcRef<PrivateName>, Immutable, NonNullable;
            KIND: PrivateElementKind, Immutable, NonNullable;
            FIELD_VALUE: GcRef<StoredValue>, Mutable, Nullable;
            METHOD: GcRef<StoredValue>, Immutable, Nullable;
            GETTER: GcRef<StoredValue>, Immutable, Nullable;
            SETTER: GcRef<StoredValue>, Immutable, Nullable;
        }
        array PrivateElementTable => PrivateElementTableSchema { ELEMENT: GcRef<PrivateElement>, Mutable, Nullable; }
        struct ExecutableCode => ExecutableCodeSchema {
            SOURCE_ID: I64Value, Immutable, NonNullable;
            KIND: ExecutableCodeKind, Immutable, NonNullable;
            STANDARD_BUILTIN: Option<scriptw_ir::StandardBuiltinId>, Immutable, NonNullable;
            HOST_BUILTIN: Option<scriptw_ir::HostBuiltinId>, Immutable, NonNullable;
            PROTOCOL: scriptw_ir::FunctionProtocolIr, Immutable, NonNullable;
            CLASS_ELEMENT_EXECUTION: scriptw_ir::ClassElementExecutionKind, Immutable, NonNullable;
            ORDINARY_ENTRY: GcFunctionRef<OrdinaryCallable>, Immutable, Nullable;
            GENERATOR_ENTRY: GcFunctionRef<GeneratorCallable>, Immutable, Nullable;
            ASYNC_ENTRY: GcFunctionRef<AsyncCallable>, Immutable, Nullable;
            ASYNC_GENERATOR_ENTRY: GcFunctionRef<AsyncGeneratorCallable>, Immutable, Nullable;
            BINARY_NUMERIC_ENTRY: GcFunctionRef<BinaryNumericCallback>, Immutable, Nullable;
        }
        struct FunctionObject: JavaScriptObject => FunctionObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            CODE: GcRef<ExecutableCode>, Immutable, NonNullable;
            CONTEXT: GcRef<FunctionContext>, Immutable, NonNullable;
            TO_STRING: GcRef<StringValue>, Immutable, NonNullable;
            PUBLIC_PROTOTYPE_CACHE: GcRef<StoredValue>, Mutable, NonNullable;
            STRICT: bool, Immutable, NonNullable;
            CLASS_HERITAGE: scriptw_ir::ClassHeritageKind, Immutable, NonNullable;
            DERIVED_CONSTRUCTOR: bool, Immutable, NonNullable;
            SYNTHETIC_DEFAULT_DERIVED: bool, Immutable, NonNullable;
            USES_SUPER: bool, Immutable, NonNullable;
            THIS_BEFORE_SUPER: bool, Immutable, NonNullable;
            IS_HTMLDDA: bool, Immutable, NonNullable;
            TYPED_ARRAY_ELEMENT_KIND: Option<crate::module::TypedArrayElementKind>, Immutable, NonNullable;
            LEGACY_CALLER: GcRef<LegacyReflectionPropertyState>, Mutable, Nullable;
            LEGACY_ARGUMENTS: GcRef<LegacyReflectionPropertyState>, Mutable, Nullable;
        }
        // Only the original optional legacy data properties own publication.
        // Their last StoredValue is a mutation token, not a JavaScript value
        // proof. A successful user definition rebuilds it; deletion changes
        // the selected descriptor. Retiring both edges is permanent.
        struct LegacyReflectionPropertyState => LegacyReflectionPropertyStateSchema {
            DESCRIPTOR: GcRef<PropertyDescriptor>, Mutable, Nullable;
            EXPECTED_VALUE: GcRef<StoredValue>, Mutable, Nullable;
        }
        // One physical P instance's code/data owner. These references are never
        // published through the shared runtime's mutable program globals.
        struct ProgramInstance => ProgramInstanceSchema {
            PREPARED_DYNAMIC_FUNCTION: GcFunctionRef<PreparedDynamicFunctionHook>, Immutable, Nullable;
            PREPARED_SCRIPT: GcFunctionRef<PreparedScriptHook>, Immutable, Nullable;
            REALM_MODULE_IMPORT: GcFunctionRef<RealmModuleImportHook>, Immutable, Nullable;
            MODULE_BODY_REACTION: GcFunctionRef<ModuleBodyReactionHook>, Immutable, Nullable;
            REGEXP_PROGRAM_CANDIDATE: GcFunctionRef<RegExpProgramCandidateHook>, Immutable, Nullable;
            LITERAL_STRINGS: GcRef<PooledStringTable>, Immutable, NonNullable;
            LITERAL_BASE: I32Value, Immutable, NonNullable;
            LITERAL_COUNT: I32Value, Immutable, NonNullable;
            METADATA: GcRef<ImmutableByteArray>, Immutable, NonNullable;
            LANGUAGE_FEATURE_POLICY: scriptw_ir::LanguageFeaturePolicy, Immutable, NonNullable;
            NATIVE_SOURCE_PAYLOAD: GcRef<NativeSourcePayloadArray>, Immutable, NonNullable;
            COMPACT_SHAPES: GcRef<CompactShapePageTable>, Mutable, Nullable;
            OWN_DATA_READ_CACHES: GcRef<OwnDataReadCachePageTable>, Mutable, Nullable;
        }
        // Completed captures are immutable. The actual function/context cycle
        // is closed only after function allocation, so its back edge is mutable.
        struct FunctionContext => FunctionContextSchema {
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            LEXICAL_ENVIRONMENT: GcRef<Environment>, Immutable, Nullable;
            PRIVATE_ENVIRONMENT: GcRef<PrivateEnvironment>, Immutable, Nullable;
            ACTIVE_FUNCTION: GcRef<FunctionObject>, Mutable, Nullable;
            HOME_OBJECT: GcRef<StoredValue>, Immutable, NonNullable;
            FIELD_KEYS: GcRef<ValueArray>, Immutable, Nullable;
            TEMPLATE_SOURCE: GcRef<TemplateSource>, Immutable, Nullable;
            BUILTIN_CAPTURE: GcRef<BuiltinClosureCapture>, Immutable, Nullable;
            INSTANCE_PRIVATE_METHODS: GcRef<PrivateElementTable>, Immutable, Nullable;
            // JavaScript captures Some(defining P); R-native code explicitly
            // inherits the caller's effective program operand on each entry.
            DEFINING_PROGRAM: GcRef<ProgramInstance>, Immutable, Nullable;
            INSTANCE_ELEMENTS_INITIALIZER: GcFunctionRef<InitializeInstanceElementsHook>, Immutable, Nullable;
        }
        struct BoundFunction: JavaScriptObject => BoundFunctionSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            TARGET: GcRef<StoredValue>, Immutable, NonNullable;
            THIS_VALUE: GcRef<StoredValue>, Immutable, NonNullable;
            ARGUMENTS: GcRef<ValueArray>, Immutable, NonNullable;
            CONSTRUCTABLE: bool, Immutable, NonNullable;
        }
        struct ProxyObject: JavaScriptObject => ProxyObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            TARGET: GcRef<StoredValue>, Mutable, NonNullable;
            HANDLER: GcRef<StoredValue>, Mutable, NonNullable;
            CALL_CAPABILITY: ProxyCallCapability, Immutable, NonNullable;
        }
        struct PrimitiveBox: JavaScriptObject => PrimitiveBoxSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            PRIMITIVE: GcRef<StoredValue>, Immutable, NonNullable;
        }
        struct ArgumentsObject: JavaScriptObject => ArgumentsObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            INDEXED: GcRef<IndexedTable>, Mutable, NonNullable;
            PARAMETER_MAP: GcRef<ArgumentsParameterMap>, Immutable, Nullable;
        }
        struct DateObject: JavaScriptObject => DateObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            TIME_VALUE: F64Value, Mutable, NonNullable;
        }
        struct ShadowRealmObject: JavaScriptObject => ShadowRealmObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
        }
        struct RegExpObject: JavaScriptObject => RegExpObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            SOURCE: GcRef<StringValue>, Mutable, NonNullable;
            ORIGINAL_FLAGS: GcRef<StringValue>, Mutable, NonNullable;
            PROGRAM: GcRef<RegExpProgram>, Mutable, Nullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            LEGACY_FEATURES_ENABLED: bool, Immutable, NonNullable;
        }
        // Each nullable String is either an available value (including empty)
        // or the proposal's invalidated empty internal-slot marker.
        struct RegExpLegacyState => RegExpLegacyStateSchema {
            INPUT: GcRef<StringValue>, Mutable, Nullable;
            LAST_MATCH: GcRef<StringValue>, Mutable, Nullable;
            LAST_PAREN: GcRef<StringValue>, Mutable, Nullable;
            LEFT_CONTEXT: GcRef<StringValue>, Mutable, Nullable;
            RIGHT_CONTEXT: GcRef<StringValue>, Mutable, Nullable;
            PAREN1: GcRef<StringValue>, Mutable, Nullable;
            PAREN2: GcRef<StringValue>, Mutable, Nullable;
            PAREN3: GcRef<StringValue>, Mutable, Nullable;
            PAREN4: GcRef<StringValue>, Mutable, Nullable;
            PAREN5: GcRef<StringValue>, Mutable, Nullable;
            PAREN6: GcRef<StringValue>, Mutable, Nullable;
            PAREN7: GcRef<StringValue>, Mutable, Nullable;
            PAREN8: GcRef<StringValue>, Mutable, Nullable;
            PAREN9: GcRef<StringValue>, Mutable, Nullable;
        }
        struct RegExpLegacyAccessorContext => RegExpLegacyAccessorContextSchema {
            SLOT: RegExpLegacySlot, Immutable, NonNullable;
        }
        struct Environment => EnvironmentSchema {
            // Only a global Environment carries this defining-Realm edge.
            DEFINING_REALM: GcRef<RealmRecord>, Immutable, Nullable;
            NAMED_BINDINGS: GcRef<NamedBindingTable>, Mutable, Nullable;
            KIND: I32Value, Immutable, NonNullable;
            PARENT: GcRef<Environment>, Immutable, Nullable;
            // Parameter initialization precedes publication of the resumable
            // function-body environment. This strong edge starts absent and is
            // filled once the actual body record exists; ordinary scopes keep null.
            FUNCTION_BODY: GcRef<Environment>, Mutable, Nullable;
            DECLARATIVE: GcRef<DeclarativeEnvironment>, Immutable, Nullable;
            OBJECT: GcRef<ObjectEnvironment>, Immutable, Nullable;
            MODULE: GcRef<ModuleEnvironment>, Immutable, Nullable;
            // A direct Script publishes its caller context after allocating
            // this activation. Capturing arrows retain this exact Environment
            // through their recorded lexical hops; other scopes keep null.
            DIRECT_EVAL_CONTEXT: GcRef<DirectEvalExecutionContext>, Mutable, Nullable;
        }
        struct DeclarativeEnvironment => DeclarativeEnvironmentSchema { CELLS: GcRef<BindingCellTable>, Immutable, NonNullable; }
        struct ObjectEnvironment => ObjectEnvironmentSchema {
            BINDING_OBJECT_CELL: GcRef<BindingCell>, Immutable, NonNullable;
            IS_WITH: bool, Immutable, NonNullable;
        }
        struct ModuleEnvironment => ModuleEnvironmentSchema {
            LOCAL_CELLS: GcRef<BindingCellTable>, Immutable, NonNullable;
            IMPORT_CELLS: GcRef<BindingCellTable>, Immutable, NonNullable;
        }
        struct BindingCell => BindingCellSchema {
            VALUE: GcRef<StoredValue>, Mutable, NonNullable;
            INITIALIZED: bool, Mutable, NonNullable;
            MUTABLE: bool, Immutable, NonNullable;
            IMMUTABLE_STRICT: bool, Immutable, NonNullable;
            // Module linking fills this edge after named entries have already
            // retained the importing cell. Own initialization/const metadata
            // remains distinct from the ultimate exporter's TDZ/value state.
            TARGET: GcRef<BindingCell>, Mutable, Nullable;
            // Compiler-private ArgumentListEvaluation output, never a JS value.
            ARGUMENT_LIST: GcRef<ValueArray>, Mutable, Nullable;
            CAPTURED_ENVIRONMENT: GcRef<Environment>, Mutable, Nullable;
            SYNC_DISPOSE_CAPABILITY: GcRef<ActivationSyncDisposeCapability>, Mutable, Nullable;
            ASYNC_DISPOSE_CAPABILITY: GcRef<ActivationAsyncDisposeCapability>, Mutable, Nullable;
            // Validated ArrayAccumulatorU64NextIndexSlot state; never a JS Number.
            ARRAY_ACCUMULATION_INDEX: I64Value, Mutable, NonNullable;
            // Compiler-private suspended Reference Record, never a JS value.
            CAPTURED_IDENTIFIER_REFERENCE: GcRef<EnvironmentIdentifierReferenceRecord>, Mutable, Nullable;
            // Actual suspended array-pattern IteratorRecord, never a JS value.
            DESTRUCTURING_ITERATOR_RECORD: GcRef<IteratorRecord>, Mutable, Nullable;
            FOR_IN_ENUMERATION_RECORD: GcRef<ForInEnumerationRecord>, Mutable, Nullable;
        }
        // Capture the exact selected record or cell before the RHS suspends.
        // Resumption restores this record without traversing an altered chain.
        struct EnvironmentIdentifierReferenceRecord => EnvironmentIdentifierReferenceRecordSchema {
            KEY: GcRef<StringValue>, Immutable, NonNullable;
            KIND: I32Value, Immutable, NonNullable;
            RECORD: GcRef<Environment>, Immutable, Nullable;
            ENTRY: GcRef<NamedBinding>, Immutable, Nullable;
            CELL: GcRef<BindingCell>, Immutable, Nullable;
            BASE: GcRef<StoredValue>, Immutable, NonNullable;
        }
        // Local binding metadata never travels with an aliased import cell.
        struct NamedBinding => NamedBindingSchema {
            NAME: GcRef<StringValue>, Immutable, NonNullable;
            CELL: GcRef<BindingCell>, Immutable, NonNullable;
            DELETABLE: bool, Immutable, NonNullable;
            PRESENT: bool, Mutable, NonNullable;
            LEXICAL_CONFLICT: bool, Immutable, NonNullable;
        }
        array NamedBindingTable => NamedBindingTableSchema { ELEMENT: GcRef<NamedBinding>, Mutable, Nullable; }
        array BindingCellTable => BindingCellTableSchema { ELEMENT: GcRef<BindingCell>, Mutable, NonNullable; }
        array ArgumentsParameterMap => ArgumentsParameterMapSchema { ELEMENT: GcRef<BindingCell>, Mutable, Nullable; }
        struct PrivateEnvironment => PrivateEnvironmentSchema {
            PARENT: GcRef<PrivateEnvironment>, Immutable, Nullable;
            CLASS_SCOPE: I64Value, Immutable, NonNullable;
            NAMES: GcRef<PrivateNameTable>, Immutable, NonNullable;
        }
        struct StringValue => StringValueSchema {
            CODE_UNITS: GcRef<CodeUnitArray>, Immutable, NonNullable;
            // Raw UTF-16 hash, before the property/collection key tag mix.
            // Zero is unprepared; a real zero hash is safely recomputed.
            RAW_KEY_HASH: I64Value, Mutable, NonNullable;
        }
        array CodeUnitArray => CodeUnitArraySchema { ELEMENT: I16Value, Mutable, NonNullable; }
        struct BigIntValue => BigIntValueSchema {
            NEGATIVE: bool, Immutable, NonNullable;
            LIMBS: GcRef<BigIntLimbArray>, Immutable, NonNullable;
        }
        array PooledStringTable => PooledStringTableSchema { ELEMENT: GcRef<StringValue>, Mutable, Nullable; }
        array WellKnownSymbolTable => WellKnownSymbolTableSchema { ELEMENT: GcRef<SymbolValue>, Immutable, NonNullable; }
        array RegisteredSymbolTable => RegisteredSymbolTableSchema { ELEMENT: GcRef<SymbolValue>, Mutable, NonNullable; }
        array BigIntLimbArray => BigIntLimbArraySchema { ELEMENT: I64Value, Mutable, NonNullable; }
        struct SymbolValue => SymbolValueSchema {
            DESCRIPTION: GcRef<StringValue>, Immutable, Nullable;
            REGISTRY_KEY: GcRef<StringValue>, Immutable, Nullable;
            KEY_HASH_ID: I64Value, Mutable, NonNullable;
        }
        struct ArrayObject: JavaScriptObject => ArrayObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ELEMENTS: GcRef<ArrayIndexStorage>, Immutable, NonNullable;
            LENGTH: I64Value, Mutable, NonNullable;
            LENGTH_WRITABLE: bool, Mutable, NonNullable;
        }
        struct ArrayBuffer: JavaScriptObject => ArrayBufferSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            // Private backing rounds physical extent up to eight bytes; null is detached.
            BYTES: GcRef<ByteArray>, Mutable, Nullable;
            MAX_BYTE_LENGTH: I64Value, Immutable, NonNullable;
            DETACH_KEY: GcRef<StoredValue>, Mutable, NonNullable;
            RESIZABLE: bool, Immutable, NonNullable;
            IMMUTABLE: bool, Immutable, NonNullable;
            // Exact active byte length is independent of rounded backing extent.
            BYTE_LENGTH: I64Value, Mutable, NonNullable;
        }
        array ByteArray => ByteArraySchema { ELEMENT: I8Value, Mutable, NonNullable; }
        struct SharedArrayBuffer: JavaScriptObject => SharedArrayBufferSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            BACKING_RESOURCE: GcRef<HostResource>, Immutable, NonNullable;
        }
        struct BufferView => BufferViewSchema {
            BUFFER: GcRef<BufferOwner>, Immutable, NonNullable;
            BYTE_OFFSET: I64Value, Immutable, NonNullable;
            FIXED_BYTE_LENGTH: I64Value, Immutable, NonNullable;
            LENGTH_TRACKING: bool, Immutable, NonNullable;
        }
        struct DataViewObject: JavaScriptObject => DataViewObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            VIEW: GcRef<BufferView>, Immutable, NonNullable;
        }
        struct TypedArrayObject: JavaScriptObject => TypedArrayObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            VIEW: GcRef<BufferView>, Immutable, NonNullable;
            ELEMENT_KIND: crate::module::TypedArrayElementKind, Immutable, NonNullable;
        }
        // Private ForIn cursor. Null current denotes completion; absent keys
        // means this prototype level has not observed OwnPropertyKeys yet.
        struct ForInEnumerationRecord => ForInEnumerationRecordSchema {
            CURRENT: GcRef<StoredValue>, Mutable, NonNullable;
            REMAINING_KEYS: GcRef<PropertyKeyTable>, Mutable, Nullable;
            NEXT_KEY: I32Value, Mutable, NonNullable;
            VISITED_KEYS: GcRef<PropertyKeyTable>, Mutable, NonNullable;
        }
        struct IteratorRecord => IteratorRecordSchema {
            ITERATOR: GcRef<StoredValue>, Immutable, NonNullable;
            NEXT_METHOD: GcRef<StoredValue>, Immutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
        }
        struct CompletionRecord => CompletionRecordSchema {
            KIND: crate::emit::CompletionKind, Mutable, NonNullable;
            VALUE: GcRef<StoredValue>, Mutable, NonNullable;
            TARGET: I32Value, Mutable, NonNullable;
            NEXT: GcRef<CompletionRecord>, Mutable, Nullable;
            RETURN_STAGE: AsyncGeneratorReturnStage, Immutable, NonNullable;
        }
        struct InvocationFrame => InvocationFrameSchema {
            FUNCTION: GcRef<FunctionObject>, Immutable, NonNullable;
            ENVIRONMENT: GcRef<Environment>, Mutable, Nullable;
            LEXICAL_ENVIRONMENT: GcRef<Environment>, Mutable, Nullable;
            INVOCATION_ENVIRONMENT: GcRef<Environment>, Mutable, Nullable;
            THIS_VALUE: GcRef<StoredValue>, Immutable, NonNullable;
            NEW_TARGET: GcRef<StoredValue>, Immutable, NonNullable;
            ARGUMENTS: GcRef<ValueArray>, Immutable, NonNullable;
            LOCALS: GcRef<ValueArray>, Mutable, NonNullable;
            PRIVATE_ARGUMENT_LISTS: GcRef<PrivateArgumentListTable>, Mutable, NonNullable;
            PENDING_COMPLETION: GcRef<CompletionRecord>, Mutable, Nullable;
            PENDING_COMPLETION_DEPTH: I64Value, Mutable, NonNullable;
            ASSIGNMENT_TARGET: GcRef<StoredValue>, Mutable, NonNullable;
            ASSIGNMENT_KEY: GcRef<StoredValue>, Mutable, NonNullable;
            INITIALIZED: bool, Mutable, NonNullable;
            FOR_AWAIT_ITERATORS: GcRef<ForAwaitIteratorTable>, Mutable, Nullable;
        }
        struct GeneratorActivation => GeneratorActivationSchema {
            FRAME: GcRef<InvocationFrame>, Immutable, NonNullable;
            RESUME_VALUE: GcRef<StoredValue>, Mutable, NonNullable;
            RESUME_KIND: crate::heap::GeneratorResumeKind, Mutable, NonNullable;
            DELEGATE: GcRef<GeneratorDelegate>, Mutable, Nullable;
            RESUME_POINT: I32Value, Mutable, NonNullable;
            STATUS: crate::heap::GeneratorState, Mutable, NonNullable;
        }
        struct GeneratorObject: JavaScriptObject => GeneratorObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ACTIVATION: GcRef<GeneratorActivation>, Immutable, NonNullable;
        }
        struct AsyncActivation => AsyncActivationSchema {
            FRAME: GcRef<InvocationFrame>, Immutable, NonNullable;
            RESUME_VALUE: GcRef<StoredValue>, Mutable, NonNullable;
            RESUME_COMPLETION: AwaitCompletionKind, Mutable, NonNullable;
            PROMISE: GcRef<PromiseObject>, Immutable, NonNullable;
            RESUME_POINT: I32Value, Mutable, NonNullable;
            COMPLETED: bool, Mutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            MODULE_ENTRY_MODE: crate::heap::AsyncModuleEntryMode, Mutable, NonNullable;
        }
        struct AsyncGeneratorActivation => AsyncGeneratorActivationSchema {
            FRAME: GcRef<InvocationFrame>, Immutable, NonNullable;
            RESUME_VALUE: GcRef<StoredValue>, Mutable, NonNullable;
            RESUME_KIND: crate::heap::AsyncGeneratorResumeKind, Mutable, NonNullable;
            DELEGATE: GcRef<GeneratorDelegate>, Mutable, Nullable;
            REQUEST_HEAD: GcRef<AsyncGeneratorRequest>, Mutable, Nullable;
            REQUEST_TAIL: GcRef<AsyncGeneratorRequest>, Mutable, Nullable;
            ACTIVE_REQUEST: GcRef<AsyncGeneratorRequest>, Mutable, Nullable;
            RESUME_POINT: I32Value, Mutable, NonNullable;
            EXECUTION_STATE: crate::heap::AsyncGeneratorExecutionState, Mutable, NonNullable;
            BODY_STATUS: crate::heap::AsyncGeneratorBodyStatus, Mutable, NonNullable;
            BODY_RESULT: GcRef<StoredValue>, Mutable, NonNullable;
            RETURN_STAGE: AsyncGeneratorReturnStage, Mutable, NonNullable;
        }
        struct AsyncGeneratorObject: JavaScriptObject => AsyncGeneratorObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ACTIVATION: GcRef<AsyncGeneratorActivation>, Immutable, NonNullable;
        }
        struct AsyncGeneratorRequest => AsyncGeneratorRequestSchema {
            COMPLETION_KIND: crate::heap::AsyncGeneratorRequestCompletionKind, Immutable, NonNullable;
            COMPLETION_VALUE: GcRef<StoredValue>, Immutable, NonNullable;
            CAPABILITY: GcRef<PromiseCapability>, Immutable, NonNullable;
            PROMISE: GcRef<PromiseObject>, Immutable, NonNullable;
            NEXT: GcRef<AsyncGeneratorRequest>, Mutable, Nullable;
        }
        struct PromiseObject: JavaScriptObject => PromiseObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RESULT: GcRef<StoredValue>, Mutable, NonNullable;
            FULFILL_REACTIONS: GcRef<PromiseReaction>, Mutable, Nullable;
            REJECT_REACTIONS: GcRef<PromiseReaction>, Mutable, Nullable;
            STATE: crate::heap::PromiseState, Mutable, NonNullable;
            HANDLED: bool, Mutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            HOST_DATA: GcRef<HostResource>, Mutable, Nullable;
            UNHANDLED_NEXT: GcRef<PromiseObject>, Mutable, Nullable;
        }
        struct PromiseCapability => PromiseCapabilitySchema {
            PROMISE: GcRef<StoredValue>, Immutable, NonNullable;
            RESOLVE: GcRef<StoredValue>, Immutable, NonNullable;
            REJECT: GcRef<StoredValue>, Immutable, NonNullable;
        }
        struct AsyncFromSyncIteratorContinuation => AsyncFromSyncIteratorContinuationSchema {
            PROMISE: GcRef<PromiseObject>, Immutable, NonNullable;
            ITERATOR: GcRef<IteratorRecord>, Immutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            DONE: bool, Immutable, NonNullable;
            CLOSE_ON_REJECTION: bool, Immutable, NonNullable;
        }
        struct PromiseReaction => PromiseReactionSchema {
            TYPE: crate::heap::PromiseReactionType, Immutable, NonNullable;
            CALLBACK_KIND: crate::heap::PromiseReactionCallbackKind, Immutable, NonNullable;
            HANDLER: GcRef<StoredValue>, Immutable, NonNullable;
            CAPABILITY: GcRef<PromiseCapability>, Immutable, Nullable;
            REALM: GcRef<RealmRecord>, Immutable, Nullable;
            ASYNC_ACTIVATION: GcRef<AsyncActivation>, Immutable, Nullable;
            ASYNC_GENERATOR: GcRef<AsyncGeneratorActivation>, Immutable, Nullable;
            MODULE: GcRef<ModuleRecord>, Immutable, Nullable;
            MODULE_JOIN: GcRef<ModuleJoin>, Immutable, Nullable;
            ASYNC_FROM_SYNC: GcRef<AsyncFromSyncIteratorContinuation>, Immutable, Nullable;
            NEXT: GcRef<PromiseReaction>, Mutable, Nullable;
        }
        struct PendingJob => PendingJobSchema {
            KIND: crate::heap::PromiseJobKind, Immutable, NonNullable;
            REACTION: GcRef<PromiseReaction>, Immutable, Nullable;
            THENABLE: GcRef<PromiseThenableJob>, Immutable, Nullable;
            REGISTRY: GcRef<FinalizationRegistryObject>, Immutable, Nullable;
            ARGUMENT: GcRef<StoredValue>, Immutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, Nullable;
            NEXT: GcRef<PendingJob>, Mutable, Nullable;
        }
        // CreateRealm allocates an empty record before installing globals and
        // the environment's strong back edge to this Realm.
        struct RealmRecord => RealmRecordSchema {
            REALM_ID: I64Value, Immutable, NonNullable;
            AGENT_ID: I64Value, Immutable, NonNullable;
            GLOBAL_OBJECT: GcRef<StoredValue>, Mutable, Nullable;
            GLOBAL_THIS: GcRef<StoredValue>, Mutable, Nullable;
            GLOBAL_ENVIRONMENT: GcRef<Environment>, Mutable, Nullable;
            INTRINSICS: GcRef<IntrinsicTable>, Immutable, NonNullable;
            HOST_HOOKS: GcRef<HostResource>, Immutable, Nullable;
            MODULES: GcRef<ModuleGraph>, Mutable, Nullable;
            TEMPLATE_REGISTRY: GcRef<TemplateSource>, Mutable, Nullable;
            REGEXP_LEGACY: GcRef<RegExpLegacyState>, Immutable, NonNullable;
            TEMPORAL_STATE: crate::heap::TemporalInitializationState, Mutable, NonNullable;
            TEMPORAL_NAMESPACE: GcRef<StoredValue>, Mutable, Nullable;
            INTL_STATE: crate::heap::IntlInitializationState, Mutable, NonNullable;
            INTL_NAMESPACE: GcRef<StoredValue>, Mutable, Nullable;
        }
        // Opt-in host observations retain actual Realm records without adding
        // a JavaScript property or consulting mutable globals/prototypes.
        struct SnapshotRealmInventory => SnapshotRealmInventorySchema {
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            PREVIOUS: GcRef<SnapshotRealmInventory>, Immutable, Nullable;
        }
        array IntrinsicTable => IntrinsicTableSchema { ELEMENT: GcRef<StoredValue>, Mutable, NonNullable; }
        struct ModuleRecord => ModuleRecordSchema {
            GRAPH: GcRef<ModuleGraph>, Immutable, NonNullable;
            FUNCTION: GcRef<FunctionObject>, Mutable, Nullable;
            REALM: GcRef<RealmRecord>, Mutable, Nullable;
            ENVIRONMENT: GcRef<Environment>, Mutable, Nullable;
            NAMESPACE_CELL: GcRef<BindingCell>, Immutable, NonNullable;
            DEFERRED_NAMESPACE_CELL: GcRef<BindingCell>, Immutable, NonNullable;
            SOURCE_CELL: GcRef<BindingCell>, Immutable, NonNullable;
            ACTIVATION_KIND: crate::heap::ModuleActivationKind, Mutable, NonNullable;
            GENERATOR: GcRef<GeneratorActivation>, Mutable, Nullable;
            ASYNC_ACTIVATION: GcRef<AsyncActivation>, Mutable, Nullable;
            REQUESTS: GcRef<ModuleRequestTable>, Mutable, Nullable;
            CYCLE_ROOT: GcRef<ModuleRecord>, Mutable, Nullable;
            EVALUATION_PROMISE: GcRef<PromiseObject>, Mutable, Nullable;
            STATE: crate::heap::ModuleEvaluationState, Mutable, NonNullable;
            COMPLETION: crate::heap::ModuleEvaluationCompletion, Mutable, NonNullable;
            BODY_STATE: crate::heap::ModuleBodyState, Mutable, NonNullable;
            ERROR: GcRef<StoredValue>, Mutable, NonNullable;
            DFS_INDEX: I64Value, Mutable, NonNullable;
            DFS_ANCESTOR: I64Value, Mutable, NonNullable;
            ASYNC_ORDER: I64Value, Mutable, NonNullable;
            PENDING_ASYNC_DEPENDENCIES: I64Value, Mutable, NonNullable;
            ASYNC_PARENTS_HEAD: GcRef<ModuleParent>, Mutable, Nullable;
            ASYNC_PARENTS_TAIL: GcRef<ModuleParent>, Mutable, Nullable;
        }
        array ModuleRegistry => ModuleRegistrySchema { ELEMENT: GcRef<ModuleRecord>, Mutable, Nullable; }
        struct HostResource => HostResourceSchema {
            KIND: HostResourceKind, Immutable, NonNullable;
            RESOURCE: ExternalResourceRef, Immutable, NonNullable;
        }
        struct TemplateSource => TemplateSourceSchema {
            SOURCE_ID: I64Value, Immutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            SITES: GcRef<TemplateSiteTable>, Immutable, NonNullable;
            NEXT: GcRef<TemplateSource>, Mutable, Nullable;
        }
        array TemplateSiteTable => TemplateSiteTableSchema { ELEMENT: GcRef<ArrayObject>, Mutable, Nullable; }
        struct MapObject: JavaScriptObject => MapObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ENTRIES: GcRef<MapEntryTable>, Mutable, NonNullable;
            LIVE_COUNT: I64Value, Mutable, NonNullable;
            HISTORY_LENGTH: I64Value, Mutable, NonNullable;
            HASH_INDEX: GcRef<CollectionHashTable>, Mutable, NonNullable;
            ORDINALS: GcRef<NumberBitsI64Array>, Mutable, NonNullable;
            NEXT_ORDINAL: I64Value, Mutable, NonNullable;
            VERSION: I64Value, Mutable, NonNullable;
        }
        // Native resources contain collector slot IDs, never strong JS keys.
        struct WeakMapObject: JavaScriptObject => WeakMapObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RESOURCE: GcRef<HostResource>, Immutable, NonNullable;
        }
        struct WeakSetObject: JavaScriptObject => WeakSetObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RESOURCE: GcRef<HostResource>, Immutable, NonNullable;
        }
        // Prepared unpublished records acquire their weak resource before use.
        struct WeakRefObject: JavaScriptObject => WeakRefObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RESOURCE: GcRef<HostResource>, Mutable, Nullable;
            TARGET_TAG: I32Value, Immutable, NonNullable;
        }
        struct FinalizationRegistryObject: JavaScriptObject => FinalizationRegistryObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            CALLBACK: GcRef<StoredValue>, Immutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            CELLS: GcRef<FinalizationCell>, Mutable, Nullable;
            CLEANUP_QUEUED: bool, Mutable, NonNullable;
        }
        struct FinalizationCell => FinalizationCellSchema {
            HOLDINGS: GcRef<StoredValue>, Immutable, NonNullable;
            RESOURCE: GcRef<HostResource>, Mutable, Nullable;
            NEXT: GcRef<FinalizationCell>, Mutable, Nullable;
        }
        struct MapEntry => MapEntrySchema {
            KEY: GcRef<StoredValue>, Immutable, NonNullable;
            VALUE: GcRef<StoredValue>, Mutable, NonNullable;
            HASH_NEXT: I64Value, Mutable, NonNullable;
        }
        array MapEntryTable => MapEntryTableSchema { ELEMENT: GcRef<MapEntry>, Mutable, Nullable; }
        struct SetObject: JavaScriptObject => SetObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ENTRIES: GcRef<SetEntryTable>, Mutable, NonNullable;
            LIVE_COUNT: I64Value, Mutable, NonNullable;
            HISTORY_LENGTH: I64Value, Mutable, NonNullable;
            HASH_INDEX: GcRef<CollectionHashTable>, Mutable, NonNullable;
            ORDINALS: GcRef<NumberBitsI64Array>, Mutable, NonNullable;
            NEXT_ORDINAL: I64Value, Mutable, NonNullable;
            VERSION: I64Value, Mutable, NonNullable;
        }
        struct SetEntry => SetEntrySchema {
            VALUE: GcRef<StoredValue>, Immutable, NonNullable;
            HASH_NEXT: I64Value, Mutable, NonNullable;
        }
        array SetEntryTable => SetEntryTableSchema { ELEMENT: GcRef<SetEntry>, Mutable, Nullable; }
        // Bucket heads and links are entry-index+1; zero denotes no entry.
        array CollectionHashTable => CollectionHashTableSchema { ELEMENT: I64Value, Mutable, NonNullable; }
        struct DisposableResource => DisposableResourceSchema {
            KIND: crate::heap::DisposableStackEntryKind, Immutable, NonNullable;
            VALUE: GcRef<StoredValue>, Immutable, NonNullable;
            METHOD: GcRef<StoredValue>, Immutable, NonNullable;
        }
        array DisposableResourceTable => DisposableResourceTableSchema { ELEMENT: GcRef<DisposableResource>, Mutable, Nullable; }
        struct DisposableStack: JavaScriptObject => DisposableStackSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RESOURCE_STACK: GcRef<DisposableResourceStack>, Mutable, NonNullable;
            STATE: crate::heap::DisposableStackState, Mutable, NonNullable;
        }
        // AddDisposableResource retains List identity across observable GetMethod.
        struct DisposableResourceStack => DisposableResourceStackSchema {
            RESOURCES: GcRef<DisposableResourceTable>, Mutable, NonNullable;
            ENTRY_COUNT: I64Value, Mutable, NonNullable;
        }
        // These immutable configurations come from completed Intl construction
        // and provider-response reads, rather than a generic scalar state array.
        struct IntlLocaleObject: JavaScriptObject => IntlLocaleObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            TAG: GcRef<StringValue>, Immutable, NonNullable;
            LANGUAGE: GcRef<StringValue>, Immutable, NonNullable;
            SCRIPT: GcRef<StringValue>, Immutable, Nullable;
            REGION: GcRef<StringValue>, Immutable, Nullable;
            BASE_NAME: GcRef<StringValue>, Immutable, NonNullable;
        }
        struct IntlCollatorObject: JavaScriptObject => IntlCollatorObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            COLLATION: GcRef<StringValue>, Immutable, NonNullable;
            USAGE: scriptw_intl::CollatorUsage, Immutable, NonNullable;
            SENSITIVITY: scriptw_intl::CollatorSensitivity, Immutable, NonNullable;
            CASE_FIRST: scriptw_intl::CollatorCaseFirst, Immutable, NonNullable;
            COLLATION_KIND: scriptw_intl::CollatorCollationKind, Immutable, NonNullable;
            NUMERIC: bool, Immutable, NonNullable;
            IGNORE_PUNCTUATION: bool, Immutable, NonNullable;
            BOUND_COMPARE: GcRef<FunctionObject>, Mutable, Nullable;
        }
        struct IntlNumberFormatObject: JavaScriptObject => IntlNumberFormatObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            DATA_LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            NUMBERING_SYSTEM: GcRef<StringValue>, Immutable, NonNullable;
            STYLE: scriptw_intl::number_format::options::StyleOption, Immutable, NonNullable;
            STYLE_TEXT: GcRef<StringValue>, Immutable, NonNullable;
            CURRENCY_DISPLAY: Option<scriptw_intl::number_format::options::CurrencyDisplay>, Immutable, NonNullable;
            CURRENCY_SIGN: Option<scriptw_intl::number_format::options::CurrencySign>, Immutable, NonNullable;
            UNIT_DISPLAY: Option<scriptw_intl::number_format::options::UnitDisplay>, Immutable, NonNullable;
            NOTATION: scriptw_intl::number_format::options::NotationOption, Immutable, NonNullable;
            COMPACT_DISPLAY: Option<scriptw_intl::number_format::options::CompactDisplay>, Immutable, NonNullable;
            ROUNDING: GcRef<IntlNumberRounding>, Immutable, NonNullable;
            GROUPING: scriptw_intl::number_format::options::Grouping, Immutable, NonNullable;
            SIGN_DISPLAY: scriptw_intl::number_format::options::SignDisplay, Immutable, NonNullable;
            BOUND_FORMAT: GcRef<FunctionObject>, Mutable, Nullable;
        }
        struct IntlNumberRounding => IntlNumberRoundingSchema {
            MINIMUM_INTEGER: scriptw_intl::number_format::options::IntegerDigitCount, Immutable, NonNullable;
            PRECISION: scriptw_intl::NumberPrecisionKind, Immutable, NonNullable;
            MINIMUM_FRACTION: Option<scriptw_intl::number_format::options::FractionDigitCount>, Immutable, NonNullable;
            MAXIMUM_FRACTION: Option<scriptw_intl::number_format::options::FractionDigitCount>, Immutable, NonNullable;
            MINIMUM_SIGNIFICANT: Option<scriptw_intl::number_format::options::SignificantDigitCount>, Immutable, NonNullable;
            MAXIMUM_SIGNIFICANT: Option<scriptw_intl::number_format::options::SignificantDigitCount>, Immutable, NonNullable;
            ROUNDING_INCREMENT: scriptw_intl::number_format::options::RoundingIncrement, Immutable, NonNullable;
            ROUNDING_MODE: scriptw_intl::number_format::options::RoundingMode, Immutable, NonNullable;
            TRAILING_ZERO: scriptw_intl::number_format::options::TrailingZeroDisplay, Immutable, NonNullable;
        }
        struct IntlPluralRulesObject: JavaScriptObject => IntlPluralRulesObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            DATA_LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            TYPE: scriptw_intl::PluralType, Immutable, NonNullable;
            CATEGORIES: scriptw_intl::PluralCategorySet, Immutable, NonNullable;
            NOTATION: scriptw_intl::number_format::options::NotationOption, Immutable, NonNullable;
            COMPACT_DISPLAY: Option<scriptw_intl::number_format::options::CompactDisplay>, Immutable, NonNullable;
            ROUNDING: GcRef<IntlNumberRounding>, Immutable, NonNullable;
        }
        struct IntlListFormatObject: JavaScriptObject => IntlListFormatObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            TYPE: scriptw_intl::ListType, Immutable, NonNullable;
            STYLE: scriptw_intl::ListStyle, Immutable, NonNullable;
        }
        struct IntlRelativeTimeFormatObject: JavaScriptObject => IntlRelativeTimeFormatObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            FORMATTING_LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            NUMBERING_SYSTEM: GcRef<StringValue>, Immutable, NonNullable;
            STYLE: scriptw_intl::RelativeStyle, Immutable, NonNullable;
            NUMERIC: scriptw_intl::RelativeNumeric, Immutable, NonNullable;
        }
        struct IntlDisplayNamesObject: JavaScriptObject => IntlDisplayNamesObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            TYPE: scriptw_intl::DisplayNamesType, Immutable, NonNullable;
            STYLE: scriptw_intl::DisplayNamesStyle, Immutable, NonNullable;
            FALLBACK: scriptw_intl::DisplayNamesFallback, Immutable, NonNullable;
            LANGUAGE_DISPLAY: Option<scriptw_intl::DisplayNamesLanguageDisplay>, Immutable, NonNullable;
        }
        struct IntlDurationFormatObject: JavaScriptObject => IntlDurationFormatObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            NUMBERING_SYSTEM: GcRef<StringValue>, Immutable, NonNullable;
            STYLE: scriptw_intl::DurationStyle, Immutable, NonNullable;
            FRACTIONAL_DIGITS: Option<scriptw_intl::DurationFractionalDigits>, Immutable, NonNullable;
            YEARS: GcRef<IntlDurationUnitOptions>, Immutable, NonNullable;
            MONTHS: GcRef<IntlDurationUnitOptions>, Immutable, NonNullable;
            WEEKS: GcRef<IntlDurationUnitOptions>, Immutable, NonNullable;
            DAYS: GcRef<IntlDurationUnitOptions>, Immutable, NonNullable;
            HOURS: GcRef<IntlDurationUnitOptions>, Immutable, NonNullable;
            MINUTES: GcRef<IntlDurationUnitOptions>, Immutable, NonNullable;
            SECONDS: GcRef<IntlDurationUnitOptions>, Immutable, NonNullable;
            MILLISECONDS: GcRef<IntlDurationUnitOptions>, Immutable, NonNullable;
            MICROSECONDS: GcRef<IntlDurationUnitOptions>, Immutable, NonNullable;
            NANOSECONDS: GcRef<IntlDurationUnitOptions>, Immutable, NonNullable;
        }
        struct IntlDurationUnitOptions => IntlDurationUnitOptionsSchema {
            STYLE: scriptw_intl::DurationUnitStyle, Immutable, NonNullable;
            DISPLAY: scriptw_intl::DurationDisplay, Immutable, NonNullable;
        }
        struct IntlDateTimeFormatObject: JavaScriptObject => IntlDateTimeFormatObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            CALENDAR: GcRef<StringValue>, Immutable, NonNullable;
            NUMBERING_SYSTEM: GcRef<StringValue>, Immutable, NonNullable;
            TIME_ZONE: GcRef<StringValue>, Immutable, NonNullable;
            TIME_ZONE_FIXED_SECONDS: I64Value, Immutable, NonNullable;
            TIME_ZONE_KIND: scriptw_intl::TimeZoneKind, Immutable, NonNullable;
            HOUR_CYCLE: Option<scriptw_intl::DateTimeHourCycle>, Immutable, NonNullable;
            WEEKDAY: Option<scriptw_intl::DateTimeTextWidth>, Immutable, NonNullable;
            ERA: Option<scriptw_intl::DateTimeTextWidth>, Immutable, NonNullable;
            YEAR: Option<scriptw_intl::DateTimeNumericWidth>, Immutable, NonNullable;
            MONTH: Option<scriptw_intl::DateTimeMonthWidth>, Immutable, NonNullable;
            DAY: Option<scriptw_intl::DateTimeNumericWidth>, Immutable, NonNullable;
            DAY_PERIOD: Option<scriptw_intl::DateTimeTextWidth>, Immutable, NonNullable;
            HOUR: Option<scriptw_intl::DateTimeNumericWidth>, Immutable, NonNullable;
            MINUTE: Option<scriptw_intl::DateTimeNumericWidth>, Immutable, NonNullable;
            SECOND: Option<scriptw_intl::DateTimeNumericWidth>, Immutable, NonNullable;
            FRACTIONAL_SECOND_DIGITS: Option<scriptw_intl::DateTimeFractionalDigits>, Immutable, NonNullable;
            TIME_ZONE_NAME: Option<scriptw_intl::TimeZoneNameStyle>, Immutable, NonNullable;
            DATE_STYLE: Option<scriptw_intl::DateTimeStyle>, Immutable, NonNullable;
            TIME_STYLE: Option<scriptw_intl::DateTimeStyle>, Immutable, NonNullable;
            HOUR12: Option<bool>, Immutable, NonNullable;
            AVAILABLE_FORMATS: scriptw_intl::DateTimeFormatAvailability, Immutable, NonNullable;
            PLAN: GcRef<ImmutableByteArray>, Immutable, NonNullable;
            BOUND_FORMAT: GcRef<FunctionObject>, Mutable, Nullable;
        }
        struct IntlSegmenterObject: JavaScriptObject => IntlSegmenterObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            LOCALE: GcRef<StringValue>, Immutable, NonNullable;
            GRANULARITY: scriptw_intl::SegmenterGranularity, Immutable, NonNullable;
        }
        struct IntlSegmentsObject: JavaScriptObject => IntlSegmentsObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            SEGMENTER: GcRef<IntlSegmenterObject>, Immutable, NonNullable;
            INPUT: GcRef<StringValue>, Immutable, NonNullable;
            BOUNDARIES: GcRef<SegmentBoundaryTable>, Immutable, NonNullable;
        }
        struct SegmentBoundary => SegmentBoundarySchema {
            END: I64Value, Immutable, NonNullable;
            WORD_LIKE: Option<bool>, Immutable, NonNullable;
        }
        array SegmentBoundaryTable => SegmentBoundaryTableSchema { ELEMENT: GcRef<SegmentBoundary>, Mutable, NonNullable; }
        struct IntlSegmentIteratorObject: JavaScriptObject => IntlSegmentIteratorObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            SEGMENTS: GcRef<IntlSegmentsObject>, Immutable, NonNullable;
            NEXT_INDEX: I64Value, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
        }

        // Actual Temporal allocators publish immutable numeric fields and
        // validated calendar/zone strings. Duration fields are Number values,
        // including large integral f64 values, rather than signed i64 counters.
        struct TemporalDurationObject: JavaScriptObject => TemporalDurationObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            YEARS: F64Value, Immutable, NonNullable;
            MONTHS: F64Value, Immutable, NonNullable;
            WEEKS: F64Value, Immutable, NonNullable;
            DAYS: F64Value, Immutable, NonNullable;
            HOURS: F64Value, Immutable, NonNullable;
            MINUTES: F64Value, Immutable, NonNullable;
            SECONDS: F64Value, Immutable, NonNullable;
            MILLISECONDS: F64Value, Immutable, NonNullable;
            MICROSECONDS: F64Value, Immutable, NonNullable;
            NANOSECONDS: F64Value, Immutable, NonNullable;
        }
        struct TemporalInstantObject: JavaScriptObject => TemporalInstantObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            EPOCH_NANOSECONDS: GcRef<BigIntValue>, Immutable, NonNullable;
        }
        struct TemporalZonedDateTimeObject: JavaScriptObject => TemporalZonedDateTimeObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            EPOCH_NANOSECONDS: GcRef<BigIntValue>, Immutable, NonNullable;
            TIME_ZONE: GcRef<StringValue>, Immutable, NonNullable;
            CALENDAR: GcRef<StringValue>, Immutable, NonNullable;
        }
        struct TemporalPlainDateObject: JavaScriptObject => TemporalPlainDateObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ISO_YEAR: I32Value, Immutable, NonNullable;
            ISO_MONTH: I32Value, Immutable, NonNullable;
            ISO_DAY: I32Value, Immutable, NonNullable;
            CALENDAR: GcRef<StringValue>, Immutable, NonNullable;
        }
        struct TemporalPlainYearMonthObject: JavaScriptObject => TemporalPlainYearMonthObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ISO_YEAR: I32Value, Immutable, NonNullable;
            ISO_MONTH: I32Value, Immutable, NonNullable;
            REFERENCE_ISO_DAY: I32Value, Immutable, NonNullable;
            CALENDAR: GcRef<StringValue>, Immutable, NonNullable;
        }
        struct TemporalPlainMonthDayObject: JavaScriptObject => TemporalPlainMonthDayObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            REFERENCE_ISO_YEAR: I32Value, Immutable, NonNullable;
            ISO_MONTH: I32Value, Immutable, NonNullable;
            ISO_DAY: I32Value, Immutable, NonNullable;
            CALENDAR: GcRef<StringValue>, Immutable, NonNullable;
        }
        struct TemporalPlainTimeObject: JavaScriptObject => TemporalPlainTimeObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            HOUR: I32Value, Immutable, NonNullable;
            MINUTE: I32Value, Immutable, NonNullable;
            SECOND: I32Value, Immutable, NonNullable;
            MILLISECOND: I32Value, Immutable, NonNullable;
            MICROSECOND: I32Value, Immutable, NonNullable;
            NANOSECOND: I32Value, Immutable, NonNullable;
        }
        struct TemporalPlainDateTimeObject: JavaScriptObject => TemporalPlainDateTimeObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ISO_YEAR: I32Value, Immutable, NonNullable;
            ISO_MONTH: I32Value, Immutable, NonNullable;
            ISO_DAY: I32Value, Immutable, NonNullable;
            HOUR: I32Value, Immutable, NonNullable;
            MINUTE: I32Value, Immutable, NonNullable;
            SECOND: I32Value, Immutable, NonNullable;
            MILLISECOND: I32Value, Immutable, NonNullable;
            MICROSECOND: I32Value, Immutable, NonNullable;
            NANOSECOND: I32Value, Immutable, NonNullable;
            CALENDAR: GcRef<StringValue>, Immutable, NonNullable;
        }
        array ImmutableByteArray => ImmutableByteArraySchema { ELEMENT: I8Value, Mutable, NonNullable; }
        struct RegExpProgram => RegExpProgramSchema {
            ENCODED_BYTES: GcRef<ImmutableByteArray>, Immutable, NonNullable;
        }
        struct RegExpStringIteratorObject: JavaScriptObject => RegExpStringIteratorObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            REGEXP: GcRef<StoredValue>, Immutable, NonNullable;
            INPUT: GcRef<StringValue>, Immutable, NonNullable;
            GLOBAL: bool, Immutable, NonNullable;
            FULL_UNICODE: bool, Immutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
        }
        struct PrivateName => PrivateNameSchema {
            DESCRIPTION: GcRef<StringValue>, Immutable, Nullable;
        }
        // Mutable only so class evaluation can store each fresh name before
        // allocating the next; no published table is written again.
        array PrivateNameTable => PrivateNameTableSchema { ELEMENT: GcRef<PrivateName>, Mutable, NonNullable; }
        // JSON parser/reviver frames and stringify context are compiler-private
        // strong owners. Only RawJsonObject is a JavaScript object.
        struct RawJsonObject: JavaScriptObject => RawJsonObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RAW_JSON: GcRef<StringValue>, Immutable, NonNullable;
        }
        struct JsonParseRecord => JsonParseRecordSchema {
            VALUE: GcRef<StoredValue>, Immutable, NonNullable;
            SOURCE: GcRef<StringValue>, Immutable, Nullable;
            CHILDREN: GcRef<JsonParseChild>, Immutable, Nullable;
        }
        struct JsonParseChild => JsonParseChildSchema {
            KEY: GcRef<StringValue>, Immutable, NonNullable;
            RECORD: GcRef<JsonParseRecord>, Mutable, NonNullable;
            NEXT: GcRef<JsonParseChild>, Immutable, Nullable;
        }
        struct JsonParseFrame => JsonParseFrameSchema {
            CONTAINER: GcRef<StoredValue>, Immutable, NonNullable;
            STATE: crate::builtins::JsonParseFrameState, Mutable, NonNullable;
            KEY: GcRef<StringValue>, Mutable, Nullable;
            INDEX: I64Value, Mutable, NonNullable;
            CHILDREN: GcRef<JsonParseChild>, Mutable, Nullable;
            PARENT: GcRef<JsonParseFrame>, Immutable, Nullable;
        }
        struct JsonReviverFrame => JsonReviverFrameSchema {
            HOLDER: GcRef<StoredValue>, Immutable, NonNullable;
            KEY: GcRef<StringValue>, Immutable, NonNullable;
            RECORD: GcRef<JsonParseRecord>, Mutable, Nullable;
            VALUE: GcRef<StoredValue>, Mutable, NonNullable;
            STATE: crate::builtins::JsonReviverFrameState, Mutable, NonNullable;
            CURSOR: I64Value, Mutable, NonNullable;
            LIMIT: I64Value, Mutable, NonNullable;
            KEYS: GcRef<ValueArray>, Mutable, Nullable;
            PARENT: GcRef<JsonReviverFrame>, Immutable, Nullable;
            ROLE: crate::builtins::JsonReviverPropertyRole, Immutable, NonNullable;
        }
        struct JsonStringifyContext => JsonStringifyContextSchema {
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            REPLACER: GcRef<StoredValue>, Immutable, NonNullable;
            PROPERTY_LIST: GcRef<ValueArray>, Immutable, Nullable;
            GAP: GcRef<StringValue>, Immutable, NonNullable;
        }

        struct NativeErrorObject: JavaScriptObject => NativeErrorObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            STACK: GcRef<StringValue>, Immutable, NonNullable;
        }
        struct MapIteratorObject: JavaScriptObject => MapIteratorObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            MAP: GcRef<MapObject>, Mutable, Nullable;
            NEXT_INDEX: I64Value, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            KIND: MapIterationKind, Immutable, NonNullable;
            NEXT_ORDINAL: I64Value, Mutable, NonNullable;
            VERSION: I64Value, Mutable, NonNullable;
        }
        struct SetIteratorObject: JavaScriptObject => SetIteratorObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            SET: GcRef<SetObject>, Mutable, Nullable;
            NEXT_INDEX: I64Value, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            KIND: SetIterationKind, Immutable, NonNullable;
            NEXT_ORDINAL: I64Value, Mutable, NonNullable;
            VERSION: I64Value, Mutable, NonNullable;
        }
        struct TypedArrayIteratorObject: JavaScriptObject => TypedArrayIteratorObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ARRAY: GcRef<TypedArrayObject>, Mutable, Nullable;
            NEXT_INDEX: I64Value, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            KIND: ArrayIterationKind, Immutable, NonNullable;
        }

        struct BufferOwner => BufferOwnerSchema {
            KIND: BufferOwnerKind, Immutable, NonNullable;
            ARRAY_BUFFER: GcRef<ArrayBuffer>, Immutable, Nullable;
            SHARED_ARRAY_BUFFER: GcRef<SharedArrayBuffer>, Immutable, Nullable;
        }
        struct ArrayIteratorObject: JavaScriptObject => ArrayIteratorObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ARRAY: GcRef<StoredValue>, Mutable, NonNullable;
            NEXT_INDEX: I64Value, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            KIND: ArrayIterationKind, Immutable, NonNullable;
        }
        struct StringIteratorObject: JavaScriptObject => StringIteratorObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            INPUT: GcRef<StringValue>, Immutable, NonNullable;
            NEXT_CODE_UNIT_INDEX: I64Value, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
        }
        struct GeneratorDelegate => GeneratorDelegateSchema {
            ITERATOR: GcRef<IteratorRecord>, Immutable, NonNullable;
            PENDING_KIND: GeneratorDelegatePending, Mutable, NonNullable;
            PENDING_VALUE: GcRef<StoredValue>, Mutable, NonNullable;
            ASYNC_ITERATOR: bool, Immutable, NonNullable;
        }
        struct ModuleGraph => ModuleGraphSchema {
            MODULES: GcRef<ModuleRegistry>, Immutable, NonNullable;
            NEXT_ASYNC_ORDER: I64Value, Mutable, NonNullable;
            PARENT_BUDGET: I64Value, Immutable, NonNullable;
            PARENT_COUNT: I64Value, Mutable, NonNullable;
            PROGRAM: GcRef<ProgramInstance>, Immutable, NonNullable;
            NEXT: GcRef<ModuleGraph>, Immutable, Nullable;
        }
        struct ModuleRequest => ModuleRequestSchema {
            PHASE: crate::heap::ModuleRequestPhase, Immutable, NonNullable;
            TARGET: GcRef<ModuleRecord>, Immutable, NonNullable;
        }
        array ModuleRequestTable => ModuleRequestTableSchema { ELEMENT: GcRef<ModuleRequest>, Immutable, NonNullable; }
        struct ModuleParent => ModuleParentSchema {
            MODULE: GcRef<ModuleRecord>, Immutable, NonNullable;
            NEXT: GcRef<ModuleParent>, Mutable, Nullable;
        }
        struct ModuleJoin => ModuleJoinSchema {
            PROMISE: GcRef<PromiseObject>, Immutable, NonNullable;
            REMAINING: I64Value, Mutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
        }
        struct ModuleDfsFrame => ModuleDfsFrameSchema {
            MODULE: GcRef<ModuleRecord>, Immutable, NonNullable;
            DEPENDENCIES: GcRef<ModuleRegistry>, Mutable, Nullable;
            DEPENDENCY_COUNT: I64Value, Immutable, NonNullable;
            NEXT_DEPENDENCY: I64Value, Mutable, NonNullable;
            RETURNED_CHILD: GcRef<ModuleRecord>, Mutable, Nullable;
        }
        array ModuleDfsStack => ModuleDfsStackSchema { ELEMENT: GcRef<ModuleDfsFrame>, Mutable, Nullable; }
        struct AsyncDisposableStackObject: JavaScriptObject => AsyncDisposableStackObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            STATE: crate::heap::AsyncDisposableStackState, Mutable, NonNullable;
            RESOURCES: GcRef<AsyncDisposableResourceStack>, Mutable, NonNullable;
        }
        // AddDisposableResource captures this List before observable method
        // getters. A reentrant move transfers the same owner, not a snapshot.
        struct AsyncDisposableResourceStack => AsyncDisposableResourceStackSchema {
            RESOURCES: GcRef<AsyncDisposableResourceTable>, Mutable, NonNullable;
            ENTRY_COUNT: I64Value, Mutable, NonNullable;
        }
        struct AsyncDisposableStackDisposal => AsyncDisposableStackDisposalSchema {
            PROMISE: GcRef<PromiseObject>, Immutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            RESOURCES: GcRef<AsyncDisposableResourceTable>, Immutable, NonNullable;
            NEXT_INDEX: I64Value, Mutable, NonNullable;
            COMPLETION_KIND: crate::builtins::AsyncDisposableStackDisposeCompletionKind, Mutable, NonNullable;
            ERROR: GcRef<StoredValue>, Mutable, NonNullable;
            NEEDS_AWAIT: bool, Mutable, NonNullable;
            HAS_AWAITED: bool, Mutable, NonNullable;
        }
        struct AsyncDisposableStackSyncDisposeContext => AsyncDisposableStackSyncDisposeContextSchema {
            METHOD: GcRef<StoredValue>, Immutable, NonNullable;
        }
        struct AsyncDisposableResource => AsyncDisposableResourceSchema {
            KIND: crate::heap::AsyncDisposableStackEntryKind, Immutable, NonNullable;
            VALUE: GcRef<StoredValue>, Immutable, NonNullable;
            METHOD: GcRef<StoredValue>, Immutable, NonNullable;
        }
        array AsyncDisposableResourceTable => AsyncDisposableResourceTableSchema { ELEMENT: GcRef<AsyncDisposableResource>, Mutable, Nullable; }

        struct PromiseThenableJob => PromiseThenableJobSchema {
            PROMISE: GcRef<PromiseObject>, Immutable, NonNullable;
            THENABLE: GcRef<StoredValue>, Immutable, NonNullable;
            THEN_METHOD: GcRef<StoredValue>, Immutable, NonNullable;
        }
        // The private brand retains the actual host record and its defining Realm.
        struct ModuleSourceObject: JavaScriptObject => ModuleSourceObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            MODULE: GcRef<ModuleRecord>, Immutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            SOURCE_TAG: GcRef<StringValue>, Immutable, NonNullable;
        }
        struct ModuleNamespaceObject: JavaScriptObject => ModuleNamespaceObjectSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            DEFERRED_EVALUATOR: GcRef<FunctionObject>, Immutable, Nullable;
            EXPORTS: GcRef<ModuleExportTable>, Immutable, NonNullable;
        }
        struct ModuleExport => ModuleExportSchema {
            NAME: GcRef<StringValue>, Immutable, NonNullable;
            READER: GcRef<FunctionObject>, Immutable, NonNullable;
        }
        array ModuleExportTable => ModuleExportTableSchema { ELEMENT: GcRef<ModuleExport>, Immutable, NonNullable; }
        struct AtomicsAsyncWaiter => AtomicsAsyncWaiterSchema {
            ACTIVE: bool, Mutable, NonNullable;
            BUFFER: GcRef<SharedArrayBuffer>, Immutable, NonNullable;
            BYTE_OFFSET: I64Value, Immutable, NonNullable;
            PROMISE: GcRef<PromiseObject>, Immutable, NonNullable;
            DEADLINE_NANOS: I64Value, Immutable, NonNullable;
            HOST_WAITER_ID: I64Value, Immutable, NonNullable;
            NEXT: GcRef<AtomicsAsyncWaiter>, Mutable, Nullable;
        }

        struct ActivationSyncDisposeCapability => ActivationSyncDisposeCapabilitySchema {
            STATE: crate::heap::DisposableStackState, Mutable, NonNullable;
            RESOURCES: GcRef<DisposableResourceTable>, Mutable, NonNullable;
            ENTRY_COUNT: I64Value, Mutable, NonNullable;
        }
        struct ActivationAsyncDisposeCapability => ActivationAsyncDisposeCapabilitySchema {
            STATE: crate::heap::ActivationAsyncDisposeCapabilityState, Mutable, NonNullable;
            RESOURCES: GcRef<ActivationAsyncDisposeResourceTable>, Mutable, NonNullable;
            ENTRY_COUNT: I64Value, Mutable, NonNullable;
            // Decremented before each callback/Await; resumes never repeat it.
            NEXT_RESOURCE_INDEX: I64Value, Mutable, NonNullable;
            // Empty async resources require one final Await only if no disposer
            // has already supplied an Await during this disposal pass.
            NEEDS_AWAIT: bool, Mutable, NonNullable;
            HAS_AWAITED: bool, Mutable, NonNullable;
        }
        struct ActivationAsyncDisposeResource => ActivationAsyncDisposeResourceSchema {
            KIND: crate::heap::ActivationAsyncDisposeEntryKind, Immutable, NonNullable;
            VALUE: GcRef<StoredValue>, Immutable, NonNullable;
            METHOD: GcRef<StoredValue>, Immutable, NonNullable;
        }
        array ActivationAsyncDisposeResourceTable => ActivationAsyncDisposeResourceTableSchema { ELEMENT: GcRef<ActivationAsyncDisposeResource>, Mutable, Nullable; }


        // Cached methods are StoredValues: GetIteratorDirect does not perform
        // the later IsCallable check. A helper's done/executing flags are
        // separate from the underlying Iterator Record's Done state.
        struct IteratorFromWrapper: JavaScriptObject => IteratorFromWrapperSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RECORD: GcRef<IteratorRecord>, Immutable, NonNullable;
        }
        struct IteratorBuffer => IteratorBufferSchema {
            HEAD: GcRef<ArgumentListNode>, Mutable, Nullable;
            TAIL: GcRef<ArgumentListNode>, Mutable, Nullable;
            COUNT: I64Value, Mutable, NonNullable;
        }
        struct IteratorBufferedHelper: JavaScriptObject => IteratorBufferedHelperSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RECORD: GcRef<IteratorRecord>, Immutable, NonNullable;
            BUFFER: GcRef<IteratorBuffer>, Immutable, NonNullable;
            MODE: IteratorBufferMode, Immutable, NonNullable;
            SIZE: I64Value, Immutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            EXECUTING: bool, Mutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            STARTED: bool, Mutable, NonNullable;
        }
        struct IteratorMapHelper: JavaScriptObject => IteratorMapHelperSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RECORD: GcRef<IteratorRecord>, Immutable, NonNullable;
            MAPPER: GcRef<StoredValue>, Immutable, NonNullable;
            INDEX: F64Value, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            EXECUTING: bool, Mutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            STARTED: bool, Mutable, NonNullable;
        }
        struct IteratorFilterHelper: JavaScriptObject => IteratorFilterHelperSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RECORD: GcRef<IteratorRecord>, Immutable, NonNullable;
            PREDICATE: GcRef<StoredValue>, Immutable, NonNullable;
            INDEX: F64Value, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            EXECUTING: bool, Mutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            STARTED: bool, Mutable, NonNullable;
        }
        struct IteratorFlatMapHelper: JavaScriptObject => IteratorFlatMapHelperSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            OUTER: GcRef<IteratorRecord>, Immutable, NonNullable;
            MAPPER: GcRef<StoredValue>, Immutable, NonNullable;
            INNER: GcRef<IteratorRecord>, Mutable, Nullable;
            INNER_ACTIVE: bool, Mutable, NonNullable;
            INDEX: F64Value, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            EXECUTING: bool, Mutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            STARTED: bool, Mutable, NonNullable;
        }
        struct IteratorTakeHelper: JavaScriptObject => IteratorTakeHelperSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RECORD: GcRef<IteratorRecord>, Immutable, NonNullable;
            // Number arithmetic retains positive infinity.
            REMAINING: F64Value, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            EXECUTING: bool, Mutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            STARTED: bool, Mutable, NonNullable;
        }
        struct IteratorDropHelper: JavaScriptObject => IteratorDropHelperSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            RECORD: GcRef<IteratorRecord>, Immutable, NonNullable;
            REMAINING: F64Value, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            EXECUTING: bool, Mutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            STARTED: bool, Mutable, NonNullable;
        }
        struct IteratorConcatEntry => IteratorConcatEntrySchema {
            ITERABLE: GcRef<StoredValue>, Immutable, NonNullable;
            METHOD: GcRef<StoredValue>, Immutable, NonNullable;
        }
        array IteratorConcatEntries => IteratorConcatEntriesSchema { ELEMENT: GcRef<IteratorConcatEntry>, Mutable, NonNullable; }
        struct IteratorConcatHelper: JavaScriptObject => IteratorConcatHelperSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ENTRIES: GcRef<IteratorConcatEntries>, Immutable, NonNullable;
            CURRENT: GcRef<IteratorRecord>, Mutable, Nullable;
            NEXT_ENTRY: I64Value, Mutable, NonNullable;
            ACTIVE: bool, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            EXECUTING: bool, Mutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
            STARTED: bool, Mutable, NonNullable;
        }
        struct IteratorZipEntry => IteratorZipEntrySchema {
            RECORD: GcRef<IteratorRecord>, Immutable, NonNullable;
            OPEN: bool, Mutable, NonNullable;
            PADDING: GcRef<StoredValue>, Immutable, NonNullable;
        }
        array IteratorZipEntries => IteratorZipEntriesSchema { ELEMENT: GcRef<IteratorZipEntry>, Mutable, NonNullable; }
        struct IteratorZipAcquisition => IteratorZipAcquisitionSchema {
            RECORD: GcRef<IteratorRecord>, Immutable, NonNullable;
            KEY: GcRef<StoredValue>, Immutable, Nullable;
            PREVIOUS: GcRef<IteratorZipAcquisition>, Immutable, Nullable;
        }
        array PropertyKeyTable => PropertyKeyTableSchema { ELEMENT: GcRef<StoredValue>, Mutable, NonNullable; }
        struct IteratorZipHelper: JavaScriptObject => IteratorZipHelperSchema {
            OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;
            ENTRIES: GcRef<IteratorZipEntries>, Immutable, NonNullable;
            // Null selects array rows; a complete property-key list selects
            // keyed rows. Entries/keys have equal length when keys are present.
            KEYS: GcRef<PropertyKeyTable>, Immutable, Nullable;
            MODE: IteratorZipMode, Immutable, NonNullable;
            STARTED: bool, Mutable, NonNullable;
            DONE: bool, Mutable, NonNullable;
            EXECUTING: bool, Mutable, NonNullable;
            REALM: GcRef<RealmRecord>, Immutable, NonNullable;
        }
        // Current async-from-sync drivers capture this state in suspension
        // bindings/delegates, rather than allocating a public wrapper object.
        struct ForAwaitIteratorState => ForAwaitIteratorStateSchema {
            RECORD: GcRef<IteratorRecord>, Immutable, NonNullable;
            ASYNC_ITERATOR: bool, Immutable, NonNullable;
        }
        array ForAwaitIteratorTable => ForAwaitIteratorTableSchema { ELEMENT: GcRef<ForAwaitIteratorState>, Mutable, Nullable; }

        // Each native closure capture has a concrete declared owner. The
        // atomic constructor must select exactly the payload belonging to KIND.
        struct BuiltinClosureCapture => BuiltinClosureCaptureSchema {
            KIND: BuiltinClosureCaptureKind, Immutable, NonNullable;
            COLLATOR: GcRef<IntlCollatorObject>, Immutable, Nullable;
            NUMBER_FORMAT: GcRef<IntlNumberFormatObject>, Immutable, Nullable;
            DATE_TIME_FORMAT: GcRef<IntlDateTimeFormatObject>, Immutable, Nullable;
            RESOLVING: GcRef<PromiseResolvingContext>, Immutable, Nullable;
            CAPABILITY_EXECUTOR: GcRef<PromiseCapabilityExecutorContext>, Immutable, Nullable;
            ELEMENT: GcRef<PromiseElementContext>, Immutable, Nullable;
            KEYED_ELEMENT: GcRef<PromiseKeyedElementContext>, Immutable, Nullable;
            FINALLY: GcRef<PromiseFinallyContext>, Immutable, Nullable;
            FINALLY_VALUE: GcRef<PromiseFinallyValueContext>, Immutable, Nullable;
            ARRAY_FROM_ASYNC: GcRef<ArrayFromAsyncState>, Immutable, Nullable;
            PROXY_REVOCATION: GcRef<ProxyRevocationContext>, Immutable, Nullable;
            ASYNC_DISPOSABLE_STACK_DISPOSAL: GcRef<AsyncDisposableStackDisposal>, Immutable, Nullable;
            ASYNC_DISPOSABLE_STACK_SYNC_DISPOSE: GcRef<AsyncDisposableStackSyncDisposeContext>, Immutable, Nullable;
            REGEXP_LEGACY_ACCESSOR: GcRef<RegExpLegacyAccessorContext>, Immutable, Nullable;
            SHADOW_REALM_WRAPPED_FUNCTION: GcRef<ShadowRealmWrappedFunctionContext>, Immutable, Nullable;
            SHADOW_REALM_IMPORT: GcRef<ShadowRealmImportContext>, Immutable, Nullable;
        }
        struct ShadowRealmWrappedFunctionContext => ShadowRealmWrappedFunctionContextSchema {
            TARGET: GcRef<StoredValue>, Immutable, NonNullable;
        }
        struct ShadowRealmImportContext => ShadowRealmImportContextSchema {
            NAMESPACE: GcRef<StoredValue>, Immutable, NonNullable;
            EXPORT_NAME: GcRef<StringValue>, Immutable, NonNullable;
        }
        struct ProxyRevocationContext => ProxyRevocationContextSchema {
            PROXY: GcRef<ProxyObject>, Mutable, Nullable;
        }
        struct PromiseResolvingContext => PromiseResolvingContextSchema {
            PROMISE: GcRef<PromiseObject>, Immutable, NonNullable;
            ALREADY_RESOLVED: bool, Mutable, NonNullable;
        }
        // NewPromiseCapability exposes the executor before user Construct
        // returns. It owns mutable, initially Undefined values, including if
        // user code retains the executor. After Construct and callable checks,
        // the future constructor publishes a separate immutable capability.
        // The repeated-executor guard must continue to inspect these values.
        struct PromiseCapabilityExecutorContext => PromiseCapabilityExecutorContextSchema {
            RESOLVE: GcRef<StoredValue>, Mutable, NonNullable;
            REJECT: GcRef<StoredValue>, Mutable, NonNullable;
        }
        struct PromiseCombinatorShared => PromiseCombinatorSharedSchema {
            REMAINING: I64Value, Mutable, NonNullable;
            VALUES: GcRef<ValueArray>, Mutable, NonNullable;
            SETTLE: GcRef<StoredValue>, Immutable, NonNullable;
        }
        struct PromiseKeyedCombinatorShared => PromiseKeyedCombinatorSharedSchema {
            REMAINING: I64Value, Mutable, NonNullable;
            VALUES: GcRef<StoredValue>, Immutable, NonNullable;
            SETTLE: GcRef<StoredValue>, Immutable, NonNullable;
        }
        struct PromiseElementContext => PromiseElementContextSchema {
            INDEX: I64Value, Immutable, NonNullable;
            SHARED: GcRef<PromiseCombinatorShared>, Immutable, NonNullable;
            ALREADY_CALLED: bool, Mutable, NonNullable;
        }
        struct PromiseKeyedElementContext => PromiseKeyedElementContextSchema {
            KEY: GcRef<StoredValue>, Immutable, NonNullable;
            SHARED: GcRef<PromiseKeyedCombinatorShared>, Immutable, NonNullable;
            ALREADY_CALLED: bool, Mutable, NonNullable;
        }
        struct PromiseFinallyContext => PromiseFinallyContextSchema {
            ON_FINALLY: GcRef<StoredValue>, Immutable, NonNullable;
            CONSTRUCTOR: GcRef<StoredValue>, Immutable, NonNullable;
        }
        struct PromiseFinallyValueContext => PromiseFinallyValueContextSchema {
            VALUE: GcRef<StoredValue>, Immutable, NonNullable;
        }
        struct ArrayFromAsyncState => ArrayFromAsyncStateSchema {
            CAPABILITY: GcRef<PromiseCapability>, Immutable, NonNullable;
            THROWAWAY_CAPABILITY: GcRef<PromiseCapability>, Mutable, Nullable;
            SOURCE: GcRef<StoredValue>, Immutable, NonNullable;
            TARGET: GcRef<StoredValue>, Immutable, NonNullable;
            MAPPER: GcRef<StoredValue>, Immutable, NonNullable;
            THIS_ARGUMENT: GcRef<StoredValue>, Immutable, NonNullable;
            INDEX: I64Value, Mutable, NonNullable;
            LENGTH: I64Value, Immutable, NonNullable;
            FULFILLED_CALLBACK: GcRef<FunctionObject>, Mutable, Nullable;
            REJECTED_CALLBACK: GcRef<FunctionObject>, Mutable, Nullable;
            STAGE: ArrayFromAsyncStage, Mutable, NonNullable;
            ITERATOR: GcRef<IteratorRecord>, Immutable, Nullable;
            MODE: ArrayFromAsyncSourceMode, Immutable, NonNullable;
            SAVED_ERROR: GcRef<StoredValue>, Mutable, NonNullable;
        }
        // Caller links are completed before publication. Nested direct eval
        // and escaping arrows reuse this context; each invocation refreshes
        // the snapshots while derived callers keep their original shared cells.
        struct DirectEvalExecutionContext => DirectEvalExecutionContextSchema {
            CLASS_CONTEXT: GcRef<FunctionContext>, Immutable, Nullable;
            HOME_OBJECT: GcRef<StoredValue>, Immutable, NonNullable;
            DERIVED_BINDINGS: GcRef<DirectEvalDerivedBindings>, Immutable, Nullable;
            THIS_SNAPSHOT: GcRef<StoredValue>, Mutable, NonNullable;
            NEW_TARGET_SNAPSHOT: GcRef<StoredValue>, Mutable, NonNullable;
        }
        // The derived activation supplies all four existing cells together.
        // Their identities are fixed; super() updates the shared this/status
        // cell values, which retained direct-eval arrows must observe.
        struct DirectEvalDerivedBindings => DirectEvalDerivedBindingsSchema {
            THIS_CELL: GcRef<BindingCell>, Immutable, NonNullable;
            THIS_STATUS_CELL: GcRef<BindingCell>, Immutable, NonNullable;
            NEW_TARGET_CELL: GcRef<BindingCell>, Immutable, NonNullable;
            ACTIVE_FUNCTION_CELL: GcRef<BindingCell>, Immutable, NonNullable;
        }
        // Unlike construction-only mutable metadata bytes, this data crosses
        // the typed host boundary and is physically immutable in Wasm.
        array NativeSourcePayloadArray => NativeSourcePayloadArraySchema { ELEMENT: I8Value, Immutable, NonNullable; }
}

/// Indices are assigned before encoding the one mutually recursive graph.
/// The input section contains only singleton function types; registration
/// consumes it and never permits using its rec-entry count as a type index.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GcLayoutRegistry {
    first_type_index: u32,
}

impl GcLayoutRegistry {
    pub(super) fn layout_index(&self, layout: GcLayout) -> u32 {
        self.first_type_index
            .checked_add(layout.ordinal())
            .expect("GC schema type-index overflow")
    }
    pub(crate) fn reference(&self, layout: GcLayout, nullable: GcNullability) -> RefType {
        RefType {
            nullable: nullable.is_nullable(),
            heap_type: HeapType::Concrete(self.layout_index(layout)),
        }
    }
    pub(super) fn assigned(first_type_index: u32) -> Self {
        Self { first_type_index }
    }

    fn index<T: GcHeapType>(&self) -> GcTypeIndex<T> {
        GcTypeIndex::new(
            self.first_type_index
                .checked_add(T::LAYOUT.ordinal())
                .expect("GC schema type-index overflow"),
        )
    }

    pub(super) fn struct_type<T: GcStructHeapType>(&self) -> GcStructType<T> {
        GcStructType::new(self.index())
    }

    pub(super) fn array_type<T: GcArrayHeapType>(
        &self,
    ) -> GcArrayType<T, T::Element, T::Mutability, T::Nullability> {
        GcArrayType::new(self.index())
    }
}
