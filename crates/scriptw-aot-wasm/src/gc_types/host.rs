//! The Engine's consumed projections of the emitted GC schema.

use super::*;
use crate::{module::StaticSignature, WasmRuntimeValueTag};

/// Layouts consumed by the structured completion and native compiler boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcHostLayout {
    String,
    CodeUnits,
    BigInt,
    BigIntLimbs,
    ByteArray,
    ImmutableByteArray,
    PrivateNameTable,
    PrivateEnvironment,
    FunctionObject,
    DirectEvalExecutionContext,
    DirectEvalDerivedBindings,
    BindingCell,
    PrivateName,
    StoredValue,
    Completion,
    ValueArray,
    FunctionContext,
    Environment,
    ProgramInstance,
    RealmRecord,
    ModuleGraph,
}

impl GcHostLayout {
    pub const fn field_count(self) -> Option<u32> {
        match self {
            Self::String => Some(StringValue::FIELD_COUNT),
            Self::BigInt => Some(BigIntValue::FIELD_COUNT),
            Self::StoredValue => Some(StoredValue::FIELD_COUNT),
            Self::Completion => Some(CompletionRecord::FIELD_COUNT),
            Self::FunctionContext => Some(FunctionContext::FIELD_COUNT),
            Self::Environment => Some(Environment::FIELD_COUNT),
            Self::ProgramInstance => Some(ProgramInstance::FIELD_COUNT),
            Self::RealmRecord => Some(RealmRecord::FIELD_COUNT),
            Self::ModuleGraph => Some(ModuleGraph::FIELD_COUNT),
            Self::PrivateEnvironment => Some(PrivateEnvironment::FIELD_COUNT),
            Self::FunctionObject => Some(FunctionObject::FIELD_COUNT),
            Self::DirectEvalExecutionContext => Some(DirectEvalExecutionContext::FIELD_COUNT),
            Self::DirectEvalDerivedBindings => Some(DirectEvalDerivedBindings::FIELD_COUNT),
            Self::BindingCell => Some(BindingCell::FIELD_COUNT),
            Self::PrivateName => Some(PrivateName::FIELD_COUNT),

            Self::CodeUnits
            | Self::BigIntLimbs
            | Self::ByteArray
            | Self::ValueArray
            | Self::ImmutableByteArray
            | Self::PrivateNameTable => None,
        }
    }

    pub const fn array_mutable(self) -> Option<bool> {
        match self {
            Self::CodeUnits => {
                Some(<<CodeUnitArray as GcArrayHeapType>::Mutability as GcFieldMutability>::MUTABLE)
            }
            Self::BigIntLimbs => Some(
                <<BigIntLimbArray as GcArrayHeapType>::Mutability as GcFieldMutability>::MUTABLE,
            ),
            Self::ByteArray => {
                Some(<<ByteArray as GcArrayHeapType>::Mutability as GcFieldMutability>::MUTABLE)
            }
            Self::ImmutableByteArray => Some(
                <<ImmutableByteArray as GcArrayHeapType>::Mutability as GcFieldMutability>::MUTABLE,
            ),
            Self::PrivateNameTable => Some(
                <<PrivateNameTable as GcArrayHeapType>::Mutability as GcFieldMutability>::MUTABLE,
            ),
            Self::ValueArray => {
                Some(<<ValueArray as GcArrayHeapType>::Mutability as GcFieldMutability>::MUTABLE)
            }
            Self::String
            | Self::BigInt
            | Self::StoredValue
            | Self::Completion
            | Self::FunctionContext
            | Self::Environment
            | Self::ProgramInstance
            | Self::RealmRecord
            | Self::ModuleGraph
            | Self::PrivateEnvironment
            | Self::FunctionObject
            | Self::DirectEvalExecutionContext
            | Self::DirectEvalDerivedBindings
            | Self::BindingCell
            | Self::PrivateName => None,
        }
    }

