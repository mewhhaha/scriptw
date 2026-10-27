//! Typed local operands, struct construction and the live scalar/reference ABI.

use super::*;
use crate::WasmRuntimeValueTag;

mod compact_array_recipes;
mod instance_elements;
mod native_direct_eval_compiler;
mod native_function_compiler;
mod native_global_script_compiler;
mod object_header_projection;
mod program_initialize;
mod program_instance;
mod prototype_reference;
pub(crate) use program_instance::{
    FunctionProgramOwner, ProgramInstanceHooks, ProgramInstanceImage, ProgramInstanceLocals,
    ProgramLiteralCount, ProgramLiteralIndex, ProgramLiteralRoots, ProgramMetadataBytes,
    ProgramMetadataView,
};
mod program_string_descriptors;
mod runtime_string_descriptors;
pub(crate) use program_string_descriptors::ProgramStringDescriptorTable;
pub(crate) use runtime_string_descriptors::RuntimeStringDescriptorRowLocals;
mod weak_host;

macro_rules! scalar_local {
    ($name:ident, $ty:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) struct $name {
            index: u32,
        }
        impl $name {
            pub(crate) fn load(self, function: &mut Function) {
                function.instruction(&Instruction::LocalGet(self.index));
            }
            pub(crate) fn store(self, function: &mut Function) {
                function.instruction(&Instruction::LocalSet(self.index));
            }
        }
    };
}
scalar_local!(I32Local, I32);
scalar_local!(I64Local, I64);
scalar_local!(F64Local, F64);

impl I32Local {
    pub(crate) fn set_constant(self, value: i32, function: &mut Function) {
        function.instruction(&Instruction::I32Const(value));
        self.store(function);
    }
}

impl I64Local {
    pub(crate) fn set_constant(self, value: i64, function: &mut Function) {
        function.instruction(&Instruction::I64Const(value));
        self.store(function);
    }
}

/// A nullable native shared backing result rooted in its exact externref local.
/// Allocation and receive are its only producers; publication fixes the closed
/// SharedBuffer role and traps on null only after the native caller's JS check.
#[must_use = "publish or clear the native backing result"]
pub(crate) struct NativeSharedBufferResult {
    index: u32,
}
impl NativeSharedBufferResult {
    fn bind(function: &mut Function) -> Self {
        let index = function.reserve_typed_local(ValType::Ref(wasm_encoder::RefType::EXTERNREF));
        function.instruction(&Instruction::LocalSet(index));
        Self { index }
    }
    pub(crate) fn is_null(&self, function: &mut Function) {
        function.instruction(&Instruction::LocalGet(self.index));
        function.instruction(&Instruction::RefIsNull);
    }
    pub(crate) fn into_resource(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<HostResource> {
        let resource = schema.struct_type::<HostResource>().construct(
            (
                GcOperand::constant(HostResourceKind::SharedBuffer),
                GcOperand {
                    source: OperandSource::NonNullLocal(self.index),
                    shape: PhantomData,
                },
            ),
            function,
        );
        self.clear(function);
        resource
    }
    pub(crate) fn clear(self, function: &mut Function) {
        function.instruction(&Instruction::RefNull(
            wasm_encoder::RefType::EXTERNREF.heap_type,
        ));
        function.instruction(&Instruction::LocalSet(self.index));
        function.release_typed_local(self.index);
    }
}

impl DeclaredGcHostImport {
    pub(crate) fn allocate_byte_array(
        self,
        length: I32Local,
        function: &mut Function,
    ) -> Result<GcStackReference<ByteArray, Nullable>, crate::EmitError> {
        if self.import() != GcHostImport::ByteArrayAllocate {
            return Err(crate::EmitError::unsupported(
                "byte backing allocation requires its declared host import",
            ));
        }
        length.load(function);
        self.emit_call_instruction(function);
        Ok(GcStackReference::new())
    }

    pub(crate) fn allocate_shared_buffer(
        self,
        initial: I64Local,
        maximum: I64Local,
        growable: I32Local,
        function: &mut Function,
    ) -> Result<NativeSharedBufferResult, crate::EmitError> {
        if self.import() != GcHostImport::SharedBufferAllocate {
            return Err(crate::EmitError::unsupported(
                "shared backing allocation requires its declared host import",
            ));
        }
        initial.load(function);
        maximum.load(function);
        growable.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.emit_call_instruction(function);
        Ok(NativeSharedBufferResult::bind(function))
    }
    pub(crate) fn receive_shared_buffer(
        self,
        id: I64Local,
        bigint: &GcLocal<ByteArray, Nullable>,
        function: &mut Function,
    ) -> Result<NativeSharedBufferResult, crate::EmitError> {
        if self.import() != GcHostImport::AgentReceiveResource {
            return Err(crate::EmitError::unsupported(
                "shared backing receive requires its declared host import",
            ));
        }
        self.emit_call_instruction(function);
        bigint.replace(GcStackReference::new(), function);
        id.store(function);
        Ok(NativeSharedBufferResult::bind(function))
    }
    pub(crate) fn call_intl_provider(
        self,
        request: &GcLocal<ByteArray>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> Result<GcStackReference<ByteArray, Nullable>, crate::EmitError> {
        if self.import() != GcHostImport::IntlProviderCall {
            return Err(crate::EmitError::unsupported(
                "Intl byte request requires its declared host import",
            ));
        }
        request.load(schema, function);
        self.emit_call_instruction(function);
        Ok(GcStackReference::new())
    }

    pub(crate) fn call_system_time_zone_snapshot(
        self,
        function: &mut Function,
    ) -> Result<GcStackReference<ByteArray>, crate::EmitError> {
        if self.import() != GcHostImport::SystemTimeZoneSnapshot {
            return Err(crate::EmitError::unsupported(
                "time-zone snapshot requires its declared host import",
            ));
        }
        self.emit_call_instruction(function);
        Ok(GcStackReference::new())
    }
}

#[derive(Debug)]
pub(crate) struct EqRefLocal {
    index: u32,
}
impl EqRefLocal {
    pub(crate) fn load(&self, function: &mut Function) {
        function.instruction(&Instruction::LocalGet(self.index));
    }
    pub(crate) fn clear(&self, function: &mut Function) {
        function.instruction(&Instruction::RefNull(HeapType::Abstract {
            shared: false,
            ty: wasm_encoder::AbstractHeapType::Eq,
        }));
        function.instruction(&Instruction::LocalSet(self.index));
    }
}

/// An initialized rooted local. Its index cannot be used as a scalar operand.
#[derive(Debug)]
pub(crate) struct GcLocal<T: GcHeapType, N: GcFieldNullability = NonNullable> {
    index: u32,
    heap_type: HeapType,
    shape: PhantomData<fn() -> (T, N)>,
}
impl<T: GcHeapType, N: GcFieldNullability> GcLocal<T, N> {
    pub(crate) fn load(
        &self,
        _schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<T, N> {
        function.instruction(&Instruction::LocalGet(self.index));
        // The local declaration already proves its concrete heap type.
        // Logical non-null ownership only needs a null refinement here.
        if !N::NULLABLE {
            function.instruction(&Instruction::RefAsNonNull);
        }
        GcStackReference::new()
    }
    pub(crate) fn replace(&self, value: GcStackReference<T, N>, function: &mut Function) {
        let GcStackReference { .. } = value;
        function.instruction(&Instruction::LocalSet(self.index));
    }
    pub(crate) fn clear(self, function: &mut Function) {
        function.instruction(&Instruction::RefNull(self.heap_type));
        function.instruction(&Instruction::LocalSet(self.index));
        function.release_typed_local(self.index);
    }
}
impl<T: GcHeapType> GcLocal<T, Nullable> {
    pub(crate) fn set_null(&self, schema: &RuntimeSchema, function: &mut Function) {
        function.instruction(&Instruction::RefNull(
            schema
                .reference_type::<T>(GcNullability::Nullable)
                .heap_type,
        ));
        function.instruction(&Instruction::LocalSet(self.index));
    }
}

/// A declaration is consumed when a reference is initialized. Non-null locals
/// cannot be read while their Wasm default value is still null.
#[derive(Debug)]
pub(crate) struct GcLocalSlot<T: GcHeapType, N: GcFieldNullability = NonNullable> {
    index: u32,
    heap_type: HeapType,
    shape: PhantomData<fn() -> (T, N)>,
}
impl<T: GcHeapType, N: GcFieldNullability> GcLocalSlot<T, N> {
    pub(crate) fn initialize(
        self,
        value: GcStackReference<T, N>,
        function: &mut Function,
    ) -> GcLocal<T, N> {
        let GcStackReference { .. } = value;
        function.instruction(&Instruction::LocalSet(self.index));
        GcLocal {
            index: self.index,
            heap_type: self.heap_type,
            shape: PhantomData,
        }
    }
}
impl<T: GcHeapType> GcLocalSlot<T, Nullable> {
    pub(crate) fn initialize_null(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcLocal<T, Nullable> {
        function.instruction(&Instruction::RefNull(
            schema
                .reference_type::<T>(GcNullability::Nullable)
                .heap_type,
        ));
        self.initialize(GcStackReference::new(), function)
    }
}

/// The actual protocol wire domain, including all four execution modes of
/// object/class methods. Allocation and runtime capability checks share it.
macro_rules! function_protocol_codes {
    ($($code:ident => $protocol:expr;)+) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(i32)]
        pub(crate) enum FunctionProtocolCode { $($code,)+ }
        impl FunctionProtocolCode {
            pub(crate) const ALL: &'static [Self] = &[$(Self::$code),+];
            pub(crate) const fn encoding(self) -> i32 { self as i32 }
            pub(crate) const fn protocol(self) -> scriptw_ir::FunctionProtocolIr {
                match self { $(Self::$code => $protocol),+ }
            }
            pub(crate) fn emit_is_constructable(input: I32Local, function: &mut Function) {
                function.instruction(&Instruction::I32Const(0));
                for code in Self::ALL {
                    if code.protocol().is_constructable() {
                        input.load(function);
                        function.instruction(&Instruction::I32Const(code.encoding()));
                        function.instruction(&Instruction::I32Eq);
                        function.instruction(&Instruction::I32Or);
                    }
                }
            }
        }
    };
}
function_protocol_codes! {
    OrdinaryCallOnly => scriptw_ir::FunctionProtocolIr::OrdinaryCallOnly;
    OrdinaryCallAndConstruct => scriptw_ir::FunctionProtocolIr::OrdinaryCallAndConstruct;
    Arrow => scriptw_ir::FunctionProtocolIr::Arrow;
    Generator => scriptw_ir::FunctionProtocolIr::Generator;
    Async => scriptw_ir::FunctionProtocolIr::Async;
    AsyncArrow => scriptw_ir::FunctionProtocolIr::AsyncArrow;
    AsyncGenerator => scriptw_ir::FunctionProtocolIr::AsyncGenerator;
    ModuleActivation => scriptw_ir::FunctionProtocolIr::ModuleActivation;
    AsyncModuleActivation => scriptw_ir::FunctionProtocolIr::AsyncModuleActivation;
    ObjectOrdinaryMethod => scriptw_ir::FunctionProtocolIr::ObjectMethod(scriptw_ir::FunctionExecutionKind::Ordinary);
    ObjectGeneratorMethod => scriptw_ir::FunctionProtocolIr::ObjectMethod(scriptw_ir::FunctionExecutionKind::Generator);
    ObjectAsyncMethod => scriptw_ir::FunctionProtocolIr::ObjectMethod(scriptw_ir::FunctionExecutionKind::Async);
    ObjectAsyncGeneratorMethod => scriptw_ir::FunctionProtocolIr::ObjectMethod(scriptw_ir::FunctionExecutionKind::AsyncGenerator);
    ObjectGetter => scriptw_ir::FunctionProtocolIr::ObjectGetter;
    ObjectSetter => scriptw_ir::FunctionProtocolIr::ObjectSetter;
    ClassConstructor => scriptw_ir::FunctionProtocolIr::ClassConstructor;
    ClassOrdinaryMethod => scriptw_ir::FunctionProtocolIr::ClassMethod(scriptw_ir::FunctionExecutionKind::Ordinary);
    ClassGeneratorMethod => scriptw_ir::FunctionProtocolIr::ClassMethod(scriptw_ir::FunctionExecutionKind::Generator);
    ClassAsyncMethod => scriptw_ir::FunctionProtocolIr::ClassMethod(scriptw_ir::FunctionExecutionKind::Async);
    ClassAsyncGeneratorMethod => scriptw_ir::FunctionProtocolIr::ClassMethod(scriptw_ir::FunctionExecutionKind::AsyncGenerator);
    ClassGetter => scriptw_ir::FunctionProtocolIr::ClassGetter;
    ClassSetter => scriptw_ir::FunctionProtocolIr::ClassSetter;
}
impl FunctionProtocolCode {
    pub(crate) const fn from_ir(protocol: scriptw_ir::FunctionProtocolIr) -> Self {
        use scriptw_ir::{FunctionExecutionKind as E, FunctionProtocolIr as P};
        match protocol {
            P::OrdinaryCallOnly => Self::OrdinaryCallOnly,
            P::OrdinaryCallAndConstruct => Self::OrdinaryCallAndConstruct,
            P::Arrow => Self::Arrow,
            P::Generator => Self::Generator,
            P::Async => Self::Async,
            P::AsyncArrow => Self::AsyncArrow,
            P::AsyncGenerator => Self::AsyncGenerator,
            P::ModuleActivation => Self::ModuleActivation,
            P::AsyncModuleActivation => Self::AsyncModuleActivation,
            P::ObjectGetter => Self::ObjectGetter,
            P::ObjectSetter => Self::ObjectSetter,
            P::ClassConstructor => Self::ClassConstructor,
            P::ClassGetter => Self::ClassGetter,
            P::ClassSetter => Self::ClassSetter,
            P::ObjectMethod(execution) => match execution {
                E::Ordinary => Self::ObjectOrdinaryMethod,
                E::Generator => Self::ObjectGeneratorMethod,
                E::Async => Self::ObjectAsyncMethod,
                E::AsyncGenerator => Self::ObjectAsyncGeneratorMethod,
            },
            P::ClassMethod(execution) => match execution {
                E::Ordinary => Self::ClassOrdinaryMethod,
                E::Generator => Self::ClassGeneratorMethod,
                E::Async => Self::ClassAsyncMethod,
                E::AsyncGenerator => Self::ClassAsyncGeneratorMethod,
            },
        }
    }
}
impl GcI32Constant for scriptw_ir::LanguageFeaturePolicy {
    fn encode(self) -> i32 {
        self.wire_code()
    }
}
impl GcI32Constant for scriptw_ir::FunctionProtocolIr {
    fn encode(self) -> i32 {
        FunctionProtocolCode::from_ir(self).encoding()
    }
}
impl GcI32Constant for ExecutableCodeKind {
    fn encode(self) -> i32 {
        match self {
            Self::JavaScript => 0,
            Self::StandardBuiltin => 1,
            Self::HostBuiltin => 2,
        }
    }
}
impl<T: GcI32Constant> GcI32Constant for Option<T>
where
    Option<T>: GcI32Field,
{
    fn encode(self) -> i32 {
        match self {
            None => -1,
            Some(value) => value.encode(),
        }
    }
}
impl GcI32Constant for scriptw_ir::StandardBuiltinId {
    fn encode(self) -> i32 {
        self as i32
    }
}
impl GcI32Constant for scriptw_ir::HostBuiltinId {
    fn encode(self) -> i32 {
        self as i32
    }
}
macro_rules! enum_i32_codec {
    ($domain:ty; $($variant:ident => $code:expr),+ $(,)?) => {
        impl GcI32Constant for $domain {
            fn encode(self) -> i32 { match self { $(Self::$variant => $code),+ } }
        }
    };
}
enum_i32_codec!(scriptw_ir::ClassHeritageKind; None => 0, Constructable => 1, Null => 2);
impl GcI32Constant for crate::module::TypedArrayElementKind {
    fn encode(self) -> i32 {
        self.abi_word() as i32
    }
}
impl GcI32Constant for crate::emit::CompletionKind {
    fn encode(self) -> i32 {
        self.code() as i32
    }
}
enum_i32_codec!(crate::heap::GeneratorState; SuspendedStart => 0, Executing => 1,
    Completed => 2, SuspendedYield => 3);
enum_i32_codec!(crate::heap::GeneratorResumeKind; Normal => 0, Return => 1, Throw => 2);
enum_i32_codec!(crate::heap::AsyncGeneratorExecutionState; SuspendedStart => 0,
    SuspendedYield => 1, Executing => 2, DrainingQueue => 3, Completed => 4);
enum_i32_codec!(crate::heap::AsyncGeneratorBodyStatus; Idle => 0, Running => 1,
    Await => 2, Yield => 3, Complete => 4, Throw => 5);
enum_i32_codec!(crate::heap::AsyncGeneratorResumeKind; Normal => 0, Return => 1,
    Throw => 2, Fulfill => 3, Reject => 4);
impl GcI32Constant for crate::heap::AsyncGeneratorRequestCompletionKind {
    fn encode(self) -> i32 {
        match self {
            Self::Normal => crate::emit::CompletionKind::Normal,
            Self::Return => crate::emit::CompletionKind::Return,
            Self::Throw => crate::emit::CompletionKind::Throw,
        }
        .code() as i32
    }
}
enum_i32_codec!(crate::heap::PromiseState; Pending => 0, Fulfilled => 1, Rejected => 2);
enum_i32_codec!(crate::heap::PromiseReactionType; Fulfill => 1, Reject => 2);
enum_i32_codec!(crate::heap::PromiseReactionCallbackKind; Default => 0, AsyncFunction => 1,
    AsyncGeneratorAwaitReturn => 2, AsyncGeneratorAwait => 3, AsyncGeneratorYield => 4,
    AsyncGeneratorYieldReturn => 5, ModuleBody => 6, ModuleJoin => 7, AsyncFromSyncIterator => 8);
enum_i32_codec!(crate::heap::PromiseJobKind; Reaction => 1, ResolveThenable => 2, FinalizationCleanup => 3);
enum_i32_codec!(crate::heap::AsyncModuleEntryMode; Allocate => 0, Instantiate => 1, Execute => 2);
enum_i32_codec!(crate::heap::ModuleEvaluationState; Linked => 0, Evaluating => 1,
    EvaluatingAsync => 2, Evaluated => 3);
enum_i32_codec!(crate::heap::ModuleEvaluationCompletion; Empty => 0, Normal => 1, Throw => 2);
enum_i32_codec!(crate::heap::ModuleBodyState; NotStarted => 0, Executing => 1, Completed => 2);
enum_i32_codec!(crate::heap::ModuleActivationKind; Synchronous => 0, Async => 1);
enum_i32_codec!(crate::heap::ModuleRequestPhase; Evaluation => 0, Defer => 1);
// JSON's actual iterative owner defines the closed traversal states once.
impl GcI32Constant for crate::builtins::JsonParseFrameState {
    fn encode(self) -> i32 {
        self.wire_code()
    }
}
impl GcI32Constant for crate::builtins::JsonReviverFrameState {
    fn encode(self) -> i32 {
        self.wire_code()
    }
}
impl GcI32Constant for crate::builtins::JsonReviverPropertyRole {
    fn encode(self) -> i32 {
        self.wire_code()
    }
}
macro_rules! word_i32_codec {
    ($($domain:ty),+ $(,)?) => { $(impl GcI32Constant for $domain {
        fn encode(self) -> i32 { self.word() as i32 }
    })+ };
}
word_i32_codec!(
    crate::heap::DisposableStackState,
    crate::heap::AsyncDisposableStackState,
    crate::heap::DisposableStackEntryKind,
    crate::heap::AsyncDisposableStackEntryKind,
    crate::heap::ActivationAsyncDisposeCapabilityState,
    crate::heap::ActivationAsyncDisposeEntryKind
);
enum_i32_codec!(ProxyCallCapability; ObjectOnly => 0, CallOnly => 1, CallAndConstruct => 2);
enum_i32_codec!(MapIterationKind; Key => 0, Value => 1, KeyAndValue => 2);
enum_i32_codec!(ArrayIndexedStorageMode; DenseDescriptors => 0, SparseDescriptors => 1, PackedNumbers => 2, PackedValues => 3);
enum_i32_codec!(SetIterationKind; Value => 0, KeyAndValue => 1);
enum_i32_codec!(ArrayIterationKind; Key => 0, Value => 1, KeyAndValue => 2);
enum_i32_codec!(PrivateElementKind; Brand => 0, Field => 1, Method => 2, Accessor => 3);
enum_i32_codec!(AwaitCompletionKind; Normal => 0, Throw => 1);
enum_i32_codec!(AsyncGeneratorReturnStage; Unawaited => 0, Awaited => 1);
enum_i32_codec!(GeneratorDelegatePending; Normal => 0, Return => 1, Throw => 2, MissingThrowClose => 3, ReturnValue => 4, YieldValue => 5);
enum_i32_codec!(HostResourceKind; SharedBuffer => 0, WeakMap => 1, WeakSet => 2, WeakRef => 3, FinalizationCell => 4);
enum_i32_codec!(BufferOwnerKind; ArrayBuffer => 0, SharedArrayBuffer => 1);
enum_i32_codec!(IteratorBufferMode; Chunks => 0, WindowsOnlyFull => 1, WindowsAllowPartial => 2);
enum_i32_codec!(IteratorZipMode; Shortest => 0, Longest => 1, Strict => 2);
enum_i32_codec!(ArrayFromAsyncStage; InputValue => 0, MappedValue => 1, AsyncIteratorResult => 2,
    AsyncCloseResult => 4);
enum_i32_codec!(ArrayFromAsyncSourceMode; ArrayLike => 0, AsyncIterator => 1, SyncIterator => 2);
enum_i32_codec!(BuiltinClosureCaptureKind; IntlCollator => 0, IntlNumberFormat => 1,
    IntlDateTimeFormat => 2, PromiseResolving => 3, PromiseCapabilityExecutor => 4,
    PromiseElement => 5, PromiseKeyedElement => 6, PromiseFinally => 7,
    PromiseFinallyValue => 8, ArrayFromAsync => 9, ProxyRevocation => 11,
    AsyncDisposableStackDisposal => 12, AsyncDisposableStackSyncDispose => 13, RegExpLegacyAccessor => 14, ShadowRealmWrappedFunction => 15, ShadowRealmImport => 16);
enum_i32_codec!(RegExpLegacySlot; Input => 0, LastMatch => 1, LastParen => 2, LeftContext => 3, RightContext => 4, Paren1 => 5, Paren2 => 6, Paren3 => 7, Paren4 => 8, Paren5 => 9, Paren6 => 10, Paren7 => 11, Paren8 => 12, Paren9 => 13);
impl GcI32Constant for crate::builtins::AsyncDisposableStackDisposeCompletionKind {
    fn encode(self) -> i32 {
        self.wire_code()
    }
}
macro_rules! intl_number_i32_codec {
    ($($domain:ty),+ $(,)?) => { $(impl GcI32Constant for $domain {
        fn encode(self) -> i32 { self.wire_code() as i32 }
    })+ };
}
intl_number_i32_codec!(
    scriptw_intl::DateTimeCalendar,
    scriptw_intl::DateTimeHourCycle,
    scriptw_intl::DateTimeHourCyclePreference,
    scriptw_intl::DateTimeLocaleMatcher,
    scriptw_intl::DateTimeFormatMatcher,
    scriptw_intl::DateTimeNumericWidth,
    scriptw_intl::DateTimeTextWidth,
    scriptw_intl::DateTimeMonthWidth,
    scriptw_intl::DateTimeStyle,
    scriptw_intl::DateTimeRequired,
    scriptw_intl::DateTimeDefaults,
    scriptw_intl::DateTimeValueKind,
    scriptw_intl::DateTimePartKind,
    scriptw_intl::DateTimeRangeSource,
    scriptw_intl::SegmenterGranularity,
    scriptw_intl::DisplayNamesType,
    scriptw_intl::DisplayNamesStyle,
    scriptw_intl::DisplayNamesFallback,
    scriptw_intl::DisplayNamesLanguageDisplay,
    scriptw_intl::RelativeStyle,
    scriptw_intl::RelativeNumeric,
    scriptw_intl::RelativeUnit,
    scriptw_intl::ListType,
    scriptw_intl::ListStyle,
    scriptw_intl::CollatorUsage,
    scriptw_intl::CollatorSensitivity,
    scriptw_intl::CollatorCaseFirst,
    scriptw_intl::CollatorCollationKind,
    scriptw_intl::PluralType,
    scriptw_intl::number_format::options::StyleOption,
    scriptw_intl::number_format::options::LocaleMatcher,
    scriptw_intl::number_format::options::RoundingPriority,
    scriptw_intl::number_format::options::CurrencyDisplay,
    scriptw_intl::number_format::options::CurrencySign,
    scriptw_intl::number_format::options::UnitDisplay,
    scriptw_intl::number_format::options::NotationOption,
    scriptw_intl::number_format::options::CompactDisplay,
    scriptw_intl::number_format::options::Grouping,
    scriptw_intl::number_format::options::SignDisplay,
    scriptw_intl::number_format::options::RoundingMode,
    scriptw_intl::number_format::options::TrailingZeroDisplay,
    scriptw_intl::NumberPrecisionKind,
    scriptw_intl::NumberNumericKind
);
impl GcI32Constant for scriptw_intl::DateTimeFractionalDigits {
    fn encode(self) -> i32 {
        i32::from(self.get())
    }
}
impl GcI32Constant for scriptw_intl::TimeZoneNameStyle {
    fn encode(self) -> i32 {
        self.code() as i32
    }
}
impl GcI32Constant for scriptw_intl::TimeZoneKind {
    fn encode(self) -> i32 {
        self.code() as i32
    }
}
impl GcI32Constant for scriptw_intl::PluralCategorySet {
    fn encode(self) -> i32 {
        self.wire_mask() as i32
    }
}
impl GcI32DomainLocal<scriptw_intl::PluralCategorySet> {
    pub(crate) fn set_checked_mask(&self, value: I64Local, function: &mut Function) {
        let allowed = scriptw_intl::PluralCategory::ALL
            .into_iter()
            .fold(0u64, |mask, category| mask | (1 << category.index()));
        let required = 1u64 << scriptw_intl::PluralCategory::Other.index();
        value.load(function);
        function.instruction(&Instruction::I64Const(!allowed as i64));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        value.load(function);
        function.instruction(&Instruction::I64Const(required as i64));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        value.load(function);
        function.instruction(&Instruction::I32WrapI64);
        self.local.store(function);
    }
}

macro_rules! intl_duration_i32_codec {
    ($($domain:ty),+ $(,)?) => { $(impl GcI32Constant for $domain {
        fn encode(self) -> i32 { i32::try_from(self.index()).expect("Duration domain index fits I32") }
    })+ };
}
intl_duration_i32_codec!(
    scriptw_intl::DurationStyle,
    scriptw_intl::DurationUnitStyle,
    scriptw_intl::DurationDisplay
);
impl GcI32Constant for scriptw_intl::DurationFractionalDigits {
    fn encode(self) -> i32 {
        i32::from(self.value())
    }
}

macro_rules! intl_digit_i32_codec {
    ($($domain:ty),+ $(,)?) => { $(impl GcI32Constant for $domain {
        fn encode(self) -> i32 { i32::from(self.get()) }
    })+ };
}
intl_digit_i32_codec!(
    scriptw_intl::number_format::options::IntegerDigitCount,
    scriptw_intl::number_format::options::FractionDigitCount,
    scriptw_intl::number_format::options::SignificantDigitCount
);
impl GcI32Constant for scriptw_intl::number_format::options::RoundingIncrement {
    fn encode(self) -> i32 {
        match self {
            Self::One => 1,
            Self::Multiple(increment) => i32::from(increment.value()),
        }
    }
}

impl GcI32Constant for scriptw_ir::ClassElementExecutionKind {
    fn encode(self) -> i32 {
        match self {
            Self::None => 0,
            Self::InstanceFieldInitializer => 1,
            Self::StaticFieldInitializer => 2,
            Self::StaticBlock => 3,
        }
    }
}

/// A local holding one registered callable role. Function references remain
/// typed funcrefs; they never pass through the eqref value payload.
pub(crate) struct FunctionLocal<R: GcFunctionRole, N: GcFieldNullability = NonNullable> {
    index: u32,
    shape: PhantomData<fn() -> (R, N)>,
}
pub(crate) struct FunctionLocalSlot<R: GcFunctionRole, N: GcFieldNullability = NonNullable> {
    index: u32,
    shape: PhantomData<fn() -> (R, N)>,
}
pub(crate) struct FunctionStackReference<R: GcFunctionRole, N: GcFieldNullability = NonNullable> {
    shape: PhantomData<fn() -> (R, N)>,
}
impl<R: GcFunctionRole, N: GcFieldNullability> FunctionLocal<R, N> {
    pub(crate) fn load(&self, function: &mut Function) -> FunctionStackReference<R, N> {
        function.instruction(&Instruction::LocalGet(self.index));
        FunctionStackReference { shape: PhantomData }
    }
    pub(crate) fn clear(self, function: &mut Function) {
        function.instruction(&Instruction::RefNull(HeapType::Concrete(
            R::SIGNATURE.type_index(),
        )));
        function.instruction(&Instruction::LocalSet(self.index));
        function.release_typed_local(self.index);
    }
}
impl<R: GcFunctionRole, N: GcFieldNullability> FunctionLocalSlot<R, N> {
    pub(crate) fn initialize(
        self,
        value: FunctionStackReference<R, N>,
        function: &mut Function,
    ) -> FunctionLocal<R, N> {
        let _ = value;
        function.instruction(&Instruction::LocalSet(self.index));
        FunctionLocal {
            index: self.index,
            shape: PhantomData,
        }
    }
}
impl<R: GcFunctionRole> FunctionStackReference<R, Nullable> {
    pub(crate) fn is_null(self, function: &mut Function) {
        function.instruction(&Instruction::RefIsNull);
    }
    pub(crate) fn require_non_null(self, function: &mut Function) -> FunctionStackReference<R> {
        function.instruction(&Instruction::RefAsNonNull);
        FunctionStackReference { shape: PhantomData }
    }
}
impl<R: GcFunctionRole> FunctionStackReference<R> {
    pub(crate) fn nullable(self) -> FunctionStackReference<R, Nullable> {
        FunctionStackReference { shape: PhantomData }
    }
}
impl<'a, R: GcFunctionRole> GcOperand<'a, GcFunctionRef<R>, Nullable> {
    pub(crate) fn function_reference(value: &'a FunctionLocal<R>) -> Self {
        Self {
            source: OperandSource::Local(value.index),
            shape: PhantomData,
        }
    }
    pub(crate) fn null_function() -> Self {
        Self {
            source: OperandSource::Null(HeapType::Concrete(R::SIGNATURE.type_index())),
            shape: PhantomData,
        }
    }
    pub(crate) fn nullable_function_reference(value: &'a FunctionLocal<R, Nullable>) -> Self {
        Self {
            source: OperandSource::Local(value.index),
            shape: PhantomData,
        }
    }
}
impl<R: GcFunctionRole, N: GcFieldNullability> GcFieldRead<GcFunctionRef<R>, N> {
    pub(crate) fn callable(self) -> FunctionStackReference<R, N> {
        FunctionStackReference { shape: PhantomData }
    }
}

pub(crate) enum ExecutableCodeEntry<'a> {
    Ordinary(&'a FunctionLocal<OrdinaryCallable>),
    Generator(&'a FunctionLocal<GeneratorCallable>),
    Async(&'a FunctionLocal<AsyncCallable>),
    AsyncGenerator(&'a FunctionLocal<AsyncGeneratorCallable>),
}
pub(crate) struct ExecutableCodeMetadata {
    pub(crate) source_id: i64,
    pub(crate) kind: ExecutableCodeKind,
    pub(crate) standard_builtin: Option<scriptw_ir::StandardBuiltinId>,
    pub(crate) host_builtin: Option<scriptw_ir::HostBuiltinId>,
    pub(crate) protocol: scriptw_ir::FunctionProtocolIr,
    pub(crate) class_element_execution: scriptw_ir::ClassElementExecutionKind,
}
impl GcStructType<ExecutableCode> {
    pub(crate) fn publish(
        self,
        entry: ExecutableCodeEntry<'_>,
        metadata: ExecutableCodeMetadata,
        function: &mut Function,
    ) -> Result<GcStackReference<ExecutableCode>, crate::EmitError> {
        let entry_execution = match &entry {
            ExecutableCodeEntry::Ordinary(_) => scriptw_ir::FunctionExecutionKind::Ordinary,
            ExecutableCodeEntry::Generator(_) => scriptw_ir::FunctionExecutionKind::Generator,
            ExecutableCodeEntry::Async(_) => scriptw_ir::FunctionExecutionKind::Async,
            ExecutableCodeEntry::AsyncGenerator(_) => {
                scriptw_ir::FunctionExecutionKind::AsyncGenerator
            }
        };
        let origin_valid = match metadata.kind {
            ExecutableCodeKind::JavaScript => {
                metadata.standard_builtin.is_none() && metadata.host_builtin.is_none()
            }
            ExecutableCodeKind::StandardBuiltin => {
                metadata.standard_builtin.is_some() && metadata.host_builtin.is_none()
            }
            ExecutableCodeKind::HostBuiltin => {
                metadata.standard_builtin.is_none() && metadata.host_builtin.is_some()
            }
        };
        if metadata.protocol.execution_kind() != entry_execution || !origin_valid {
            return Err(crate::EmitError::unsupported(
                "executable metadata disagrees with its registered entry",
            ));
        }
        let (ordinary, generator, asynchronous, async_generator) = match entry {
            ExecutableCodeEntry::Ordinary(value) => (
                GcOperand::function_reference(value),
                GcOperand::null_function(),
                GcOperand::null_function(),
                GcOperand::null_function(),
            ),
            ExecutableCodeEntry::Generator(value) => (
                GcOperand::null_function(),
                GcOperand::function_reference(value),
                GcOperand::null_function(),
                GcOperand::null_function(),
            ),
            ExecutableCodeEntry::Async(value) => (
                GcOperand::null_function(),
                GcOperand::null_function(),
                GcOperand::function_reference(value),
                GcOperand::null_function(),
            ),
            ExecutableCodeEntry::AsyncGenerator(value) => (
                GcOperand::null_function(),
                GcOperand::null_function(),
                GcOperand::null_function(),
                GcOperand::function_reference(value),
            ),
        };
        Ok(self.construct(
            (
                GcOperand::i64(metadata.source_id),
                GcOperand::constant(metadata.kind),
                GcOperand::constant(metadata.standard_builtin),
                GcOperand::constant(metadata.host_builtin),
                GcOperand::constant(metadata.protocol),
                GcOperand::constant(metadata.class_element_execution),
                ordinary,
                generator,
                asynchronous,
                async_generator,
            ),
            function,
        ))
    }
}

pub(crate) enum BuiltinClosurePayload<'a> {
    IntlCollator(&'a GcLocal<IntlCollatorObject>),
    IntlNumberFormat(&'a GcLocal<IntlNumberFormatObject>),
    IntlDateTimeFormat(&'a GcLocal<IntlDateTimeFormatObject>),
    PromiseResolving(&'a GcLocal<PromiseResolvingContext>),
    PromiseCapabilityExecutor(&'a GcLocal<PromiseCapabilityExecutorContext>),
    PromiseElement(&'a GcLocal<PromiseElementContext>),
    PromiseKeyedElement(&'a GcLocal<PromiseKeyedElementContext>),
    PromiseFinally(&'a GcLocal<PromiseFinallyContext>),
    PromiseFinallyValue(&'a GcLocal<PromiseFinallyValueContext>),
    ArrayFromAsync(&'a GcLocal<ArrayFromAsyncState>),
    ProxyRevocation(&'a GcLocal<ProxyRevocationContext>),
    AsyncDisposableStackDisposal(&'a GcLocal<AsyncDisposableStackDisposal>),
    AsyncDisposableStackSyncDispose(&'a GcLocal<AsyncDisposableStackSyncDisposeContext>),
    RegExpLegacyAccessor(&'a GcLocal<RegExpLegacyAccessorContext>),
    ShadowRealmWrappedFunction(&'a GcLocal<ShadowRealmWrappedFunctionContext>),
    ShadowRealmImport(&'a GcLocal<ShadowRealmImportContext>),
}
impl GcStructType<BuiltinClosureCapture> {
    pub(crate) fn publish(
        self,
        payload: BuiltinClosurePayload<'_>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<BuiltinClosureCapture> {
        match payload {
            BuiltinClosurePayload::IntlCollator(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::IntlCollator),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::IntlNumberFormat(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::IntlNumberFormat),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::IntlDateTimeFormat(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::IntlDateTimeFormat),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::PromiseResolving(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::PromiseResolving),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::PromiseCapabilityExecutor(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::PromiseCapabilityExecutor),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::PromiseElement(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::PromiseElement),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::PromiseKeyedElement(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::PromiseKeyedElement),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::PromiseFinally(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::PromiseFinally),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::PromiseFinallyValue(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::PromiseFinallyValue),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::ArrayFromAsync(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::ArrayFromAsync),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),

            BuiltinClosurePayload::ProxyRevocation(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::ProxyRevocation),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::AsyncDisposableStackDisposal(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::AsyncDisposableStackDisposal),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::AsyncDisposableStackSyncDispose(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::AsyncDisposableStackSyncDispose),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::RegExpLegacyAccessor(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::RegExpLegacyAccessor),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::ShadowRealmWrappedFunction(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::ShadowRealmWrappedFunction),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            BuiltinClosurePayload::ShadowRealmImport(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::ShadowRealmImport),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                ),
                function,
            ),
        }
    }
}

/// An emitted reference on the operand stack. Only typed allocation/read/cast
/// owners can create this witness; binding consumes it exactly once.
#[derive(Debug)]
pub(crate) struct GcStackReference<T: GcHeapType, N: GcFieldNullability = NonNullable> {
    shape: PhantomData<fn() -> (T, N)>,
}
impl<T: GcHeapType, N: GcFieldNullability> GcStackReference<T, N> {
    fn new() -> Self {
        Self { shape: PhantomData }
    }
}
impl<T: GcHeapType, N: GcFieldNullability> GcStackReference<T, N> {
    pub(crate) fn upcast<Base: GcHeapType>(self) -> GcStackReference<Base, N>
    where
        T: GcSubtype<Base>,
    {
        GcStackReference::new()
    }

    pub(crate) fn cast_subtype<Sub: GcSubtype<T>>(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<Sub> {
        function.instruction(&Instruction::RefCastNonNull(
            schema
                .reference_type::<Sub>(GcNullability::NonNullable)
                .heap_type,
        ));
        GcStackReference::new()
    }
}
impl<T: GcHeapType, N: GcFieldNullability> GcLocal<T, N> {
    pub(crate) fn is_subtype<Sub: GcSubtype<T>>(
        &self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.load(schema, function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<Sub>(GcNullability::NonNullable)
                .heap_type,
        ));
    }
}
impl<T: GcHeapType> GcRootGlobal<T> {
    /// A declared root's actual nullable type is the only authority for this
    /// stack token. The unchecked token constructor remains private here.
    pub(super) fn load_nullable(self, function: &mut Function) -> GcStackReference<T, Nullable> {
        emit_root_get(function, self);
        GcStackReference::new()
    }
}
impl<T: GcArrayHeapType> EmptyArrayGlobal<T> {
    /// The declaration owns a non-null, immutable global containing exactly the
    /// registered zero-length array type. It is a root in this runtime instance.
    pub(super) fn load(self, function: &mut Function) -> GcStackReference<T> {
        function.instruction(&Instruction::GlobalGet(self.index()));
        GcStackReference::new()
    }
}

impl StoredUndefinedGlobal {
    fn load(&self, function: &mut Function) -> GcStackReference<StoredValue> {
        function.instruction(&Instruction::GlobalGet(self.index()));
        GcStackReference::new()
    }
}

impl<T: GcHeapType> GcStackReference<T, NonNullable> {
    pub(crate) fn nullable(self) -> GcStackReference<T, Nullable> {
        GcStackReference::new()
    }
}
impl<T: GcHeapType> GcStackReference<T, Nullable> {
    pub(crate) fn is_null(self, function: &mut Function) {
        function.instruction(&Instruction::RefIsNull);
    }
    pub(crate) fn require_non_null(self, function: &mut Function) -> GcStackReference<T> {
        function.instruction(&Instruction::RefAsNonNull);
        GcStackReference::new()
    }
}

/// The scalar/reference local triple used by actual values and call results.
/// Holding the reference as a separate rooted local prevents integer transport.
pub(crate) enum ScalarValue {
    Undefined,
    Null,
    Boolean(bool),
    NumberBits(i64),
}

#[derive(Debug)]
pub(crate) struct ValueLocals {
    tag: I32Local,
    scalar: I64Local,
    reference: EqRefLocal,
}
impl ValueLocals {
    pub(crate) fn tag(&self) -> I32Local {
        self.tag
    }
    pub(crate) fn scalar(&self) -> I64Local {
        self.scalar
    }
    pub(crate) fn reference(&self) -> &EqRefLocal {
        &self.reference
    }
    pub(crate) fn emit(&self, function: &mut Function) {
        self.tag.load(function);
        self.scalar.load(function);
        self.reference.load(function);
    }
    pub(crate) fn set_undefined(&self, function: &mut Function) {
        self.set_scalar(ScalarValue::Undefined, function);
    }
    pub(crate) fn set_scalar(&self, value: ScalarValue, function: &mut Function) {
        let (tag, scalar) = match value {
            ScalarValue::Undefined => (WasmRuntimeValueTag::Undefined, 0),
            ScalarValue::Null => (WasmRuntimeValueTag::Null, 0),
            ScalarValue::Boolean(value) => (WasmRuntimeValueTag::Boolean, i64::from(value)),
            ScalarValue::NumberBits(bits) => (WasmRuntimeValueTag::Number, bits),
        };
        function.instruction(&Instruction::I32Const(tag as i32));
        self.tag.store(function);
        function.instruction(&Instruction::I64Const(scalar));
        self.scalar.store(function);
        self.reference.clear(function);
    }
    pub(crate) fn copy_from(&self, source: &Self, function: &mut Function) {
        source.tag.load(function);
        self.tag.store(function);
        source.scalar.load(function);
        self.scalar.store(function);
        source.reference.load(function);
        function.instruction(&Instruction::LocalSet(self.reference.index));
    }
    pub(crate) fn set_boolean(&self, source: I32Local, function: &mut Function) {
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Boolean as i32));
        self.tag.store(function);
        source.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I64ExtendI32U);
        self.scalar.store(function);
        self.reference.clear(function);
    }
    pub(crate) fn set_number(&self, bits: I64Local, function: &mut Function) {
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        self.tag.store(function);
        bits.load(function);
        self.scalar.store(function);
        self.reference.clear(function);
    }
    /// A concrete runtime cast validates the reference even when a dynamic
    /// caller has already checked its value tag. No scalar word can become a
    /// semantic reference through this projection.
    pub(crate) fn cast_reference<T: JavaScriptReference>(
        &self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<T> {
        self.reference.load(function);
        function.instruction(&Instruction::RefCastNonNull(
            schema
                .reference_type::<T>(GcNullability::NonNullable)
                .heap_type,
        ));
        GcStackReference::new()
    }
    pub(crate) fn set_reference<T: JavaScriptReference>(
        &self,
        source: &GcLocal<T>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I32Const(T::TAG as i32));
        self.tag.store(function);
        function.instruction(&Instruction::I64Const(0));
        self.scalar.store(function);
        source.load(schema, function);
        function.instruction(&Instruction::LocalSet(self.reference.index));
    }
    /// Pops a value's tag, scalar and reference results (in that stack order).
    pub(crate) fn store_value_result(&self, function: &mut Function) {
        function.instruction(&Instruction::LocalSet(self.reference.index));
        self.scalar.store(function);
        self.tag.store(function);
    }
    pub(crate) fn store_call_result(
        &self,
        kind: I32Local,
        target: I32Local,
        function: &mut Function,
    ) {
        // Results are tag, scalar, reference, kind, target in that exact order.
        target.store(function);
        kind.store(function);
        function.instruction(&Instruction::LocalSet(self.reference.index));
        self.scalar.store(function);
        self.tag.store(function);
    }
    pub(crate) fn emit_completion(
        &self,
        kind: I32Local,
        target: I32Local,
        function: &mut Function,
    ) {
        self.emit(function);
        kind.load(function);
        target.load(function);
    }
    pub(crate) fn clear(self, function: &mut Function) {
        self.reference.clear(function);
        for index in [self.reference.index, self.scalar.index, self.tag.index] {
            function.release_typed_local(index);
        }
    }
}

/// One live completion owns all five ABI parts. A normal conversion result can
/// replace its value only while the completion remains Normal; a nested throw
/// retains the original tag/scalar/reference together.
#[derive(Debug)]
pub(crate) struct CompletionLocals {
    value: ValueLocals,
    kind: I32Local,
    target: I32Local,
}
impl CompletionLocals {
    pub(crate) fn copy_from(&self, source: &Self, function: &mut Function) {
        self.value.copy_from(&source.value, function);
        source.kind.load(function);
        self.kind.store(function);
        source.target.load(function);
        self.target.store(function);
    }
    pub(crate) fn clear(self, function: &mut Function) {
        function.release_typed_local(self.target.index);
        function.release_typed_local(self.kind.index);
        self.value.clear(function);
    }
    pub(crate) fn value(&self) -> &ValueLocals {
        &self.value
    }
    pub(crate) fn kind(&self) -> I32Local {
        self.kind
    }
    pub(crate) fn target(&self) -> I32Local {
        self.target
    }
    pub(crate) fn initialize(&self, function: &mut Function) {
        self.value.set_undefined(function);
        self.set_kind(crate::emit::CompletionKind::Normal, function);
        function.instruction(&Instruction::I32Const(0));
        self.target.store(function);
    }
    pub(crate) fn set_kind(&self, kind: crate::emit::CompletionKind, function: &mut Function) {
        function.instruction(&Instruction::I32Const(kind.code() as i32));
        self.kind.store(function);
    }
    pub(crate) fn set_normal(&self, value: &ValueLocals, function: &mut Function) {
        self.value.copy_from(value, function);
        self.set_kind(crate::emit::CompletionKind::Normal, function);
        function.instruction(&Instruction::I32Const(0));
        self.target.store(function);
    }
    pub(crate) fn set_throw(&self, value: &ValueLocals, function: &mut Function) {
        self.value.copy_from(value, function);
        self.set_kind(crate::emit::CompletionKind::Throw, function);
        function.instruction(&Instruction::I32Const(0));
        self.target.store(function);
    }
    pub(crate) fn commit_if_normal(&self, value: &ValueLocals, function: &mut Function) {
        self.kind.load(function);
        function.instruction(&Instruction::I32Const(crate::COMPLETION_KIND_NORMAL as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.value.copy_from(value, function);
        function.instruction(&Instruction::End);
    }
    pub(crate) fn emit(&self, function: &mut Function) {
        self.value.emit_completion(self.kind, self.target, function);
    }
    pub(crate) fn store_call(&self, result: GcCallResult, function: &mut Function) {
        let GcCallResult { .. } = result;
        self.value
            .store_call_result(self.kind, self.target, function);
    }
}

/// Created only by a call whose registered result is the complete JS ABI.
/// Pure/private helper results cannot be accepted as a JS completion.
pub(crate) struct GcCallResult {
    private: (),
}
impl GcCallResult {
    pub(super) fn emitted() -> Self {
        Self { private: () }
    }
}

/// Only the central schema relates semantic reference layouts to value tags.
pub(crate) trait JavaScriptReference: GcHeapType {
    const TAG: WasmRuntimeValueTag;
}
pub(crate) enum ObjectHeaderProjection {
    Own,
    Field(GcFieldOrdinal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeQueueEnd {
    Head,
    Tail,
}

impl RuntimeSchema {
    pub(crate) fn array_storage_mode_is(
        &self,
        storage: &GcLocal<ArrayIndexStorage>,
        mode: ArrayIndexedStorageMode,
        function: &mut Function,
    ) {
        storage.load(self, function);
        self.field(ArrayIndexStorageSchema::MODE).emit_get(function);
        function.instruction(&Instruction::I32Const(mode.encode()));
        function.instruction(&Instruction::I32Eq);
    }

    pub(crate) fn array_storage_is_packed(
        &self,
        storage: &GcLocal<ArrayIndexStorage>,
        function: &mut Function,
    ) {
        self.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedNumbers, function);
        self.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedValues, function);
        function.instruction(&Instruction::I32Or);
    }

    pub(crate) fn set_array_storage_mode(
        &self,
        storage: &GcLocal<ArrayIndexStorage>,
        mode: ArrayIndexedStorageMode,
        function: &mut Function,
    ) {
        self.field(ArrayIndexStorageSchema::MODE).write(
            storage,
            GcOperand::constant(mode),
            self,
            function,
        );
    }
    /// Mint one stable nonzero object/Symbol key identity. Exhaustion traps
    /// before zero can be published or a prior identity can be reused.
    pub(crate) fn emit_next_collection_key_hash_id(
        &self,
        output: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::GlobalGet(
            self.collection_key_hash_counter.index,
        ));
        output.store(function);
        output.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        output.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::GlobalSet(
            self.collection_key_hash_counter.index,
        ));
    }

    pub(crate) fn load_module_evaluation_promise(
        &self,
        function: &mut Function,
    ) -> GcLocal<PromiseObject, Nullable> {
        let slot = self.reserve_gc_local::<PromiseObject, Nullable>(function);
        emit_root_get(function, self.module_evaluation_promise);
        slot.initialize(GcStackReference::new(), function)
    }
    pub(crate) fn replace_module_evaluation_promise<N: GcFieldNullability>(
        &self,
        value: &GcLocal<PromiseObject, N>,
        function: &mut Function,
    ) {
        value.load(self, function);
        emit_root_set(function, self.module_evaluation_promise);
    }
    pub(crate) fn clear_module_evaluation_promise(&self, function: &mut Function) {
        function.instruction(&Instruction::RefNull(
            self.reference_type::<PromiseObject>(GcNullability::Nullable)
                .heap_type,
        ));
        emit_root_set(function, self.module_evaluation_promise);
    }
    fn pending_job_queue_root(&self, end: RuntimeQueueEnd) -> GcRootGlobal<PendingJob> {
        match end {
            RuntimeQueueEnd::Head => self.pending_jobs_head,
            RuntimeQueueEnd::Tail => self.pending_jobs_tail,
        }
    }
    fn unhandled_promise_queue_root(&self, end: RuntimeQueueEnd) -> GcRootGlobal<PromiseObject> {
        match end {
            RuntimeQueueEnd::Head => self.unhandled_promises_head,
            RuntimeQueueEnd::Tail => self.unhandled_promises_tail,
        }
    }
    pub(crate) fn load_pending_job_queue(
        &self,
        end: RuntimeQueueEnd,
        function: &mut Function,
    ) -> GcLocal<PendingJob, Nullable> {
        let slot = self.reserve_gc_local::<PendingJob, Nullable>(function);
        emit_root_get(function, self.pending_job_queue_root(end));
        slot.initialize(GcStackReference::new(), function)
    }
    pub(crate) fn replace_pending_job_queue<N: GcFieldNullability>(
        &self,
        end: RuntimeQueueEnd,
        value: &GcLocal<PendingJob, N>,
        function: &mut Function,
    ) {
        value.load(self, function);
        emit_root_set(function, self.pending_job_queue_root(end));
    }
    pub(crate) fn clear_pending_job_queue(&self, end: RuntimeQueueEnd, function: &mut Function) {
        function.instruction(&Instruction::RefNull(
            self.reference_type::<PendingJob>(GcNullability::Nullable)
                .heap_type,
        ));
        emit_root_set(function, self.pending_job_queue_root(end));
    }
    pub(crate) fn load_unhandled_promise_queue(
        &self,
        end: RuntimeQueueEnd,
        function: &mut Function,
    ) -> GcLocal<PromiseObject, Nullable> {
        let slot = self.reserve_gc_local::<PromiseObject, Nullable>(function);
        emit_root_get(function, self.unhandled_promise_queue_root(end));
        slot.initialize(GcStackReference::new(), function)
    }
    pub(crate) fn replace_unhandled_promise_queue<N: GcFieldNullability>(
        &self,
        end: RuntimeQueueEnd,
        value: &GcLocal<PromiseObject, N>,
        function: &mut Function,
    ) {
        value.load(self, function);
        emit_root_set(function, self.unhandled_promise_queue_root(end));
    }
    pub(crate) fn clear_unhandled_promise_queue(
        &self,
        end: RuntimeQueueEnd,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::RefNull(
            self.reference_type::<PromiseObject>(GcNullability::Nullable)
                .heap_type,
        ));
        emit_root_set(function, self.unhandled_promise_queue_root(end));
    }
    fn atomics_async_waiter_queue_root(
        &self,
        end: RuntimeQueueEnd,
    ) -> GcRootGlobal<AtomicsAsyncWaiter> {
        match end {
            RuntimeQueueEnd::Head => self.atomics_async_waiters_head,
            RuntimeQueueEnd::Tail => self.atomics_async_waiters_tail,
        }
    }
    pub(crate) fn load_atomics_async_waiter_queue(
        &self,
        end: RuntimeQueueEnd,
        function: &mut Function,
    ) -> GcLocal<AtomicsAsyncWaiter, Nullable> {
        let slot = self.reserve_gc_local::<AtomicsAsyncWaiter, Nullable>(function);
        emit_root_get(function, self.atomics_async_waiter_queue_root(end));
        slot.initialize(GcStackReference::new(), function)
    }
    pub(crate) fn replace_atomics_async_waiter_queue<N: GcFieldNullability>(
        &self,
        end: RuntimeQueueEnd,
        value: &GcLocal<AtomicsAsyncWaiter, N>,
        function: &mut Function,
    ) {
        value.load(self, function);
        emit_root_set(function, self.atomics_async_waiter_queue_root(end));
    }
    pub(crate) fn clear_atomics_async_waiter_queue(
        &self,
        end: RuntimeQueueEnd,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::RefNull(
            self.reference_type::<AtomicsAsyncWaiter>(GcNullability::Nullable)
                .heap_type,
        ));
        emit_root_set(function, self.atomics_async_waiter_queue_root(end));
    }
    pub(crate) fn load_symbol_registry(
        &self,
        function: &mut Function,
    ) -> GcLocal<RegisteredSymbolTable> {
        let slot = self.reserve_gc_local::<RegisteredSymbolTable, NonNullable>(function);
        emit_root_get(function, self.registered_symbols);
        function.instruction(&Instruction::RefAsNonNull);
        slot.initialize(GcStackReference::new(), function)
    }
    pub(crate) fn replace_symbol_registry(
        &self,
        table: &GcLocal<RegisteredSymbolTable>,
        function: &mut Function,
    ) {
        table.load(self, function);
        emit_root_set(function, self.registered_symbols);
    }
    pub(crate) fn empty_value_array(
        &self,
        function: &mut Function,
    ) -> GcStackReference<ValueArray> {
        self.empty_arrays.values.load(function)
    }

    pub(crate) fn empty_property_table(
        &self,
        function: &mut Function,
    ) -> GcStackReference<PropertyTable> {
        self.empty_arrays.properties.load(function)
    }

    pub(crate) fn empty_property_index(
        &self,
        function: &mut Function,
    ) -> GcStackReference<OrdinaryPropertyIndex> {
        self.empty_arrays.property_index.load(function)
    }

    pub(crate) fn empty_private_elements(
        &self,
        function: &mut Function,
    ) -> GcStackReference<PrivateElementTable> {
        self.empty_arrays.private_elements.load(function)
    }

    pub(crate) fn empty_array_buckets(
        &self,
        function: &mut Function,
    ) -> GcStackReference<ArrayIndexBucketTable> {
        self.empty_arrays.array_buckets.load(function)
    }

    pub(crate) fn empty_array_elements(
        &self,
        function: &mut Function,
    ) -> GcStackReference<IndexedTable> {
        self.empty_arrays.array_elements.load(function)
    }

    pub(crate) fn empty_number_bits(
        &self,
        function: &mut Function,
    ) -> GcStackReference<NumberBitsI64Array> {
        self.empty_arrays.number_bits.load(function)
    }

    pub(crate) fn empty_number_presence(
        &self,
        function: &mut Function,
    ) -> GcStackReference<NumberPresenceU64Array> {
        self.empty_arrays.number_presence.load(function)
    }

    pub(crate) fn stored_undefined(
        &self,
        function: &mut Function,
    ) -> GcStackReference<StoredValue> {
        self.stored_undefined.load(function)
    }

    pub(crate) fn pooled_strings_initialized(&self, function: &mut Function) {
        emit_root_get(function, self.pooled_strings);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
    }
    pub(crate) fn publish_pooled_strings(
        &self,
        table: &GcLocal<PooledStringTable>,
        function: &mut Function,
    ) {
        table.load(self, function);
        emit_root_set(function, self.pooled_strings);
    }
    pub(crate) fn publish_well_known_symbols(
        &self,
        table: &GcLocal<WellKnownSymbolTable>,
        function: &mut Function,
    ) {
        table.load(self, function);
        emit_root_set(function, self.well_known_symbols);
    }
    /// The pooled-string table the runtime module published.
    pub(crate) fn pooled_string_table_reference(
        &self,
        function: &mut Function,
    ) -> GcStackReference<PooledStringTable> {
        emit_root_get(function, self.pooled_strings);
        function.instruction(&Instruction::RefAsNonNull);
        GcStackReference::new()
    }
    pub(crate) fn pooled_string_reference(
        &self,
        index: crate::data::PooledStringIndex,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        let ordinal = self.reserve_i32_local(function);
        ordinal.set_constant(index.ordinal() as i32, function);
        let result = self.resolve_pooled_string_reference(ordinal, base, function);
        self.release_i32_local(ordinal, function);
        result
    }
    pub(crate) fn resolve_pooled_string_reference(
        &self,
        ordinal: I32Local,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        self.helper_reference_on_stack(self.call_helper(
            crate::runtime_helpers::PooledStringResolveArguments::new(ordinal),
            base,
            function,
        ))
    }
    pub(crate) fn well_known_symbol_reference(
        &self,
        symbol: scriptw_ir::WellKnownSymbol,
        function: &mut Function,
    ) -> GcStackReference<SymbolValue> {
        let ordinal = scriptw_ir::WellKnownSymbol::ALL
            .iter()
            .position(|candidate| *candidate == symbol)
            .expect("closed symbol row");
        emit_root_get(function, self.well_known_symbols);
        function.instruction(&Instruction::RefAsNonNull);
        function.instruction(&Instruction::I32Const(ordinal as i32));
        self.array_type::<WellKnownSymbolTable>().emit_get(function);
        GcStackReference::new()
    }
    /// Actual caller identity at the current source-call boundary. Opaque
    /// native entries install their own barrier identity; prepared Scripts and
    /// forwarding intrinsics remain transparent. Null denotes no active call.
    pub(crate) fn load_active_source_function(
        &self,
        function: &mut Function,
    ) -> GcStackReference<FunctionObject, Nullable> {
        emit_root_get(function, self.active_source_function);
        GcStackReference::new()
    }
    pub(crate) fn replace_active_source_function<N: GcFieldNullability>(
        &self,
        callable: &GcLocal<FunctionObject, N>,
        function: &mut Function,
    ) {
        callable.load(self, function);
        emit_root_set(function, self.active_source_function);
    }
    pub(crate) fn load_current_realm(
        &self,
        function: &mut Function,
    ) -> GcStackReference<RealmRecord> {
        emit_root_get(function, self.current_realm);
        function.instruction(&Instruction::RefAsNonNull);
        GcStackReference::new()
    }
    pub(crate) fn replace_current_realm(
        &self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) {
        realm.load(self, function);
        emit_root_set(function, self.current_realm);
    }
    pub(crate) fn clear_throw_diagnostic(
        &self,
        role: crate::module::ThrowDiagnosticRole,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::RefNull(
            self.reference_type::<StringValue>(GcNullability::Nullable)
                .heap_type,
        ));
        emit_root_set(function, self.throw_diagnostic_root(role));
    }
    pub(crate) fn load_throw_diagnostic(
        &self,
        role: crate::module::ThrowDiagnosticRole,
        function: &mut Function,
    ) -> GcLocal<StringValue, Nullable> {
        let slot = self.reserve_gc_local::<StringValue, Nullable>(function);
        emit_root_get(function, self.throw_diagnostic_root(role));
        slot.initialize(GcStackReference::new(), function)
    }
    pub(crate) fn replace_throw_diagnostic<N: GcFieldNullability>(
        &self,
        role: crate::module::ThrowDiagnosticRole,
        value: &GcLocal<StringValue, N>,
        function: &mut Function,
    ) {
        value.load(self, function);
        emit_root_set(function, self.throw_diagnostic_root(role));
    }
    pub(crate) fn store_throw_diagnostic(
        &self,
        role: crate::module::ThrowDiagnosticRole,
        value: &GcLocal<StringValue>,
        function: &mut Function,
    ) {
        value.load(self, function);
        emit_root_set(function, self.throw_diagnostic_root(role));
    }
    pub(crate) fn reserve_function_local<R: GcFunctionRole, N: GcFieldNullability>(
        &self,
        function: &mut Function,
    ) -> FunctionLocalSlot<R, N> {
        FunctionLocalSlot {
            index: function.reserve_typed_local(ValType::Ref(RefType {
                nullable: true,
                heap_type: HeapType::Concrete(R::SIGNATURE.type_index()),
            })),
            shape: PhantomData,
        }
    }
    pub(crate) fn reference_entry<R: GcEntryFunctionRole>(
        &self,
        entry: &crate::function_entry::PlannedFunctionEntry,
        function: &mut Function,
    ) -> Result<FunctionStackReference<R>, crate::EmitError> {
        if GcFunctionSignature::Entry(entry.signature()) != R::SIGNATURE {
            return Err(crate::EmitError::unsupported(
                "compiler callable role does not match its registered signature",
            ));
        }
        entry.emit_ref_func(function);
        Ok(FunctionStackReference { shape: PhantomData })
    }
    pub(crate) fn call_entry(
        &self,
        entry: &crate::function_entry::PlannedFunctionEntry,
        function: &mut Function,
    ) -> GcCallResult {
        entry.emit_direct_call_instruction(function);
        GcCallResult::emitted()
    }
    pub(crate) fn call_reference<R: GcEntryFunctionRole>(
        &self,
        callee: &FunctionLocal<R>,
        function: &mut Function,
    ) -> GcCallResult {
        callee.load(function);
        function.instruction(&Instruction::CallRef(R::SIGNATURE.type_index()));
        GcCallResult::emitted()
    }

    /// Ordinary JavaScript/native bodies return the complete Completion ABI.
    /// Resumable entries retain their activation-specific call continuation.
    pub(crate) fn return_call_ordinary_reference(
        &self,
        callee: &FunctionLocal<OrdinaryCallable>,
        function: &mut Function,
    ) {
        callee.load(function);
        function.instruction(&Instruction::ReturnCallRef(
            OrdinaryCallable::SIGNATURE.type_index(),
        ));
    }

    /// Consumes only a registered typed helper's already-emitted reference.
    /// No caller can invent a GC stack reference from a scalar or raw index.
    pub(crate) fn helper_reference_on_stack<T: GcHeapType, N: GcFieldNullability>(
        &self,
        result: crate::runtime_helpers::ReferenceHelperResult<T, N>,
    ) -> GcStackReference<T, N> {
        let _ = result;
        GcStackReference::new()
    }

    pub(crate) fn bind_helper_reference<T: GcHeapType, N: GcFieldNullability>(
        &self,
        result: crate::runtime_helpers::ReferenceHelperResult<T, N>,
        slot: GcLocalSlot<T, N>,
        function: &mut Function,
    ) -> GcLocal<T, N> {
        let _ = result;
        slot.initialize(GcStackReference::new(), function)
    }
    pub(crate) fn bind_regexp_compiler_program(
        &self,
        result: crate::runtime_helpers::RegExpCompileCallResult,
        slot: GcLocalSlot<RegExpProgram, Nullable>,
        function: &mut Function,
    ) -> GcLocal<RegExpProgram, Nullable> {
        let _ = result;
        slot.initialize(GcStackReference::new(), function)
    }
    pub(crate) fn bind_gathered_modules(
        &self,
        result: crate::runtime_helpers::ModuleGatherCallResult,
        slot: GcLocalSlot<ModuleRegistry>,
        function: &mut Function,
    ) -> GcLocal<ModuleRegistry> {
        let _ = result;
        slot.initialize(GcStackReference::new(), function)
    }
    pub(crate) fn reserve_i32_local(&self, function: &mut Function) -> I32Local {
        I32Local {
            index: function.reserve_typed_local(ValType::I32),
        }
    }
    pub(crate) fn reserve_i64_local(&self, function: &mut Function) -> I64Local {
        I64Local {
            index: function.reserve_typed_local(ValType::I64),
        }
    }
    pub(crate) fn reserve_f64_local(&self, function: &mut Function) -> F64Local {
        F64Local {
            index: function.reserve_typed_local(ValType::F64),
        }
    }
    pub(crate) fn reserve_value_local(&self, function: &mut Function) -> ValueLocals {
        ValueLocals {
            tag: self.reserve_i32_local(function),
            scalar: self.reserve_i64_local(function),
            reference: EqRefLocal {
                index: function.reserve_typed_local(ValType::Ref(RefType::EQREF)),
            },
        }
    }
    pub(crate) fn reserve_completion(&self, function: &mut Function) -> CompletionLocals {
        CompletionLocals {
            value: self.reserve_value_local(function),
            kind: self.reserve_i32_local(function),
            target: self.reserve_i32_local(function),
        }
    }
    pub(crate) fn reserve_gc_local<T: GcHeapType, N: GcFieldNullability>(
        &self,
        function: &mut Function,
    ) -> GcLocalSlot<T, N> {
        let reference_type = self.reference_type::<T>(GcNullability::Nullable);
        GcLocalSlot {
            index: function.reserve_typed_local(ValType::Ref(reference_type)),
            heap_type: reference_type.heap_type,
            shape: PhantomData,
        }
    }
    pub(crate) fn parameter_i32(&self, index: u32, function: &Function) -> I32Local {
        assert_eq!(
            function.parameter_type(index),
            Some(ValType::I32),
            "I32 parameter mismatch"
        );
        I32Local { index }
    }
    pub(crate) fn parameter_i64(&self, index: u32, function: &Function) -> I64Local {
        assert_eq!(
            function.parameter_type(index),
            Some(ValType::I64),
            "I64 parameter mismatch"
        );
        I64Local { index }
    }
    pub(crate) fn parameter_f64(&self, index: u32, function: &Function) -> F64Local {
        assert_eq!(
            function.parameter_type(index),
            Some(ValType::F64),
            "F64 parameter mismatch"
        );
        F64Local { index }
    }
    pub(crate) fn parameter_value(&self, index: u32, function: &mut Function) -> ValueLocals {
        assert_eq!(
            function.parameter_type(index),
            Some(ValType::I32),
            "value tag parameter mismatch"
        );
        assert_eq!(
            function.parameter_type(index + 1),
            Some(ValType::I64),
            "value scalar parameter mismatch"
        );
        assert_eq!(
            function.parameter_type(index + 2),
            Some(ValType::Ref(RefType::EQREF)),
            "value reference parameter mismatch"
        );
        let value = self.reserve_value_local(function);
        function.instruction(&Instruction::LocalGet(index));
        value.tag.store(function);
        function.instruction(&Instruction::LocalGet(index + 1));
        value.scalar.store(function);
        function.instruction(&Instruction::LocalGet(index + 2));
        function.instruction(&Instruction::LocalSet(value.reference.index));
        value
    }
    /// A registered Completion operand retains its value, kind and branch
    /// target together, including reference identity across a helper call.
    pub(crate) fn parameter_completion(
        &self,
        index: u32,
        function: &mut Function,
    ) -> CompletionLocals {
        let value = self.parameter_value(index, function);
        let kind = self.reserve_i32_local(function);
        let target = self.reserve_i32_local(function);
        self.parameter_i32(index + 3, function).load(function);
        kind.store(function);
        self.parameter_i32(index + 4, function).load(function);
        target.store(function);
        CompletionLocals {
            value,
            kind,
            target,
        }
    }
    pub(crate) fn parameter_gc<T: GcHeapType, N: GcFieldNullability>(
        &self,
        index: u32,
        function: &mut Function,
    ) -> GcLocal<T, N> {
        let expected = ValType::Ref(self.reference_type::<T>(if N::NULLABLE {
            GcNullability::Nullable
        } else {
            GcNullability::NonNullable
        }));
        assert_eq!(
            function.parameter_type(index),
            Some(expected),
            "typed GC parameter mismatch"
        );
        let slot = self.reserve_gc_local(function);
        function.instruction(&Instruction::LocalGet(index));
        slot.initialize(GcStackReference::new(), function)
    }
    pub(crate) fn release_i32_local(&self, local: I32Local, function: &mut Function) {
        function.release_typed_local(local.index);
    }
    pub(crate) fn release_i64_local(&self, local: I64Local, function: &mut Function) {
        function.release_typed_local(local.index);
    }
    pub(crate) fn release_f64_local(&self, local: F64Local, function: &mut Function) {
        function.release_typed_local(local.index);
    }
}

#[derive(Clone, Copy)]
enum OperandSource {
    I32(i32),
    ProgramLiteralCount(ProgramLiteralCount),
    I64(i64),
    F64(f64),
    Local(u32),
    NonNullLocal(u32),
    BooleanLocal(u32),
    Null(HeapType),
    DataDescriptor {
        writable: u32,
        enumerable: u32,
        configurable: u32,
    },
    AccessorDescriptor {
        enumerable: u32,
        configurable: u32,
    },
}
/// A field operand carries its exact schema value and nullability. There is no
/// conversion from a scalar operand to a typed reference operand.
pub(crate) struct GcOperand<'a, V: sealed::Sealed, N: GcFieldNullability> {
    source: OperandSource,
    shape: PhantomData<&'a (V, N)>,
}
impl<V: sealed::Sealed, N: GcFieldNullability> GcOperand<'_, V, N> {
    pub(super) fn emit(self, function: &mut Function) {
        let instruction = match self.source {
            OperandSource::I32(value) => Instruction::I32Const(value),
            OperandSource::ProgramLiteralCount(value) => {
                function.program_literal_count(value);
                return;
            }
            OperandSource::I64(value) => Instruction::I64Const(value),
            OperandSource::F64(value) => Instruction::F64Const(value.into()),
            OperandSource::Local(index) => Instruction::LocalGet(index),
            OperandSource::Null(ty) => Instruction::RefNull(ty),
            OperandSource::NonNullLocal(index) => {
                function.instruction(&Instruction::LocalGet(index));
                function.instruction(&Instruction::RefAsNonNull);
                return;
            }
            OperandSource::BooleanLocal(index) => {
                function.instruction(&Instruction::LocalGet(index));
                function.instruction(&Instruction::I32Eqz);
                function.instruction(&Instruction::I32Eqz);
                return;
            }
            OperandSource::DataDescriptor {
                writable,
                enumerable,
                configurable,
            } => {
                use crate::heap::StoredPropertyAttributes;
                function.instruction(&Instruction::I32Const(0));
                for (index, word) in [
                    (
                        writable,
                        StoredPropertyAttributes::Data {
                            writable: true,
                            enumerable: false,
                            configurable: false,
                        }
                        .descriptor_word(),
                    ),
                    (
                        enumerable,
                        StoredPropertyAttributes::Data {
                            writable: false,
                            enumerable: true,
                            configurable: false,
                        }
                        .descriptor_word(),
                    ),
                    (
                        configurable,
                        StoredPropertyAttributes::Data {
                            writable: false,
                            enumerable: false,
                            configurable: true,
                        }
                        .descriptor_word(),
                    ),
                ] {
                    emit_descriptor_boolean(index, word, function);
                }
                return;
            }
            OperandSource::AccessorDescriptor {
                enumerable,
                configurable,
            } => {
                use crate::heap::StoredPropertyAttributes;
                let base = StoredPropertyAttributes::Accessor {
                    enumerable: false,
                    configurable: false,
                }
                .descriptor_word();
                function.instruction(&Instruction::I32Const(base.as_i32()));
                for (index, word) in [
                    (
                        enumerable,
                        StoredPropertyAttributes::Data {
                            writable: false,
                            enumerable: true,
                            configurable: false,
                        }
                        .descriptor_word(),
                    ),
                    (
                        configurable,
                        StoredPropertyAttributes::Data {
                            writable: false,
                            enumerable: false,
                            configurable: true,
                        }
                        .descriptor_word(),
                    ),
                ] {
                    emit_descriptor_boolean(index, word, function);
                }
                return;
            }
        };
        function.instruction(&instruction);
    }
}
fn emit_descriptor_boolean(index: u32, word: crate::heap::DescriptorWord, function: &mut Function) {
    function.instruction(&Instruction::LocalGet(index));
    function.instruction(&Instruction::I32Eqz);
    function.instruction(&Instruction::I32Eqz);
    function.instruction(&Instruction::I32Const(word.as_i32()));
    function.instruction(&Instruction::I32Mul);
    function.instruction(&Instruction::I32Or);
}
pub(crate) trait GcI32Field: sealed::Sealed {}
pub(crate) trait GcNumericI32Field: GcI32Field {}
impl GcNumericI32Field for I32Value {}
impl GcNumericI32Field for I8Value {}
impl GcNumericI32Field for I16Value {}
pub(crate) trait GcI32Constant: GcI32Field {
    fn encode(self) -> i32;
}
impl<V: GcI32Constant> GcOperand<'_, V, NonNullable> {
    pub(crate) fn constant(value: V) -> Self {
        Self {
            source: OperandSource::I32(value.encode()),
            shape: PhantomData,
        }
    }
}
/// A live closed-domain scalar. Native option matching can write only a real
/// domain constant or copy the same domain; bare I32 locals cannot publish it.
#[must_use = "a closed-domain local must be consumed at scope retirement"]
pub(crate) struct GcI32DomainLocal<V: GcI32Constant> {
    local: I32Local,
    domain: PhantomData<fn() -> V>,
}
impl<V: GcI32Constant> GcI32DomainLocal<V> {
    pub(crate) fn new(schema: &RuntimeSchema, initial: V, function: &mut Function) -> Self {
        let value = Self {
            local: schema.reserve_i32_local(function),
            domain: PhantomData,
        };
        value.set_constant(initial, function);
        value
    }
    pub(crate) fn set_constant(&self, value: V, function: &mut Function) {
        function.instruction(&Instruction::I32Const(value.encode()));
        self.local.store(function);
    }
    pub(crate) fn copy_from(&self, source: &Self, function: &mut Function) {
        source.load(function);
        self.local.store(function);
    }
    pub(crate) fn load(&self, function: &mut Function) {
        self.local.load(function);
    }
    pub(crate) fn operand(&self) -> GcOperand<'_, V, NonNullable> {
        GcOperand {
            source: OperandSource::Local(self.local.index),
            shape: PhantomData,
        }
    }
    pub(crate) fn clear(self, schema: &RuntimeSchema, function: &mut Function) {
        schema.release_i32_local(self.local, function);
    }
}
impl<V: GcI32Constant> GcFieldRead<V, NonNullable> {
    pub(crate) fn store_domain(self, destination: &GcI32DomainLocal<V>, function: &mut Function) {
        destination.local.store(function);
    }
}

/// A validated native I64 domain, with no arbitrary numeric publication.
pub(crate) trait GcI64Constant: GcI64Field {
    fn encode(self) -> i64;
}
impl GcI64Constant for scriptw_intl::DateTimeFormatAvailability {
    fn encode(self) -> i64 {
        self.wire_code() as i64
    }
}
#[must_use = "a closed-domain local must be consumed at scope retirement"]
pub(crate) struct GcI64DomainLocal<V: GcI64Constant> {
    local: I64Local,
    domain: PhantomData<fn() -> V>,
}
impl<V: GcI64Constant> GcI64DomainLocal<V> {
    pub(crate) fn new(schema: &RuntimeSchema, initial: V, function: &mut Function) -> Self {
        let value = Self {
            local: schema.reserve_i64_local(function),
            domain: PhantomData,
        };
        value.set_constant(initial, function);
        value
    }
    pub(crate) fn set_constant(&self, value: V, function: &mut Function) {
        function.instruction(&Instruction::I64Const(value.encode()));
        self.local.store(function);
    }
    pub(crate) fn copy_from(&self, source: &Self, function: &mut Function) {
        source.load(function);
        self.local.store(function);
    }
    pub(crate) fn load(&self, function: &mut Function) {
        self.local.load(function);
    }
    pub(crate) fn operand(&self) -> GcOperand<'_, V, NonNullable> {
        GcOperand {
            source: OperandSource::Local(self.local.index),
            shape: PhantomData,
        }
    }
    pub(crate) fn clear(self, schema: &RuntimeSchema, function: &mut Function) {
        schema.release_i64_local(self.local, function);
    }
}
impl GcI64DomainLocal<scriptw_intl::DateTimeFormatAvailability> {
    pub(crate) fn set_checked_mask(&self, value: I64Local, function: &mut Function) {
        let allowed = scriptw_intl::DateTimeValueKind::ALL
            .iter()
            .copied()
            .fold(0_u64, |mask, kind| {
                mask | scriptw_intl::DateTimeFormatAvailability::mask_for(kind)
            });
        value.load(function);
        function.instruction(&Instruction::I64Const(!allowed as i64));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        value.load(function);
        self.local.store(function);
    }
}
impl<V: GcI64Constant> GcFieldRead<V, NonNullable> {
    pub(crate) fn store_i64_domain(
        self,
        destination: &GcI64DomainLocal<V>,
        function: &mut Function,
    ) {
        destination.local.store(function);
    }
}

macro_rules! intl_checked_count_local {
    ($domain:ty, $count:ty) => {
        impl GcI32DomainLocal<$domain> {
            /// GetNumberOption owns user RangeError and floor. This single
            /// publication boundary rejects an impossible invalid native count.
            pub(crate) fn set_checked_count(&self, value: I32Local, function: &mut Function) {
                value.load(function);
                function.instruction(&Instruction::I32Const(i32::from(<$count>::MIN)));
                function.instruction(&Instruction::I32LtU);
                value.load(function);
                function.instruction(&Instruction::I32Const(i32::from(<$count>::MAX)));
                function.instruction(&Instruction::I32GtU);
                function.instruction(&Instruction::I32Or);
                function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
                function.instruction(&Instruction::Unreachable);
                function.instruction(&Instruction::End);
                value.load(function);
                self.local.store(function);
            }
        }
    };
}
intl_checked_count_local!(
    scriptw_intl::number_format::options::IntegerDigitCount,
    scriptw_intl::number_format::options::IntegerDigitCount
);
intl_checked_count_local!(
    Option<scriptw_intl::number_format::options::FractionDigitCount>,
    scriptw_intl::number_format::options::FractionDigitCount
);
intl_checked_count_local!(
    Option<scriptw_intl::number_format::options::SignificantDigitCount>,
    scriptw_intl::number_format::options::SignificantDigitCount
);

impl GcI32Constant for bool {
    fn encode(self) -> i32 {
        i32::from(self)
    }
}
impl GcOperand<'_, bool, NonNullable> {
    pub(crate) fn boolean(value: bool) -> Self {
        Self::constant(value)
    }
    pub(crate) fn boolean_local(value: I32Local) -> Self {
        Self {
            source: OperandSource::BooleanLocal(value.index),
            shape: PhantomData,
        }
    }
}
pub(crate) trait GcI64Field: sealed::Sealed {}
pub(crate) trait GcNumericI64Field: GcI64Field {}
impl GcNumericI64Field for I64Value {}
impl GcOperand<'_, crate::heap::DescriptorWord, NonNullable> {
    pub(crate) fn data_descriptor_flags(
        writable: I32Local,
        enumerable: I32Local,
        configurable: I32Local,
    ) -> Self {
        Self {
            source: OperandSource::DataDescriptor {
                writable: writable.index,
                enumerable: enumerable.index,
                configurable: configurable.index,
            },
            shape: PhantomData,
        }
    }
    pub(crate) fn accessor_descriptor_flags(enumerable: I32Local, configurable: I32Local) -> Self {
        Self {
            source: OperandSource::AccessorDescriptor {
                enumerable: enumerable.index,
                configurable: configurable.index,
            },
            shape: PhantomData,
        }
    }
    pub(crate) fn descriptor_word(value: crate::heap::DescriptorWord) -> Self {
        Self {
            source: OperandSource::I32(value.as_i32()),
            shape: PhantomData,
        }
    }
}
pub(crate) trait GcF64Field: sealed::Sealed {}
impl<V: GcNumericI32Field> GcOperand<'_, V, NonNullable> {
    pub(crate) fn program_literal_count(value: ProgramLiteralCount) -> Self {
        Self {
            source: OperandSource::ProgramLiteralCount(value),
            shape: PhantomData,
        }
    }
    pub(crate) fn i32(value: i32) -> Self {
        Self {
            source: OperandSource::I32(value),
            shape: PhantomData,
        }
    }
    pub(crate) fn i32_local(value: I32Local) -> Self {
        Self {
            source: OperandSource::Local(value.index),
            shape: PhantomData,
        }
    }
}
impl<V: GcNumericI64Field> GcOperand<'_, V, NonNullable> {
    pub(crate) fn i64(value: i64) -> Self {
        Self {
            source: OperandSource::I64(value),
            shape: PhantomData,
        }
    }
    pub(crate) fn i64_local(value: I64Local) -> Self {
        Self {
            source: OperandSource::Local(value.index),
            shape: PhantomData,
        }
    }
}
impl<V: GcF64Field> GcOperand<'_, V, NonNullable> {
    pub(crate) fn f64(value: f64) -> Self {
        Self {
            source: OperandSource::F64(value),
            shape: PhantomData,
        }
    }
    pub(crate) fn f64_local(value: F64Local) -> Self {
        Self {
            source: OperandSource::Local(value.index),
            shape: PhantomData,
        }
    }
}
impl<'a, T: GcHeapType, N: GcFieldNullability> GcOperand<'a, GcRef<T>, N> {
    pub(crate) fn reference(value: &'a GcLocal<T, N>, _schema: &RuntimeSchema) -> Self {
        Self {
            source: if N::NULLABLE {
                OperandSource::Local(value.index)
            } else {
                OperandSource::NonNullLocal(value.index)
            },
            shape: PhantomData,
        }
    }
}
impl<'a, Base: GcHeapType> GcOperand<'a, GcRef<Base>, NonNullable> {
    pub(crate) fn subtype_reference<Sub: GcSubtype<Base>>(
        value: &'a GcLocal<Sub>,
        _schema: &RuntimeSchema,
    ) -> Self {
        Self {
            // GC locals are physically nullable so they can release roots.
            // A nominal upcast preserves the logical non-null requirement.
            source: OperandSource::NonNullLocal(value.index),
            shape: PhantomData,
        }
    }
}
impl<'a, T: GcHeapType> GcOperand<'a, GcRef<T>, Nullable> {
    pub(crate) fn nullable_reference(value: &'a GcLocal<T>, _schema: &RuntimeSchema) -> Self {
        Self {
            source: OperandSource::NonNullLocal(value.index),
            shape: PhantomData,
        }
    }
    pub(crate) fn null(schema: &RuntimeSchema) -> Self {
        Self {
            source: OperandSource::Null(
                schema
                    .reference_type::<T>(GcNullability::Nullable)
                    .heap_type,
            ),
            shape: PhantomData,
        }
    }
}
impl<'a> GcOperand<'a, AnyGcRef, Nullable> {
    pub(crate) fn eq_reference(value: &'a EqRefLocal) -> Self {
        Self {
            source: OperandSource::Local(value.index),
            shape: PhantomData,
        }
    }
}

impl<Owner: GcStructHeapType> GcStructType<Owner> {
    pub(super) fn construction_result(self) -> GcStackReference<Owner> {
        GcStackReference::new()
    }
}

impl GcStructType<StoredValue> {
    pub(crate) fn read_into(
        &self,
        stored: &GcLocal<StoredValue>,
        destination: &ValueLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.field(Self::TAG)
            .read(stored, schema, function)
            .store(destination.tag(), function);
        self.field(Self::SCALAR)
            .read(stored, schema, function)
            .store_i64(destination.scalar(), function);
        self.field(Self::REFERENCE)
            .read(stored, schema, function)
            .store_eq(destination.reference(), function);
    }