    pub const fn array_element(self) -> Option<GcHostStorage> {
        match self {
            Self::CodeUnits => Some(
                <<CodeUnitArray as GcArrayHeapType>::Element as HostFieldStorage>::HOST_STORAGE,
            ),
            Self::BigIntLimbs => Some(
                <<BigIntLimbArray as GcArrayHeapType>::Element as HostFieldStorage>::HOST_STORAGE,
            ),
            Self::ByteArray => {
                Some(<<ByteArray as GcArrayHeapType>::Element as HostFieldStorage>::HOST_STORAGE)
            }
            Self::ValueArray => Some(reference_array_element::<ValueArray, StoredValue>()),
            Self::ImmutableByteArray => Some(
                <<ImmutableByteArray as GcArrayHeapType>::Element as HostFieldStorage>::HOST_STORAGE,
            ),
            Self::PrivateNameTable => Some(reference_array_element::<PrivateNameTable, PrivateName>()),
            Self::String
            | Self::BigInt
            | Self::StoredValue
            | Self::Completion
            | Self::FunctionContext
            | Self::Environment
            | Self::ProgramInstance
            | Self::RealmRecord
            | Self::ModuleGraph
            | Self::PrivateEnvironment
            | Self::FunctionObject
            | Self::DirectEvalExecutionContext
            | Self::DirectEvalDerivedBindings
            | Self::BindingCell
            | Self::PrivateName => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcHostStorage {
    I32,
    I64,
    PackedU8,
    PackedU16,
    NullableEqRef,
    FunctionReference {
        nullable: bool,
    },
    Reference {
        layout: GcHostLayout,
        nullable: bool,
    },
}

trait HostTarget: GcHeapType {
    const HOST_LAYOUT: GcHostLayout;
}
macro_rules! host_targets {
    ($($target:ty => $layout:ident),+ $(,)?) => { $(
        impl HostTarget for $target { const HOST_LAYOUT: GcHostLayout = GcHostLayout::$layout; }
    )+ };
}
host_targets!(StringValue => String, CodeUnitArray => CodeUnits,
    BigIntValue => BigInt, BigIntLimbArray => BigIntLimbs, ByteArray => ByteArray,
    StoredValue => StoredValue, CompletionRecord => Completion,
    ValueArray => ValueArray, FunctionContext => FunctionContext,
    Environment => Environment, ProgramInstance => ProgramInstance, RealmRecord => RealmRecord,
    ImmutableByteArray => ImmutableByteArray, PrivateNameTable => PrivateNameTable,
    PrivateEnvironment => PrivateEnvironment,
    FunctionObject => FunctionObject,
    ModuleGraph => ModuleGraph,
    DirectEvalExecutionContext => DirectEvalExecutionContext,
    DirectEvalDerivedBindings => DirectEvalDerivedBindings,
    BindingCell => BindingCell,
    PrivateName => PrivateName);

const fn reference_array_element<A, T>() -> GcHostStorage
where
    A: GcArrayHeapType<Element = GcRef<T>>,
    T: HostTarget,
{
    GcHostStorage::Reference {
        layout: T::HOST_LAYOUT,
        nullable: <A::Nullability as GcFieldNullability>::NULLABLE,
    }
}

trait HostFieldStorage {
    const HOST_STORAGE: GcHostStorage;
}
impl HostFieldStorage for I32Value {
    const HOST_STORAGE: GcHostStorage = GcHostStorage::I32;
}
impl HostFieldStorage for I8Value {
    const HOST_STORAGE: GcHostStorage = GcHostStorage::PackedU8;
}
impl HostFieldStorage for I16Value {
    const HOST_STORAGE: GcHostStorage = GcHostStorage::PackedU16;
}
impl HostFieldStorage for I64Value {
    const HOST_STORAGE: GcHostStorage = GcHostStorage::I64;
}
impl HostFieldStorage for bool {
    const HOST_STORAGE: GcHostStorage = GcHostStorage::I32;
}
impl HostFieldStorage for crate::emit::CompletionKind {
    const HOST_STORAGE: GcHostStorage = GcHostStorage::I32;
}
impl HostFieldStorage for AsyncGeneratorReturnStage {
    const HOST_STORAGE: GcHostStorage = GcHostStorage::I32;
}
impl HostFieldStorage for AnyGcRef {
    const HOST_STORAGE: GcHostStorage = GcHostStorage::NullableEqRef;
}

/// Closed field names replace host string lookup and duplicated numeric indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcHostField {
    StringCodeUnits,
    StringRawKeyHash,
    BigIntNegative,
    BigIntLimbs,
    StoredTag,
    StoredScalar,
    StoredReference,
    CompletionKind,
    CompletionValue,
    CompletionTarget,
    CompletionNext,
    CompletionReturnStage,
    ContextDefiningProgram,
    ContextRealm,
    ContextInstanceElementsInitializer,
    PrivateEnvironmentParent,
    PrivateEnvironmentClassScope,
    PrivateEnvironmentNames,
    PrivateNameDescription,
    DirectEvalClassContext,
    DirectEvalHomeObject,
    DirectEvalDerivedBindings,
    DirectEvalThisSnapshot,
    DirectEvalNewTargetSnapshot,
    DerivedThisCell,
    DerivedThisStatusCell,
    DerivedNewTargetCell,
    DerivedActiveFunctionCell,
    BindingCellValue,
    ContextActiveFunction,
    FunctionObjectContext,
    FunctionObjectDerivedConstructor,
    RealmModules,
    RealmGlobalEnvironment,
    ModuleGraphProgram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcHostFieldDefinition {
    layout: GcHostLayout,
    index: u32,
    storage: GcHostStorage,
    mutable: bool,
}
impl GcHostFieldDefinition {
    pub const fn layout(self) -> GcHostLayout {
        self.layout
    }
    pub const fn index(self) -> u32 {
        self.index
    }
    pub const fn storage(self) -> GcHostStorage {
        self.storage
    }
    pub const fn mutable(self) -> bool {
        self.mutable
    }
}

const fn scalar_field<O, V, M, N>(field: &GcField<O, V, M, N>) -> GcHostFieldDefinition
where
    O: GcStructHeapType + HostTarget,
    V: GcFieldValue<O, N> + HostFieldStorage,
    M: GcFieldMutability,
    N: GcFieldNullability,
{
    GcHostFieldDefinition {
        layout: O::HOST_LAYOUT,
        index: field.ordinal().raw(),
        storage: V::HOST_STORAGE,
        mutable: M::MUTABLE,
    }
}
const fn reference_field<O, T, M, N>(field: &GcField<O, GcRef<T>, M, N>) -> GcHostFieldDefinition
where
    O: GcStructHeapType + HostTarget,
    T: HostTarget,
    M: GcFieldMutability,
    N: GcFieldNullability,
{
    GcHostFieldDefinition {
        layout: O::HOST_LAYOUT,
        index: field.ordinal().raw(),
        storage: GcHostStorage::Reference {
            layout: T::HOST_LAYOUT,
            nullable: N::NULLABLE,
        },
        mutable: M::MUTABLE,
    }
}
const fn function_field<O, R, M, N>(
    field: &GcField<O, GcFunctionRef<R>, M, N>,
) -> GcHostFieldDefinition
where
    O: GcStructHeapType + HostTarget,
    R: GcFunctionRole,
    M: GcFieldMutability,
    N: GcFieldNullability,
{
    GcHostFieldDefinition {
        layout: O::HOST_LAYOUT,
        index: field.ordinal().raw(),
        storage: GcHostStorage::FunctionReference {
            nullable: N::NULLABLE,
        },
        mutable: M::MUTABLE,
    }
}
impl GcHostField {
    pub const fn definition(self) -> GcHostFieldDefinition {
        match self {
            Self::RealmModules => reference_field(&RealmRecordSchema::MODULES),
            Self::RealmGlobalEnvironment => reference_field(&RealmRecordSchema::GLOBAL_ENVIRONMENT),
            Self::ModuleGraphProgram => reference_field(&ModuleGraphSchema::PROGRAM),
            Self::ContextInstanceElementsInitializer => {
                function_field(&FunctionContextSchema::INSTANCE_ELEMENTS_INITIALIZER)
            }
            Self::StringCodeUnits => reference_field(&StringValueSchema::CODE_UNITS),
            Self::StringRawKeyHash => scalar_field(&StringValueSchema::RAW_KEY_HASH),
            Self::BigIntNegative => scalar_field(&BigIntValueSchema::NEGATIVE),
            Self::BigIntLimbs => reference_field(&BigIntValueSchema::LIMBS),
            Self::StoredTag => scalar_field(&StoredValueSchema::TAG),
            Self::StoredScalar => scalar_field(&StoredValueSchema::SCALAR),
            Self::StoredReference => scalar_field(&StoredValueSchema::REFERENCE),
            Self::CompletionKind => scalar_field(&CompletionRecordSchema::KIND),
            Self::CompletionValue => reference_field(&CompletionRecordSchema::VALUE),
            Self::CompletionTarget => scalar_field(&CompletionRecordSchema::TARGET),
            Self::CompletionNext => reference_field(&CompletionRecordSchema::NEXT),
            Self::CompletionReturnStage => scalar_field(&CompletionRecordSchema::RETURN_STAGE),
            Self::PrivateEnvironmentParent => reference_field(&PrivateEnvironmentSchema::PARENT),
            Self::PrivateEnvironmentClassScope => {
                scalar_field(&PrivateEnvironmentSchema::CLASS_SCOPE)
            }
            Self::PrivateEnvironmentNames => reference_field(&PrivateEnvironmentSchema::NAMES),
            Self::PrivateNameDescription => reference_field(&PrivateNameSchema::DESCRIPTION),
            Self::DirectEvalClassContext => {
                reference_field(&DirectEvalExecutionContextSchema::CLASS_CONTEXT)
            }
            Self::DirectEvalHomeObject => {
                reference_field(&DirectEvalExecutionContextSchema::HOME_OBJECT)
            }
            Self::DirectEvalDerivedBindings => {
                reference_field(&DirectEvalExecutionContextSchema::DERIVED_BINDINGS)
            }
            Self::DirectEvalThisSnapshot => {
                reference_field(&DirectEvalExecutionContextSchema::THIS_SNAPSHOT)
            }
            Self::DirectEvalNewTargetSnapshot => {
                reference_field(&DirectEvalExecutionContextSchema::NEW_TARGET_SNAPSHOT)
            }
            Self::DerivedThisCell => reference_field(&DirectEvalDerivedBindingsSchema::THIS_CELL),
            Self::DerivedThisStatusCell => {
                reference_field(&DirectEvalDerivedBindingsSchema::THIS_STATUS_CELL)
            }
            Self::DerivedNewTargetCell => {
                reference_field(&DirectEvalDerivedBindingsSchema::NEW_TARGET_CELL)
            }
            Self::DerivedActiveFunctionCell => {
                reference_field(&DirectEvalDerivedBindingsSchema::ACTIVE_FUNCTION_CELL)
            }
            Self::BindingCellValue => reference_field(&BindingCellSchema::VALUE),
            Self::ContextActiveFunction => reference_field(&FunctionContextSchema::ACTIVE_FUNCTION),
            Self::FunctionObjectContext => reference_field(&FunctionObjectSchema::CONTEXT),
            Self::FunctionObjectDerivedConstructor => {
                scalar_field(&FunctionObjectSchema::DERIVED_CONSTRUCTOR)
            }
            Self::ContextRealm => reference_field(&FunctionContextSchema::REALM),
            Self::ContextDefiningProgram => {
                reference_field(&FunctionContextSchema::DEFINING_PROGRAM)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcHostValueStorage {
    Scalar,
    String,
    BigInt,
    ReferenceCategory,
}

/// A host value is checked once before the Engine inspects its rooted reference.
/// The reference stays owned by the Store/RootScope, never in this scalar record.
#[derive(Debug)]
pub struct GcHostValue {
    tag: WasmRuntimeValueTag,
    scalar: i64,
    storage: GcHostValueStorage,
}
impl GcHostValue {
    pub fn check(tag: i32, scalar: i64, has_reference: bool) -> Result<Self, &'static str> {
        let tag = WasmRuntimeValueTag::from_tag(tag).ok_or("unknown Wasm value tag")?;
        let storage = match tag {
            WasmRuntimeValueTag::Undefined | WasmRuntimeValueTag::Null => {
                if scalar != 0 {
                    return Err("undefined/null scalar must be zero");
                }
                GcHostValueStorage::Scalar
            }
            WasmRuntimeValueTag::Boolean => {
                if !matches!(scalar, 0 | 1) {
                    return Err("Boolean scalar must be zero or one");
                }
                GcHostValueStorage::Scalar
            }
            WasmRuntimeValueTag::Number => GcHostValueStorage::Scalar,
            WasmRuntimeValueTag::String => GcHostValueStorage::String,
            WasmRuntimeValueTag::BigInt => GcHostValueStorage::BigInt,
            WasmRuntimeValueTag::Symbol
            | WasmRuntimeValueTag::Object
            | WasmRuntimeValueTag::Array
            | WasmRuntimeValueTag::Function
            | WasmRuntimeValueTag::Arguments => GcHostValueStorage::ReferenceCategory,
        };
        match storage {
            GcHostValueStorage::Scalar if has_reference => {
                return Err("scalar value has a GC reference");
            }
            GcHostValueStorage::Scalar => {}
            GcHostValueStorage::String
            | GcHostValueStorage::BigInt
            | GcHostValueStorage::ReferenceCategory => {
                if !has_reference {
                    return Err("reference value has no GC reference");
                }
                if scalar != 0 {
                    return Err("reference value scalar must be zero");
                }
            }
        }
        Ok(Self {
            tag,
            scalar,
            storage,
        })
    }
    pub const fn tag(&self) -> WasmRuntimeValueTag {
        self.tag
    }
    pub const fn scalar(&self) -> i64 {
        self.scalar
    }
    pub const fn storage(&self) -> GcHostValueStorage {
        self.storage
    }
}

/// Main exposes only Normal/Throw and never an unresolved local control target.
pub fn check_gc_main_completion(
    kind: i32,
    target: i32,
) -> Result<scriptw_ir::CompletionKindIr, &'static str> {
    if target != 0 {
        return Err("Main completion has a local control target");
    }
    match i64::from(kind) {
        crate::COMPLETION_KIND_NORMAL => Ok(scriptw_ir::CompletionKindIr::Normal),
        crate::COMPLETION_KIND_THROW => Ok(scriptw_ir::CompletionKindIr::Throw),
        _ => Err("Main completion is neither Normal nor Throw"),
    }
}

/// Native imports carry explicit resource operations or typed GC references.
/// JavaScript references never cross this boundary as scalar handles.
macro_rules! gc_host_imports {
    ($($import:ident => ($name:literal, $signature:ident)),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum GcHostImport { $($import),+ }
        impl GcHostImport {
            pub const ALL: &'static [Self] = &[$(Self::$import),+];
            pub const fn module(self) -> &'static str { crate::module::HOST_IMPORT_MODULE }
            pub const fn name(self) -> &'static str {
                match self { $(Self::$import => $name),+ }
            }
            pub(crate) const fn signature(self) -> StaticSignature {
                match self { $(Self::$import => StaticSignature::$signature),+ }
            }
        }
    };
}
gc_host_imports! {
    CollectGc => ("collect_gc", HostCollectGc),
    ByteArrayAllocate => ("byte_array_allocate", HostByteArrayAllocate),
    SharedBufferAllocate => ("shared_buffer_allocate", HostSharedBufferAllocate),
    SharedBufferBase => ("shared_buffer_base", HostSharedBufferBase),
    SharedBufferLength => ("shared_buffer_length", HostSharedBufferLength),
    SharedBufferMaximum => ("shared_buffer_maximum", HostSharedBufferMaximum),
    SharedBufferGrowable => ("shared_buffer_growable", HostSharedBufferGrowable),
    SharedBufferGrow => ("shared_buffer_grow", HostSharedBufferGrow),
    AgentBroadcastResource => ("agent_broadcast_resource", HostAgentBroadcastResource),
    AgentReceiveResource => ("agent_receive_resource", HostAgentReceiveResource),
    RegisterAsyncWaiter => ("register_async_waiter", HostRegisterAsyncWaiter),
    NotifyAsyncWaiters => ("notify_async_waiters", HostNotifyAsyncWaiters),
    IntlProviderCall => ("intl_provider_call", HostIntlProviderCall),
    SystemTimeZoneSnapshot => ("system_time_zone_snapshot", HostSystemTimeZoneSnapshot),
    SharedBufferWait => ("shared_buffer_wait", HostSharedBufferWait),
    WeakMapAllocate => ("weak_map_allocate", HostWeakMapAllocate),
    WeakSetAllocate => ("weak_set_allocate", HostWeakSetAllocate),
    WeakMapSet => ("weak_map_set", HostWeakMapSet),
    WeakMapGet => ("weak_map_get", HostWeakMapGet),
    WeakMapHas => ("weak_map_has", HostWeakMapHas),
    WeakMapDelete => ("weak_map_delete", HostWeakMapDelete),
    WeakSetAdd => ("weak_set_add", HostWeakSetAdd),
    WeakSetHas => ("weak_set_has", HostWeakSetHas),
    WeakSetDelete => ("weak_set_delete", HostWeakSetDelete),
    WeakRefAllocate => ("weak_ref_allocate", HostWeakRefAllocate),
    WeakRefDeref => ("weak_ref_deref", HostWeakRefDeref),
    WeakJobFinish => ("weak_job_finish", HostWeakJobFinish),
    FinalizationTrack => ("finalization_track", HostFinalizationTrack),
    FinalizationNextRegistry => ("finalization_next_registry", HostFinalizationNextRegistry),
    FinalizationCellAllocate => ("finalization_cell_allocate", HostFinalizationCellAllocate),
    FinalizationTargetAlive => ("finalization_target_alive", HostFinalizationTargetAlive),
    FinalizationTokenMatches => ("finalization_token_matches", HostFinalizationTokenMatches),
    FinalizationCellRemove => ("finalization_cell_remove", HostFinalizationCellRemove),
    ReportCleanupException => ("report_cleanup_exception", HostReportCleanupException),
    CompileDynamicFunction => ("compile_dynamic_function", HostCompileDynamicFunction),
    CompileGlobalScript => ("compile_global_script", HostCompileGlobalScript),
    CompileDirectEval => ("compile_direct_eval", HostCompileDirectEval),
}

/// Only declaration assigns a function index, together with its closed import
/// role. A native call cannot mistake this token for a JavaScript entry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeclaredGcHostImport {
    import: GcHostImport,
    function_index: u32,
}
impl DeclaredGcHostImport {
    pub(crate) fn emit_call_instruction(self, function: &mut Function) {
        function.instruction(&Instruction::Call(self.function_index));
    }
    pub(crate) fn broadcast_shared_buffer(
        self,
        resource: &GcLocal<HostResource>,
        id: I64Local,
        bigint: &GcLocal<ByteArray, Nullable>,
        result: I64Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        assert_eq!(self.import, GcHostImport::AgentBroadcastResource);
        let _ = schema
            .field(HostResourceSchema::RESOURCE)
            .read(resource, schema, function);
        id.load(function);
        bigint.load(schema, function);
        self.emit_call_instruction(function);
        result.store(function);
    }

    pub(crate) const fn import(self) -> GcHostImport {
        self.import
    }
}

pub(crate) struct GcHostImports {
    declarations: Vec<DeclaredGcHostImport>,
    source_compilation_profile: crate::RuntimeSourceCompilationProfile,
}
impl GcHostImports {
    pub(crate) const fn empty() -> Self {
        Self {
            declarations: Vec::new(),
            source_compilation_profile: crate::RuntimeSourceCompilationProfile::Portable,
        }
    }
    pub(crate) fn plan(
        next_function_index: &mut u32,
        shared_memory: bool,
        agents: bool,
        wait_async: bool,
        wait_sync: bool,
        intl_provider: bool,
        system_time_zone: bool,
        weak: bool,
        source_compilation_profile: crate::RuntimeSourceCompilationProfile,
    ) -> Self {
        let mut declarations = Vec::new();
        for import in GcHostImport::ALL {
            let needed = match import {
                GcHostImport::CollectGc | GcHostImport::ByteArrayAllocate => true,
                GcHostImport::CompileDynamicFunction
                | GcHostImport::CompileGlobalScript
                | GcHostImport::CompileDirectEval => {
                    source_compilation_profile == crate::RuntimeSourceCompilationProfile::NativeHost
                }
                GcHostImport::SharedBufferAllocate
                | GcHostImport::SharedBufferBase
                | GcHostImport::SharedBufferLength
                | GcHostImport::SharedBufferMaximum
                | GcHostImport::SharedBufferGrowable
                | GcHostImport::SharedBufferGrow
                | GcHostImport::NotifyAsyncWaiters => shared_memory,
                GcHostImport::AgentBroadcastResource | GcHostImport::AgentReceiveResource => agents,
                GcHostImport::RegisterAsyncWaiter => wait_async,
                GcHostImport::SharedBufferWait => wait_sync,
                GcHostImport::IntlProviderCall => intl_provider,
                GcHostImport::SystemTimeZoneSnapshot => system_time_zone,
                GcHostImport::WeakMapAllocate
                | GcHostImport::WeakSetAllocate
                | GcHostImport::WeakMapSet
                | GcHostImport::WeakMapGet
                | GcHostImport::WeakMapHas
                | GcHostImport::WeakMapDelete
                | GcHostImport::WeakSetAdd
                | GcHostImport::WeakSetHas
                | GcHostImport::WeakSetDelete
                | GcHostImport::WeakRefAllocate
                | GcHostImport::WeakRefDeref
                | GcHostImport::WeakJobFinish
                | GcHostImport::FinalizationTrack
                | GcHostImport::FinalizationNextRegistry
                | GcHostImport::FinalizationCellAllocate
                | GcHostImport::FinalizationTargetAlive
                | GcHostImport::FinalizationTokenMatches
                | GcHostImport::FinalizationCellRemove
                | GcHostImport::ReportCleanupException => weak,
            };
            if needed {
                declarations.push(DeclaredGcHostImport {
                    import: *import,
                    function_index: *next_function_index,
                });
                *next_function_index = next_function_index
                    .checked_add(1)
                    .expect("native GC import function-index overflow");
            }
        }
        Self {
            declarations,
            source_compilation_profile,
        }
    }
    pub(crate) const fn source_compilation_profile(
        &self,
    ) -> crate::RuntimeSourceCompilationProfile {
        self.source_compilation_profile
    }
    pub(crate) fn get(&self, import: GcHostImport) -> Option<DeclaredGcHostImport> {
        self.declarations
            .iter()
            .find(|entry| entry.import == import)
            .copied()
    }
    pub(crate) fn imports(&self) -> impl Iterator<Item = GcHostImport> + '_ {
        self.declarations.iter().map(|entry| entry.import)
    }
    pub(crate) fn emit_declarations(&self, imports: &mut wasm_encoder::ImportSection) {
        for entry in &self.declarations {
            imports.import(
                entry.import.module(),
                entry.import.name(),
                wasm_encoder::EntityType::Function(entry.import.signature().type_index()),
            );
        }
    }
}