    pub(crate) fn from_value(
        self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> GcStackReference<StoredValue> {
        self.construct(
            (
                GcOperand::i64_local(value.scalar),
                GcOperand::i32_local(value.tag),
                GcOperand::eq_reference(&value.reference),
            ),
            function,
        )
    }
}

impl GcStructType<CompletionRecord> {
    pub(crate) fn from_completion(
        self,
        completion: &CompletionLocals,
        next: &GcLocal<CompletionRecord, Nullable>,
        return_stage: &GcI32DomainLocal<AsyncGeneratorReturnStage>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<CompletionRecord> {
        let stored = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(completion.value(), function),
                function,
            );
        let result = self.construct(
            (
                GcOperand {
                    source: OperandSource::Local(completion.kind.index),
                    shape: PhantomData,
                },
                GcOperand::reference(&stored, schema),
                GcOperand::i32_local(completion.target),
                GcOperand::reference(next, schema),
                return_stage.operand(),
            ),
            function,
        );
        stored.clear(function);
        result
    }
    pub(crate) fn read_into(
        &self,
        record: &GcLocal<CompletionRecord>,
        destination: &CompletionLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let stored = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                self.field(Self::VALUE)
                    .read(record, schema, function)
                    .reference(),
                function,
            );
        schema.struct_type::<StoredValue>().read_into(
            &stored,
            destination.value(),
            schema,
            function,
        );
        self.field(Self::KIND)
            .read(record, schema, function)
            .store(destination.kind, function);
        self.field(Self::TARGET)
            .read(record, schema, function)
            .store(destination.target, function);
        stored.clear(function);
    }
}

/// Reading a schema field produces exactly that field's storage shape.
pub(crate) struct GcFieldRead<V: sealed::Sealed, N: GcFieldNullability> {
    shape: PhantomData<fn() -> (V, N)>,
}
impl<O, V, M, N> GcFieldAccessor<O, V, M, N>
where
    O: GcStructHeapType,
    V: GcFieldValue<O, N>,
    M: GcFieldMutability,
    N: GcFieldNullability,
{
    pub(crate) fn read<R: GcFieldNullability>(
        &self,
        owner: &GcLocal<O, R>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcFieldRead<V, N> {
        owner.load(schema, function);
        self.emit_get(function);
        GcFieldRead { shape: PhantomData }
    }
}
impl<O, V, N> GcFieldAccessor<O, V, Mutable, N>
where
    O: GcStructHeapType,
    V: GcFieldValue<O, N>,
    N: GcFieldNullability,
{
    pub(crate) fn write<R: GcFieldNullability>(
        &self,
        owner: &GcLocal<O, R>,
        value: GcOperand<'_, V, N>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        owner.load(schema, function);
        value.emit(function);
        self.emit_set(function);
    }
}
impl<V: GcI32Field> GcFieldRead<V, NonNullable> {
    pub(crate) fn store(self, destination: I32Local, function: &mut Function) {
        destination.store(function);
    }
}
impl<V: GcI64Field> GcFieldRead<V, NonNullable> {
    pub(crate) fn store_i64(self, destination: I64Local, function: &mut Function) {
        destination.store(function);
    }
}
impl<V: GcF64Field> GcFieldRead<V, NonNullable> {
    pub(crate) fn store_f64(self, destination: F64Local, function: &mut Function) {
        destination.store(function);
    }
}
impl<T: GcHeapType, N: GcFieldNullability> GcFieldRead<GcRef<T>, N> {
    pub(crate) fn reference(self) -> GcStackReference<T, N> {
        GcStackReference::new()
    }
}
impl GcFieldRead<AnyGcRef, Nullable> {
    pub(crate) fn store_eq(self, destination: &EqRefLocal, function: &mut Function) {
        function.instruction(&Instruction::LocalSet(destination.index));
    }
}

impl<T, V, M, N> GcArrayType<T, V, M, N>
where
    T: GcArrayHeapType,
    V: GcFieldValue<T, N>,
    M: GcFieldMutability,
    N: GcFieldNullability,
{
    pub(crate) fn fixed<'a>(
        &self,
        elements: impl IntoIterator<Item = GcOperand<'a, V, N>>,
        function: &mut Function,
    ) -> GcStackReference<T>
    where
        V: 'a,
        N: 'a,
    {
        let mut count = 0u32;
        for element in elements {
            element.emit(function);
            count = count
                .checked_add(1)
                .expect("GC array element count overflow");
        }
        function.instruction(&Instruction::ArrayNewFixed {
            array_type_index: self.type_index().raw(),
            array_size: count,
        });
        GcStackReference::new()
    }
    pub(crate) fn filled(
        &self,
        element: GcOperand<'_, V, N>,
        length: I32Local,
        function: &mut Function,
    ) -> GcStackReference<T> {
        element.emit(function);
        length.load(function);
        function.instruction(&Instruction::ArrayNew(self.type_index().raw()));
        GcStackReference::new()
    }
    pub(crate) fn length<R: GcFieldNullability>(
        &self,
        owner: &GcLocal<T, R>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        owner.load(schema, function);
        function.instruction(&Instruction::ArrayLen);
    }
    pub(crate) fn read<R: GcFieldNullability>(
        &self,
        owner: &GcLocal<T, R>,
        index: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcFieldRead<V, N> {
        owner.load(schema, function);
        index.load(function);
        self.emit_get(function);
        GcFieldRead { shape: PhantomData }
    }
}

/// Primitive backing arrays are mutable only during their private construction.
/// Publication consumes the builder; no public mutable-array implementation is
/// declared for CodeUnitArray, BigIntLimbArray, PropertyKeyTable or ImmutableByteArray.
pub(crate) trait GcWritableArray: GcArrayHeapType + sealed::WritableArray {}
impl<T, V, N> GcArrayType<T, V, Mutable, N>
where
    T: GcWritableArray,
    V: GcFieldValue<T, N>,
    N: GcFieldNullability,
{
    pub(crate) fn write<R: GcFieldNullability>(
        &self,
        owner: &GcLocal<T, R>,
        index: I32Local,
        value: GcOperand<'_, V, N>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        owner.load(schema, function);
        index.load(function);
        value.emit(function);
        self.emit_set(function);
    }

    /// Copies the rooted prefix without exposing untyped array instructions.
    /// Source and destination have the same declared element and owner types.
    pub(crate) fn copy_prefix_from(
        &self,
        destination: &GcLocal<T>,
        source: &GcLocal<T>,
        length: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        destination.load(schema, function);
        function.instruction(&Instruction::I32Const(0));
        source.load(schema, function);
        function.instruction(&Instruction::I32Const(0));
        length.load(function);
        function.instruction(&Instruction::ArrayCopy {
            array_type_index_dst: self.type_index().raw(),
            array_type_index_src: self.type_index().raw(),
        });
    }
}

pub(crate) struct StringConstruction {
    units: GcLocal<CodeUnitArray>,
}
impl StringConstruction {
    /// Construct one exact UTF-16 unit without a fill followed by a copy.
    pub(crate) fn from_code_unit(
        schema: &RuntimeSchema,
        slot: GcLocalSlot<CodeUnitArray>,
        unit: I32Local,
        function: &mut Function,
    ) -> Self {
        Self {
            units: slot.initialize(
                schema
                    .array_type::<CodeUnitArray>()
                    .fixed([GcOperand::i32_local(unit)], function),
                function,
            ),
        }
    }

    pub(crate) fn allocate(
        schema: &RuntimeSchema,
        slot: GcLocalSlot<CodeUnitArray>,
        length: I32Local,
        function: &mut Function,
    ) -> Self {
        let array = schema.array_type::<CodeUnitArray>();
        Self {
            units: slot.initialize(array.filled(GcOperand::i32(0), length, function), function),
        }
    }
    pub(crate) fn write(
        &self,
        index: I32Local,
        unit: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.units.load(schema, function);
        index.load(function);
        unit.load(function);
        schema.array_type::<CodeUnitArray>().emit_set(function);
    }
    /// Copy exact UTF-16 units into the unpublished backing array. Published
    /// strings remain read-only; this capability belongs only to construction.
    pub(crate) fn copy_units_from(
        &self,
        destination_index: I32Local,
        source: &GcLocal<CodeUnitArray>,
        length: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.units.load(schema, function);
        destination_index.load(function);
        source.load(schema, function);
        function.instruction(&Instruction::I32Const(0));
        length.load(function);
        let index = schema.array_type::<CodeUnitArray>().type_index().raw();
        function.instruction(&Instruction::ArrayCopy {
            array_type_index_dst: index,
            array_type_index_src: index,
        });
    }
    /// Copy a selected interval into private construction storage, keeping
    /// partial slices independent of their source backing allocation.
    pub(crate) fn copy_units_range_from(
        &self,
        destination_index: I32Local,
        source: &GcLocal<CodeUnitArray>,
        source_index: I32Local,
        length: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.units.load(schema, function);
        destination_index.load(function);
        source.load(schema, function);
        source_index.load(function);
        length.load(function);
        let index = schema.array_type::<CodeUnitArray>().type_index().raw();
        function.instruction(&Instruction::ArrayCopy {
            array_type_index_dst: index,
            array_type_index_src: index,
        });
    }

    /// Fill a span with the exact UTF-16 prefix of a repeated immutable pattern.
    /// Seed one pattern, then copy the initialized prefix in doubling chunks.
    /// No published string is mutable and no intermediate strings are exposed.
    pub(crate) fn copy_repeated_units_from(
        &self,
        destination_index: I32Local,
        source: &GcLocal<CodeUnitArray>,
        extent: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let length = schema.reserve_i32_local(function);
        let copied = schema.reserve_i32_local(function);
        let count = schema.reserve_i32_local(function);
        let position = schema.reserve_i32_local(function);
        extent.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema
            .array_type::<CodeUnitArray>()
            .length(source, schema, function);
        length.store(function);
        length.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        // Repeat has zero extent for an empty source; pad returns before here.
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        extent.load(function);
        length.load(function);
        extent.load(function);
        length.load(function);
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::Select);
        count.store(function);
        self.copy_units_from(destination_index, source, count, schema, function);
        count.load(function);
        copied.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        copied.load(function);
        extent.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        extent.load(function);
        copied.load(function);
        function.instruction(&Instruction::I32Sub);
        count.store(function);
        count.load(function);
        copied.load(function);
        count.load(function);
        copied.load(function);
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::Select);
        count.store(function);
        destination_index.load(function);
        copied.load(function);
        function.instruction(&Instruction::I32Add);
        position.store(function);
        self.units.load(schema, function);
        position.load(function);
        self.units.load(schema, function);
        destination_index.load(function);
        count.load(function);
        let array = schema.array_type::<CodeUnitArray>().type_index().raw();
        function.instruction(&Instruction::ArrayCopy {
            array_type_index_dst: array,
            array_type_index_src: array,
        });
        copied.load(function);
        count.load(function);
        function.instruction(&Instruction::I32Add);
        copied.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i32_local(position, function);
        schema.release_i32_local(count, function);
        schema.release_i32_local(copied, function);
        schema.release_i32_local(length, function);
    }

    /// Reallocate this unpublished backing while preserving its used prefix.
    pub(crate) fn grow(
        &self,
        capacity: I32Local,
        used_length: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let grown = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<CodeUnitArray>()
                .filled(GcOperand::i32(0), capacity, function),
            function,
        );
        grown.load(schema, function);
        function.instruction(&Instruction::I32Const(0));
        self.units.load(schema, function);
        function.instruction(&Instruction::I32Const(0));
        used_length.load(function);
        let index = schema.array_type::<CodeUnitArray>().type_index().raw();
        function.instruction(&Instruction::ArrayCopy {
            array_type_index_dst: index,
            array_type_index_src: index,
        });
        self.units.replace(grown.load(schema, function), function);
        grown.clear(function);
    }
    /// Consume spare capacity without exposing it as observable String units.
    pub(crate) fn publish_prefix(
        self,
        length: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        schema
            .array_type::<CodeUnitArray>()
            .length(&self.units, schema, function);
        length.load(function);
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        // The owner is consumed below. Exact-capacity storage can be published
        // directly; only spare capacity requires an observable-length copy.
        let exact = Self::allocate(schema, schema.reserve_gc_local(function), length, function);
        let zero = schema.reserve_i32_local(function);
        zero.set_constant(0, function);
        exact.copy_units_from(zero, &self.units, length, schema, function);
        self.units
            .replace(exact.publish_units(schema, function), function);
        schema.release_i32_local(zero, function);
        function.instruction(&Instruction::End);
        self.publish(schema, function)
    }
    /// Transfer a completed private snapshot without publishing a JS String.
    /// Consuming construction prevents later compiler writes through this owner.
    pub(crate) fn publish_units(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<CodeUnitArray> {
        let result = self.units.load(schema, function);
        self.units.clear(function);
        result
    }

    pub(crate) fn publish(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        let result = schema.struct_type::<StringValue>().construct(
            (GcOperand::reference(&self.units, schema), GcOperand::i64(0)),
            function,
        );
        self.units.clear(function);
        result
    }
}

/// Filled bytes remain private until consuming publication. Published
/// ImmutableByteArray readers have no general mutable-array capability.
#[must_use = "an immutable byte construction must be published or cleared"]
pub(crate) struct ImmutableByteArrayConstruction {
    bytes: GcLocal<ImmutableByteArray>,
}
impl ImmutableByteArrayConstruction {
    pub(crate) fn allocate(
        schema: &RuntimeSchema,
        length: I32Local,
        function: &mut Function,
    ) -> Self {
        let slot = schema.reserve_gc_local(function);
        let bytes = slot.initialize(
            schema
                .array_type::<ImmutableByteArray>()
                .filled(GcOperand::i32(0), length, function),
            function,
        );
        Self { bytes }
    }
    pub(crate) fn write(
        &self,
        index: I32Local,
        byte: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.bytes.load(schema, function);
        index.load(function);
        byte.load(function);
        schema.array_type::<ImmutableByteArray>().emit_set(function);
    }
    pub(crate) fn publish(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<ImmutableByteArray> {
        let bytes = self.bytes.load(schema, function);
        self.bytes.clear(function);
        bytes
    }
    pub(crate) fn clear(self, function: &mut Function) {
        self.bytes.clear(function);
    }
}

/// RegExp programs use the same private byte construction and add their
/// concrete program brand only after the bytes are complete.
#[must_use = "a RegExp program construction must be published or cleared"]
pub(crate) struct RegExpProgramConstruction {
    bytes: ImmutableByteArrayConstruction,
}
impl RegExpProgramConstruction {
    pub(crate) fn allocate(
        schema: &RuntimeSchema,
        length: I32Local,
        function: &mut Function,
    ) -> Self {
        Self {
            bytes: ImmutableByteArrayConstruction::allocate(schema, length, function),
        }
    }
    pub(crate) fn write(
        &self,
        index: I32Local,
        byte: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.bytes.write(index, byte, schema, function);
    }
    pub(crate) fn publish(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<RegExpProgram> {
        let bytes = schema
            .reserve_gc_local(function)
            .initialize(self.bytes.publish(schema, function), function);
        let program = schema
            .struct_type::<RegExpProgram>()
            .construct((GcOperand::reference(&bytes, schema),), function);
        bytes.clear(function);
        program
    }
    pub(crate) fn clear(self, function: &mut Function) {
        self.bytes.clear(function);
    }
}

/// Private entry arrays cannot escape while their rows are being filled.
/// Publication consumes the sole write owner; generic array writes are absent.
macro_rules! iterator_entry_construction {
    ($builder:ident, $array:ty, $entry:ty) => {
        #[must_use = "a complete native iterator entry array must be published"]
        pub(crate) struct $builder {
            entries: GcLocal<$array>,
        }
        impl $builder {
            pub(crate) fn allocate(
                schema: &RuntimeSchema,
                seed: &GcLocal<$entry>,
                length: I32Local,
                function: &mut Function,
            ) -> Self {
                let entries = schema.reserve_gc_local(function).initialize(
                    schema.array_type::<$array>().filled(
                        GcOperand::reference(seed, schema),
                        length,
                        function,
                    ),
                    function,
                );
                Self { entries }
            }
            pub(crate) fn write(
                &self,
                index: I32Local,
                entry: &GcLocal<$entry>,
                schema: &RuntimeSchema,
                function: &mut Function,
            ) {
                self.entries.load(schema, function);
                index.load(function);
                entry.load(schema, function);
                schema.array_type::<$array>().emit_set(function);
            }
            pub(crate) fn publish(
                self,
                schema: &RuntimeSchema,
                function: &mut Function,
            ) -> GcStackReference<$array> {
                let result = self.entries.load(schema, function);
                self.entries.clear(function);
                result
            }
        }
    };
}
iterator_entry_construction!(
    IteratorConcatEntriesConstruction,
    IteratorConcatEntries,
    IteratorConcatEntry
);
iterator_entry_construction!(
    IteratorZipEntriesConstruction,
    IteratorZipEntries,
    IteratorZipEntry
);

/// A fresh ordinary Array starts in packed Number mode with every backing
/// empty. The registered indexed mutation owners alone publish closed modes;
/// logical Array length never determines a backing allocation size.
impl GcStructType<ArrayIndexStorage> {
    pub(crate) fn empty(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<ArrayIndexStorage> {
        let buckets = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_arrays.array_buckets.load(function), function);
        let dense = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_array_elements(function), function);
        let numbers = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_number_bits(function), function);
        let presence = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_number_presence(function), function);
        let result = self.construct(
            (
                GcOperand::reference(&buckets, schema),
                GcOperand::i64(0),
                GcOperand::reference(&dense, schema),
                GcOperand::constant(ArrayIndexedStorageMode::PackedNumbers),
                GcOperand::reference(&numbers, schema),
                GcOperand::reference(&presence, schema),
                GcOperand::null(schema),
            ),
            function,
        );
        dense.clear(function);
        presence.clear(function);
        numbers.clear(function);
        buckets.clear(function);
        result
    }
}

/// OwnKeys and ArraySetLength sort exactly the occupied indices in this private
/// buffer. Publication consumes write access; a JavaScript length never sizes it.
#[must_use = "publish or clear the occupied-index construction"]
pub(crate) struct ArrayIndexKeyConstruction {
    keys: GcLocal<ArrayIndexKeyTable>,
}
impl ArrayIndexKeyConstruction {
    pub(crate) fn allocate(
        schema: &RuntimeSchema,
        length: I32Local,
        function: &mut Function,
    ) -> Self {
        let keys = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ArrayIndexKeyTable>()
                .filled(GcOperand::i64(0), length, function),
            function,
        );
        Self { keys }
    }
    pub(crate) fn read(
        &self,
        index: I32Local,
        output: I64Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        schema
            .array_type::<ArrayIndexKeyTable>()
            .read(&self.keys, index, schema, function)
            .store_i64(output, function);
    }
    pub(crate) fn write(
        &self,
        index: I32Local,
        value: I64Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.keys.load(schema, function);
        index.load(function);
        value.load(function);
        schema.array_type::<ArrayIndexKeyTable>().emit_set(function);
    }
    pub(crate) fn publish(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<ArrayIndexKeyTable> {
        let result = self.keys.load(schema, function);
        self.keys.clear(function);
        result
    }
    pub(crate) fn clear(self, function: &mut Function) {
        self.keys.clear(function);
    }
}

/// OwnKeys and keyed iterator construction retain only validated String/Symbol
/// values. The mutable table is private until this owner is consumed.
pub(crate) struct PropertyKeyConstruction {
    keys: GcLocal<PropertyKeyTable>,
}
impl PropertyKeyConstruction {
    pub(crate) fn allocate(
        schema: &RuntimeSchema,
        slot: GcLocalSlot<PropertyKeyTable>,
        length: I32Local,
        function: &mut Function,
    ) -> Self {
        let zero = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        zero.store(function);
        let empty = StringConstruction::allocate(
            schema,
            schema.reserve_gc_local::<CodeUnitArray, NonNullable>(function),
            zero,
            function,
        );
        let string = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(empty.publish(schema, function), function);
        let key_value = schema.reserve_value_local(function);
        key_value.set_reference(&string, schema, function);
        let stored = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&key_value, function),
                function,
            );
        let keys = slot.initialize(
            schema.array_type::<PropertyKeyTable>().filled(
                GcOperand::reference(&stored, schema),
                length,
                function,
            ),
            function,
        );
        stored.clear(function);
        key_value.clear(function);
        string.clear(function);
        schema.release_i32_local(zero, function);
        Self { keys }
    }
    pub(crate) fn write(
        &self,
        index: I32Local,
        key: &crate::operations::PropertyKeyLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.keys.load(schema, function);
        index.load(function);
        schema
            .struct_type::<StoredValue>()
            .from_value(key.value(), function);
        schema.array_type::<PropertyKeyTable>().emit_set(function);
    }
    pub(crate) fn clear(self, function: &mut Function) {
        self.keys.clear(function);
    }
    pub(crate) fn publish(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<PropertyKeyTable> {
        let result = self.keys.load(schema, function);
        self.keys.clear(function);
        result
    }
}

/// Runtime segmentation fills a private boundary table. Publication consumes
/// the write owner after the caller has checked its complete partition.
#[must_use = "a segment boundary construction must be published or cleared"]
pub(crate) struct SegmentBoundaryConstruction {
    boundaries: GcLocal<SegmentBoundaryTable>,
}
impl SegmentBoundaryConstruction {
    pub(crate) fn allocate(
        schema: &RuntimeSchema,
        length: I32Local,
        function: &mut Function,
    ) -> Self {
        let default = schema
            .reserve_gc_local::<SegmentBoundary, NonNullable>(function)
            .initialize(
                schema.struct_type::<SegmentBoundary>().construct(
                    (GcOperand::i64(0), GcOperand::constant(None::<bool>)),
                    function,
                ),
                function,
            );
        let boundaries = schema.reserve_gc_local(function).initialize(
            schema.array_type::<SegmentBoundaryTable>().filled(
                GcOperand::reference(&default, schema),
                length,
                function,
            ),
            function,
        );
        default.clear(function);
        Self { boundaries }
    }
    pub(crate) fn write(
        &self,
        index: I32Local,
        row: &GcLocal<SegmentBoundary>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.boundaries.load(schema, function);
        index.load(function);
        row.load(schema, function);
        schema
            .array_type::<SegmentBoundaryTable>()
            .emit_set(function);
    }
    pub(crate) fn publish(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<SegmentBoundaryTable> {
        let result = self.boundaries.load(schema, function);
        self.boundaries.clear(function);
        result
    }
    pub(crate) fn clear(self, function: &mut Function) {
        self.boundaries.clear(function);
    }
}

pub(crate) struct BigIntConstruction {
    limbs: GcLocal<BigIntLimbArray>,
}
impl BigIntConstruction {
    pub(crate) fn allocate(
        schema: &RuntimeSchema,
        slot: GcLocalSlot<BigIntLimbArray>,
        length: I32Local,
        function: &mut Function,
    ) -> Self {
        let array = schema.array_type::<BigIntLimbArray>();
        Self {
            limbs: slot.initialize(array.filled(GcOperand::i64(0), length, function), function),
        }
    }
    pub(crate) fn reallocate(
        &self,
        length: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let array = schema.array_type::<BigIntLimbArray>();
        self.limbs
            .replace(array.filled(GcOperand::i64(0), length, function), function);
    }
    pub(crate) fn copy_from(&self, other: &Self, schema: &RuntimeSchema, function: &mut Function) {
        self.limbs
            .replace(other.limbs.load(schema, function), function);
    }
    pub(crate) fn write(
        &self,
        index: I32Local,
        limb: I64Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.limbs.load(schema, function);
        index.load(function);
        limb.load(function);
        schema.array_type::<BigIntLimbArray>().emit_set(function);
    }
    pub(crate) fn length(&self, output: I32Local, schema: &RuntimeSchema, function: &mut Function) {
        schema
            .array_type::<BigIntLimbArray>()
            .length(&self.limbs, schema, function);
        output.store(function);
    }
    pub(crate) fn read(
        &self,
        index: I32Local,
        output: I64Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        schema
            .array_type::<BigIntLimbArray>()
            .read(&self.limbs, index, schema, function)
            .store_i64(output, function);
    }
    pub(crate) fn clear(self, function: &mut Function) {
        self.limbs.clear(function);
    }
    pub(crate) fn publish(
        self,
        negative: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<BigIntValue> {
        let array = schema.array_type::<BigIntLimbArray>();
        let length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let limb = schema.reserve_i64_local(function);
        let normalized_negative = schema.reserve_i32_local(function);
        array.length(&self.limbs, schema, function);
        length.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        length.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        length.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        index.store(function);
        array
            .read(&self.limbs, index, schema, function)
            .store_i64(limb, function);
        limb.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        length.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        let canonical_slot = schema.reserve_gc_local::<BigIntLimbArray, NonNullable>(function);
        let canonical =
            canonical_slot.initialize(array.filled(GcOperand::i64(0), length, function), function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        array
            .read(&self.limbs, index, schema, function)
            .store_i64(limb, function);
        canonical.load(schema, function);
        index.load(function);
        limb.load(function);
        array.emit_set(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        negative.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Eqz);
        length.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        normalized_negative.store(function);
        let result = schema.struct_type::<BigIntValue>().construct(
            (
                GcOperand::boolean_local(normalized_negative),
                GcOperand::reference(&canonical, schema),
            ),
            function,
        );
        canonical.clear(function);
        schema.release_i32_local(normalized_negative, function);
        schema.release_i64_local(limb, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
        self.limbs.clear(function);
        result
    }
}

// Only a declared synchronous import can mint these canonical stack witnesses.
// The result discriminant is checked before emitting the direct native call.
impl crate::foreign::PlannedForeignFunction {
    pub(crate) fn call_string_utf16(
        &self,
        function: &mut Function,
    ) -> Result<GcStackReference<CodeUnitArray>, crate::EmitError> {
        if self.mode != scriptw_ir::foreign::ForeignFunctionMode::Synchronous
            || self.result != scriptw_ir::foreign::ForeignResultType::StringUtf16
        {
            return Err(crate::EmitError::unsupported(
                "foreign call has no synchronous UTF-16 result",
            ));
        }
        self.index.emit_call(function);
        Ok(GcStackReference::new())
    }

    pub(crate) fn call_bytes(
        &self,
        function: &mut Function,
    ) -> Result<GcStackReference<ByteArray>, crate::EmitError> {
        if self.mode != scriptw_ir::foreign::ForeignFunctionMode::Synchronous
            || self.result != scriptw_ir::foreign::ForeignResultType::Bytes
        {
            return Err(crate::EmitError::unsupported(
                "foreign call has no synchronous byte result",
            ));
        }
        self.index.emit_call(function);
        Ok(GcStackReference::new())
    }
}
