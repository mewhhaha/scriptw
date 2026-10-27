//! The consumed layout, value, callable and module-global authority for the
//! single Wasm-GC backend. Every function receives the same sealed RuntimeSchema;
//! semantic references remain typed references throughout construction and calls.
//! Private linear memory is limited to numeric wire and transient byte storage.

use core::marker::PhantomData;
use std::sync::{Arc, OnceLock};

use wasm_encoder::{
    BlockType, ConstExpr, Encode, GlobalSection, GlobalType, HeapType, Instruction, RefType,
    Section, StorageType, TypeSection, ValType,
};

use crate::Function;

mod layouts;
pub(crate) mod native_payload;
pub use layouts::ArrayIndexedStorageMode;
pub(crate) use layouts::*;

mod empty_arrays;
use empty_arrays::{EmptyArrayGlobal, EmptyArrays};
mod stored_undefined;
use stored_undefined::StoredUndefinedGlobal;
mod native_metadata_shapes;
mod normalization_tables;
use normalization_tables::NormalizationTablesGlobal;

mod value;
pub(crate) use value::*;
mod host;
pub use host::*;
mod snapshot;
mod source_activation_depth;
pub(crate) use source_activation_depth::SourceActivationDepth;
use source_activation_depth::SourceActivationDepthGlobal;
#[cfg(test)]
mod module_source_tests;
#[cfg(test)]
mod type_registration_tests;
pub(crate) use host::{DeclaredGcHostImport, GcHostImports};
pub use snapshot::*;

mod sealed {
    pub trait Sealed {}
    pub trait Struct {}
    pub trait Array {}
    pub trait WritableArray {}
    pub trait Subtype<Base> {}
}

/// Nullability is part of the actual signature/field contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GcNullability {
    Nullable,
    NonNullable,
}
impl GcNullability {
    pub(crate) const fn is_nullable(self) -> bool {
        matches!(self, Self::Nullable)
    }
}

/// Constructor-only invocation phase before any source continuation state.
pub(crate) const INITIALIZING_RESUME_POINT: i32 = -1;

/// Actual function operands. Concrete layout references resolve only after the
/// complete callable/layout index domain has been assigned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AbiType {
    I32,
    I64,
    F64,
    EqRef,
    ExternRef(GcNullability),
    Gc(GcLayout, GcNullability),
}
impl AbiType {
    pub(crate) fn resolve(self, layouts: &GcLayoutRegistry) -> ValType {
        match self {
            Self::I32 => ValType::I32,
            Self::I64 => ValType::I64,
            Self::F64 => ValType::F64,
            Self::EqRef => ValType::Ref(RefType::EQREF),
            Self::ExternRef(nullable) => ValType::Ref(RefType {
                nullable: nullable.is_nullable(),
                heap_type: RefType::EXTERNREF.heap_type,
            }),
            Self::Gc(layout, nullable) => ValType::Ref(layouts.reference(layout, nullable)),
        }
    }
}

/// Marker implemented only by layouts in the central Wasm-GC schema.
///
/// Keeping this sealed makes the schema the exhaustive source of heap types;
/// an emitter submodule cannot silently invent an unregistered layout.
pub(crate) trait GcHeapType: sealed::Sealed + Copy + 'static {
    const LAYOUT: GcLayout;
}

/// A sealed nominal Wasm GC subtype relation, declared beside its layouts.
pub(crate) trait GcSubtype<Base: GcHeapType>: GcHeapType + sealed::Subtype<Base> {}

/// A type-section index that can name only `T`'s declared Wasm-GC type.
///
/// The raw index remains available at the final `wasm-encoder` boundary, but
/// it cannot be exchanged with another heap type's index before that point.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub(crate) struct GcTypeIndex<T: GcHeapType> {
    raw: u32,
    ty: PhantomData<fn() -> T>,
}

impl<T: GcHeapType> GcTypeIndex<T> {
    const fn new(raw: u32) -> Self {
        Self {
            raw,
            ty: PhantomData,
        }
    }

    const fn raw(self) -> u32 {
        self.raw
    }
}

impl<T: GcHeapType> Clone for GcTypeIndex<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: GcHeapType> Copy for GcTypeIndex<T> {}

/// A field's declaration-order index within one GC struct type.
///
/// This is distinct from [`GcTypeIndex`], so a type index cannot accidentally
/// be passed to a `struct.get`/`struct.set` field position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub(crate) struct GcFieldOrdinal(u32);

impl GcFieldOrdinal {
    const fn new(raw: u32) -> Self {
        Self(raw)
    }

    const fn raw(self) -> u32 {
        self.0
    }
}

pub(crate) trait GcFieldMutability: sealed::Sealed + Copy + 'static {
    const MUTABLE: bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Immutable {}

impl sealed::Sealed for Immutable {}
impl GcFieldMutability for Immutable {
    const MUTABLE: bool = false;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mutable {}

impl sealed::Sealed for Mutable {}
impl GcFieldMutability for Mutable {
    const MUTABLE: bool = true;
}

pub(crate) trait GcFieldNullability: sealed::Sealed + Copy + 'static {
    const NULLABLE: bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NonNullable {}

impl sealed::Sealed for NonNullable {}
impl GcFieldNullability for NonNullable {
    const NULLABLE: bool = false;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Nullable {}

impl sealed::Sealed for Nullable {}
impl GcFieldNullability for Nullable {
    const NULLABLE: bool = true;
}

/// Scalar Wasm storage markers used by GC fields.
///
/// They are types rather than an enum value so an individual [`GcField`] owns
/// its storage contract at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum I32Value {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum I64Value {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum F64Value {}

impl sealed::Sealed for I32Value {}
impl sealed::Sealed for I64Value {}
impl sealed::Sealed for F64Value {}

/// The storage-type witness for a strong typed Wasm-GC reference.
///
/// This is intentionally a zero-sized schema marker, not a `u32`/`u64`
/// handle. Wasm references must remain references in emitted code. There is no
/// weak counterpart: the current Wasm-GC lower bound has no weak reference or
/// ephemeron storage type, so spelling one here would be a false capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GcRef<T: GcHeapType>(PhantomData<fn() -> T>);

impl<T: GcHeapType> sealed::Sealed for GcRef<T> {}

/// The index of one mutable, nullable Wasm global that roots a strong GC
/// reference to `T`.
///
/// The nullable state is the lifecycle boundary: the global is null before
/// the actual producer establishes the root. Its typed consumer owns replacement
/// and clearing; exported diagnostics remain rooted until the host has decoded them. This
/// type names the global slot only; it cannot contain a reference value, a
/// linear-memory address, or the index of a scalar global.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub(crate) struct GcRootGlobal<T: GcHeapType> {
    raw: u32,
    target: PhantomData<fn() -> T>,
}

impl<T: GcHeapType> GcRootGlobal<T> {
    const fn new(raw: u32) -> Self {
        Self {
            raw,
            target: PhantomData,
        }
    }

    const fn raw(self) -> u32 {
        self.raw
    }
    fn declare(layouts: &GcLayoutRegistry, globals: &mut GlobalLedger) -> Self {
        let root = Self::new(globals.len());
        let reference = layouts.reference(T::LAYOUT, GcNullability::Nullable);
        globals.global(
            GlobalType {
                val_type: ValType::Ref(reference),
                mutable: true,
                shared: false,
            },
            &ConstExpr::ref_null(reference.heap_type),
        );
        root
    }
}

impl<T: GcHeapType> Clone for GcRootGlobal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: GcHeapType> Copy for GcRootGlobal<T> {}

/// Sealed relation between a field owner, its value, and nullability.
///
/// Scalar fields are non-nullable. Strong and external
/// reference storage admits [`Nullable`], so `GcField<_, I64Value, _, Nullable>` does not type-check.
pub(crate) trait GcFieldValue<Owner, Nullability>: sealed::Sealed
where
    Owner: GcHeapType,
    Nullability: GcFieldNullability,
{
    const GET_KIND: GcStorageGet;
    fn storage_type(layouts: &GcLayoutRegistry) -> StorageType;
}

/// One field in one declared Wasm-GC struct.
///
/// `Owner`, `Value`, `Mutability`, and `Nullability` are all part of the type;
/// the encoder can therefore accept exactly the field shape an operation is
/// valid for instead of accepting four independent booleans and integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GcField<Owner, Value, Mutability, Nullability>
where
    Owner: GcHeapType,
    Value: GcFieldValue<Owner, Nullability>,
    Mutability: GcFieldMutability,
    Nullability: GcFieldNullability,
{
    ordinal: GcFieldOrdinal,
    shape: PhantomData<fn() -> (Owner, Value, Mutability, Nullability)>,
}

impl<Owner, Value, Mutability, Nullability> GcField<Owner, Value, Mutability, Nullability>
where
    Owner: GcHeapType,
    Value: GcFieldValue<Owner, Nullability>,
    Mutability: GcFieldMutability,
    Nullability: GcFieldNullability,
{
    const fn new(ordinal: GcFieldOrdinal) -> Self {
        Self {
            ordinal,
            shape: PhantomData,
        }
    }

    const fn ordinal(&self) -> GcFieldOrdinal {
        self.ordinal
    }
}

/// The module's globals in declaration order. Keeping the types beside the
/// encoded section lets a program module import the runtime's globals without
/// parsing the runtime back.
mod global_script_prelude;
pub(crate) use global_script_prelude::GlobalScriptPreludePhase;

pub(crate) struct GlobalLedger {
    entries: Vec<(GlobalType, ConstExpr)>,
}

impl GlobalLedger {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub(crate) fn global(&mut self, global_type: GlobalType, init: &ConstExpr) -> &mut Self {
        self.entries.push((global_type, init.clone()));
        self
    }

    pub(crate) fn len(&self) -> u32 {
        u32::try_from(self.entries.len()).expect("global count fits the Wasm index domain")
    }

    /// The types of the first `count` globals.
    pub(crate) fn types(&self, count: u32) -> Vec<GlobalType> {
        self.entries[..count as usize]
            .iter()
            .map(|(global_type, _)| *global_type)
            .collect()
    }

    /// The section declaring every global after the first `skip`.
    fn section_after(&self, skip: u32) -> GlobalSection {
        let mut section = GlobalSection::new();
        for (global_type, init) in &self.entries[skip as usize..] {
            section.global(*global_type, init);
        }
        section
    }
}

/// The runtime-visible GC portion of the module's central type registry.
///
/// Registration and global-section finalization are the only operations
/// exposed to module assembly. Raw type indices and field ordinals never leave
/// this module, so a caller cannot guess an index or pair a field with a
/// different owner before encoding. Finalizing the complete scalar global
/// section derives and appends the complete typed root set, then keeps its construction
/// and extraction private.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RuntimeGcTypes {
    layouts: Arc<GcLayoutRegistry>,
}

impl RuntimeGcTypes {
    fn from_layouts(layouts: Arc<GcLayoutRegistry>) -> Self {
        Self { layouts }
    }

    /// Consumes the complete open global section, derives the root from its
    /// actual next indices, appends them, and seals the section together with the
    /// only matching runtime schema.
    ///
    /// The per-program module once guards are declared last, after every root,
    /// so no root index depends on the program.
    fn finalize_globals(
        self,
        mut globals: GlobalLedger,
        snapshot: bool,
        module_guard_count: u32,
    ) -> FinalizedModuleGlobals {
        let normalization_tables = NormalizationTablesGlobal::declare(&mut globals);
        let pooled_strings = GcRootGlobal::declare(&self.layouts, &mut globals);
        let well_known_symbols = GcRootGlobal::declare(&self.layouts, &mut globals);
        let registered_symbols = GcRootGlobal::declare(&self.layouts, &mut globals);
        let current_realm = GcRootGlobal::declare(&self.layouts, &mut globals);
        let active_source_function = GcRootGlobal::declare(&self.layouts, &mut globals);
        let source_activation_depth = SourceActivationDepthGlobal::declare(&mut globals);
        let throw_name = GcRootGlobal::declare(&self.layouts, &mut globals);
        let throw_message = GcRootGlobal::declare(&self.layouts, &mut globals);
        let throw_constructor_name = GcRootGlobal::declare(&self.layouts, &mut globals);
        let pending_jobs_head = GcRootGlobal::declare(&self.layouts, &mut globals);
        let pending_jobs_tail = GcRootGlobal::declare(&self.layouts, &mut globals);
        let unhandled_promises_head = GcRootGlobal::declare(&self.layouts, &mut globals);
        let unhandled_promises_tail = GcRootGlobal::declare(&self.layouts, &mut globals);
        let module_evaluation_promise = GcRootGlobal::declare(&self.layouts, &mut globals);
        let collection_key_hash_counter = CollectionKeyHashCounterGlobal::declare(&mut globals);
        let atomics_async_waiters_head = GcRootGlobal::declare(&self.layouts, &mut globals);
        let atomics_async_waiters_tail = GcRootGlobal::declare(&self.layouts, &mut globals);
        let empty_arrays = EmptyArrays::declare(&self.layouts, &mut globals);
        let stored_undefined = StoredUndefinedGlobal::declare(&self.layouts, &mut globals);
        let configurable_native_metadata = GcRootGlobal::declare(&self.layouts, &mut globals);
        let fixed_native_metadata = GcRootGlobal::declare(&self.layouts, &mut globals);
        let snapshot =
            snapshot.then(|| snapshot::SnapshotRoots::declare(&self.layouts, &mut globals));
        let module_guards_first = globals.len();
        for _ in 0..module_guard_count {
            globals.global(
                GlobalType {
                    val_type: ValType::I32,
                    mutable: true,
                    shared: false,
                },
                &ConstExpr::i32_const(0),
            );
        }
        let runtime_schema = RuntimeSchema {
            types: self,
            normalization_tables,
            pooled_strings,
            well_known_symbols,
            registered_symbols,
            current_realm,
            active_source_function,
            source_activation_depth,
            throw_name,
            throw_message,
            throw_constructor_name,
            pending_jobs_head,
            pending_jobs_tail,
            unhandled_promises_head,
            unhandled_promises_tail,
            module_evaluation_promise,
            collection_key_hash_counter,
            atomics_async_waiters_head,
            atomics_async_waiters_tail,
            empty_arrays,
            stored_undefined,
            configurable_native_metadata,
            fixed_native_metadata,
            snapshot,
            module_guards: ModuleGuardGlobals {
                first: module_guards_first,
                count: module_guard_count,
            },
        };
        FinalizedModuleGlobals {
            global_prelude_phase: None,
            section: globals,
            runtime_schema,
        }
    }
}

/// Frozen type declarations and their exact assigned GC indices.
///
/// Registration consumes the section after singleton function signatures.
/// R contains only the core graph; P and standalone modules append direct
/// program-helper signatures after that graph without moving any core type.
/// Multi-member recursion groups make TypeSection::len a group-entry count;
/// callers cannot append or guess a type index through that section. The
/// registry may append validated P-only foreign signatures before finalizing
/// globals into the existing sealed package.
pub(crate) struct RuntimeModuleTypes {
    section: Arc<TypeSection>,
    next_type_index: u32,
    program_globals: bool,
    global_prelude: bool,
    runtime: RuntimeGcTypes,
}

/// Only immutable declaration bytes and their assigned type indices are shared.
/// Every registry still consumes its own globals and owns its own prelude flag.
struct RuntimeTypeDeclarations {
    section: Arc<TypeSection>,
    next_type_index: u32,
    layouts: Arc<GcLayoutRegistry>,
}

impl RuntimeModuleTypes {
    pub(crate) fn register(kind: crate::runtime_artifact::ModuleKind<'_>) -> Self {
        static RUNTIME: OnceLock<RuntimeTypeDeclarations> = OnceLock::new();
        static PROGRAM: OnceLock<RuntimeTypeDeclarations> = OnceLock::new();
        let program_globals = kind.compiles_program();
        let declarations = if program_globals { &PROGRAM } else { &RUNTIME }
            .get_or_init(|| Self::build_declarations(program_globals));
        Self {
            section: Arc::clone(&declarations.section),
            next_type_index: declarations.next_type_index,
            runtime: RuntimeGcTypes::from_layouts(Arc::clone(&declarations.layouts)),
            program_globals,
            global_prelude: false,
        }
    }

    /// Admission uses the same canonical recursive-group indices as emission.
    pub(crate) fn canonical_layouts() -> Arc<GcLayoutRegistry> {
        Self::register(crate::runtime_artifact::ModuleKind::Runtime(
            crate::RuntimeSourceCompilationProfile::Portable,
        ))
        .runtime
        .layouts
    }

    fn build_declarations(program_globals: bool) -> RuntimeTypeDeclarations {
        use crate::{
            module::StaticSignature,
            runtime_helpers::{HelperTypeRegion, RuntimeHelperId},
        };
        use wasm_encoder::{CompositeInnerType, CompositeType, FuncType, SubType};
        let function_count = StaticSignature::ALL.len() + RuntimeHelperId::recursive_type_count();
        let layouts = GcLayoutRegistry::assigned(
            u32::try_from(function_count).expect("function type count overflow"),
        );
        let mut section = TypeSection::new();
        let mut declared_types = 0u32;
        let mut members = Vec::new();
        let function_type = |definition: crate::module::StaticSignatureDefinition| SubType {
            is_final: true,
            supertype_idx: None,
            composite_type: CompositeType {
                inner: CompositeInnerType::Func(FuncType::new(
                    definition.parameters().iter().copied(),
                    definition.results().iter().copied(),
                )),
                shared: false,
                descriptor: None,
                describes: None,
            },
        };
        for signature in StaticSignature::ALL {
            let definition = signature.definition(&layouts);
            if signature.requires_runtime_group() {
                members.push(function_type(definition));
            } else {
                section.ty().function(
                    definition.parameters().iter().copied(),
                    definition.results().iter().copied(),
                );
                declared_types = declared_types.checked_add(1).expect("type count fits u32");
            }
        }
        for helper in RuntimeHelperId::ALL {
            if helper.requires_runtime_group() {
                members.push(function_type(helper.definition(&layouts)));
            }
        }
        members.extend(
            GcLayout::ALL
                .iter()
                .map(|layout| layout.definition(&layouts)),
        );
        declared_types = declared_types
            .checked_add(u32::try_from(members.len()).expect("recursive type count fits u32"))
            .expect("type count fits u32");
        section.ty().rec(members);
        // A runtime helper signature can refer to the complete GC schema
        // without making every helper part of its recursive identity. Separate
        // declarations let the native engine intern equal signatures once.
        for helper in RuntimeHelperId::ALL {
            if helper.type_region() == HelperTypeRegion::Core && !helper.requires_runtime_group() {
                let definition = helper.definition(&layouts);
                section.ty().function(
                    definition.parameters().iter().copied(),
                    definition.results().iter().copied(),
                );
                declared_types = declared_types.checked_add(1).expect("type count fits u32");
            }
        }
        if program_globals {
            for helper in RuntimeHelperId::ALL {
                if helper.type_region() == HelperTypeRegion::Program {
                    let definition = helper.definition(&layouts);
                    section.ty().function(
                        definition.parameters().iter().copied(),
                        definition.results().iter().copied(),
                    );
                    declared_types = declared_types.checked_add(1).expect("type count fits u32");
                }
            }
        }
        RuntimeTypeDeclarations {
            section: Arc::new(section),
            next_type_index: declared_types,
            layouts: Arc::new(layouts),
        }
    }

    pub(crate) fn with_global_prelude(mut self, present: bool) -> Self {
        assert!(
            !present || self.program_globals,
            "only P owns the global prelude phase"
        );
        self.global_prelude = present;
        self
    }

    /// Append only P's declared foreign ABI, after the complete shared GC and
    /// program-helper prefix. R's recursive identity and declaration bytes do
    /// not depend on an application's registry.
    pub(crate) fn with_foreign_modules(
        mut self,
        registry: &scriptw_ir::foreign::ForeignModuleRegistry,
    ) -> Result<(Self, crate::foreign::ForeignImportTypes), crate::EmitError> {
        if !registry.is_empty() && !self.program_globals {
            return Err(crate::EmitError::unsupported(
                "runtime module cannot declare foreign function types",
            ));
        }
        let mut registered = crate::foreign::ForeignImportTypes::default();
        if registry.is_empty() {
            return Ok((self, registered));
        }
        let section = Arc::make_mut(&mut self.section);
        for (id, declaration) in registry.functions() {
            let index = self.next_type_index;
            self.next_type_index = index.checked_add(1).ok_or_else(|| {
                crate::EmitError::unsupported("foreign function type exceeds the Wasm index space")
            })?;
            let mut parameters = declaration
                .parameters()
                .iter()
                .map(|parameter| {
                    crate::foreign::foreign_value_abi_type(*parameter)
                        .resolve(&self.runtime.layouts)
                })
                .collect::<Vec<_>>();
            let results = match declaration.mode() {
                scriptw_ir::foreign::ForeignFunctionMode::Synchronous => {
                    crate::foreign::foreign_result_abi_type(declaration.result())
                        .map(|result| result.resolve(&self.runtime.layouts))
                }
                scriptw_ir::foreign::ForeignFunctionMode::Asynchronous => {
                    let resolver = ValType::Ref(RefType {
                        nullable: false,
                        heap_type: RefType::EQREF.heap_type,
                    });
                    parameters.extend([resolver, resolver]);
                    None
                }
            };
            section.ty().function(parameters, results);
            registered.register_function(id, index);
        }
        if registry.error_tag().is_some() {
            let index = self.next_type_index;
            self.next_type_index = index.checked_add(1).ok_or_else(|| {
                crate::EmitError::unsupported("foreign error tag type exceeds the Wasm index space")
            })?;
            section.ty().function([ValType::I32], []);
            registered.register_error_tag(index);
        }
        Ok((self, registered))
    }

    pub(crate) fn finalize_globals(
        self,
        globals: GlobalLedger,
        snapshot: bool,
        module_guard_count: u32,
    ) -> FinalizedRuntimeModule {
        let mut globals = self
            .runtime
            .finalize_globals(globals, snapshot, module_guard_count);
        if self.global_prelude {
            globals.declare_global_prelude_phase();
        }
        FinalizedRuntimeModule {
            types: FinalizedModuleTypes {
                section: self.section,
            },
            globals,
        }
    }
}

/// Finalized type declarations expose only Wasm section encoding.
///
/// The raw section never leaves this module after recursive registration:
/// there is no extraction, mutable borrow, count query or append operation.
pub(crate) struct FinalizedModuleTypes {
    section: Arc<TypeSection>,
}

impl Encode for FinalizedModuleTypes {
    fn encode(&self, sink: &mut Vec<u8>) {
        self.section.encode(sink);
    }
}

impl Section for FinalizedModuleTypes {
    fn id(&self) -> u8 {
        self.section.id()
    }
}

/// One consume-once owner of the matching frozen types and rooted globals.
///
/// Only registration can construct this owner. Borrowed encoding/root views
/// cannot move its sections apart or combine their ownership across packages.
/// Module assembly carries the owner unchanged through main and code emission.
pub(crate) struct FinalizedRuntimeModule {
    types: FinalizedModuleTypes,
    globals: FinalizedModuleGlobals,
}

impl FinalizedRuntimeModule {
    pub(crate) fn types(&self) -> &FinalizedModuleTypes {
        &self.types
    }

    pub(crate) fn globals(&self) -> &FinalizedModuleGlobals {
        &self.globals
    }
}

/// Nonzero identity ordinals for object/Symbol collection keys. Only the
/// sealed module schema can declare or advance this scalar global.
#[derive(Debug, PartialEq, Eq)]
struct CollectionKeyHashCounterGlobal {
    index: u32,
}
impl CollectionKeyHashCounterGlobal {
    fn declare(globals: &mut GlobalLedger) -> Self {
        let index = globals.len();
        globals.global(
            GlobalType {
                val_type: ValType::I64,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i64_const(1),
        );
        Self { index }
    }
}

/// Complete, opaque runtime GC schema borrowed by every function builder.
///
/// Every semantic global carries the layout assigned by the same registry.
/// Declaration and use cannot acquire independently supplied raw indices.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RuntimeSchema {
    types: RuntimeGcTypes,
    normalization_tables: NormalizationTablesGlobal,
    pooled_strings: GcRootGlobal<PooledStringTable>,
    well_known_symbols: GcRootGlobal<WellKnownSymbolTable>,
    registered_symbols: GcRootGlobal<RegisteredSymbolTable>,
    current_realm: GcRootGlobal<RealmRecord>,
    active_source_function: GcRootGlobal<FunctionObject>,
    source_activation_depth: SourceActivationDepthGlobal,
    throw_name: GcRootGlobal<StringValue>,
    throw_message: GcRootGlobal<StringValue>,
    throw_constructor_name: GcRootGlobal<StringValue>,
    pending_jobs_head: GcRootGlobal<PendingJob>,
    pending_jobs_tail: GcRootGlobal<PendingJob>,
    unhandled_promises_head: GcRootGlobal<PromiseObject>,
    unhandled_promises_tail: GcRootGlobal<PromiseObject>,
    module_evaluation_promise: GcRootGlobal<PromiseObject>,
    collection_key_hash_counter: CollectionKeyHashCounterGlobal,
    atomics_async_waiters_head: GcRootGlobal<AtomicsAsyncWaiter>,
    atomics_async_waiters_tail: GcRootGlobal<AtomicsAsyncWaiter>,
    empty_arrays: EmptyArrays,
    stored_undefined: StoredUndefinedGlobal,
    configurable_native_metadata: GcRootGlobal<CompactDataShape>,
    fixed_native_metadata: GcRootGlobal<CompactDataShape>,
    snapshot: Option<snapshot::SnapshotRoots>,
    module_guards: ModuleGuardGlobals,
}

/// The once-guard globals, declared after every root.
#[derive(Debug, PartialEq, Eq)]
struct ModuleGuardGlobals {
    first: u32,
    count: u32,
}

impl RuntimeSchema {
    pub(crate) fn module_unit_guard(
        &self,
        unit: u32,
    ) -> Result<crate::planning::ModuleUnitGuard, crate::EmitError> {
        if unit >= self.module_guards.count {
            return Err(crate::EmitError::unsupported(
                "module once guard requires its declared unit",
            ));
        }
        Ok(crate::planning::ModuleUnitGuard::new(
            self.module_guards.first + unit,
        ))
    }

    pub(crate) fn layouts(&self) -> &GcLayoutRegistry {
        &self.types.layouts
    }
    pub(crate) fn struct_type<T: GcStructHeapType>(&self) -> GcStructType<T> {
        self.types.layouts.struct_type()
    }
    /// Bind a declaration-owned field to this module's registered struct type.
    pub(crate) fn field<O, V, M, N>(
        &self,
        field: GcField<O, V, M, N>,
    ) -> GcFieldAccessor<O, V, M, N>
    where
        O: GcStructHeapType,
        V: GcFieldValue<O, N>,
        M: GcFieldMutability,
        N: GcFieldNullability,
    {
        self.struct_type::<O>().field(field)
    }

    pub(crate) fn array_type<T: GcArrayHeapType>(
        &self,
    ) -> GcArrayType<T, T::Element, T::Mutability, T::Nullability> {
        self.types.layouts.array_type()
    }
    pub(crate) fn reference_type<T: GcHeapType>(&self, nullable: GcNullability) -> RefType {
        self.types.layouts.reference(T::LAYOUT, nullable)
    }
    pub(crate) fn signature(
        &self,
        signature: crate::module::StaticSignature,
    ) -> crate::module::StaticSignatureDefinition {
        signature.definition(&self.types.layouts)
    }

    fn throw_diagnostic_root(
        &self,
        role: crate::module::ThrowDiagnosticRole,
    ) -> GcRootGlobal<StringValue> {
        match role {
            crate::module::ThrowDiagnosticRole::Name => self.throw_name,
            crate::module::ThrowDiagnosticRole::Message => self.throw_message,
            crate::module::ThrowDiagnosticRole::ConstructorName => self.throw_constructor_name,
        }
    }

    pub(crate) fn export_throw_diagnostics(&self, exports: &mut wasm_encoder::ExportSection) {
        for role in crate::module::ThrowDiagnosticRole::ALL {
            exports.export(
                role.export_name(),
                wasm_encoder::ExportKind::Global,
                self.throw_diagnostic_root(*role).raw(),
            );
        }
    }
}

/// A global section sealed after the runtime root, paired with the only schema
/// whose typed global index names that section.
///
/// The raw section and schema are both private. This value implements the Wasm
/// section traits itself, and main holds a reference to this exact package for
/// its opaque lifecycle operations. No caller can clone the raw
/// [`GlobalLedger`], append another global, extract a copyable schema, or pair
/// lifecycle instructions from one package with another package's section.
pub(crate) struct FinalizedModuleGlobals {
    section: GlobalLedger,
    global_prelude_phase: Option<GlobalScriptPreludePhase>,
    runtime_schema: RuntimeSchema,
}

impl FinalizedModuleGlobals {
    fn declare_global_prelude_phase(&mut self) {
        assert!(self.global_prelude_phase.is_none());
        self.global_prelude_phase = Some(GlobalScriptPreludePhase::declare(&mut self.section));
    }
    pub(crate) fn global_prelude_phase(&self) -> Option<GlobalScriptPreludePhase> {
        self.global_prelude_phase
    }

    pub(crate) fn schema(&self) -> &RuntimeSchema {
        &self.runtime_schema
    }

    /// The globals every program shares: all of them but the program's own
    /// once guards, which are declared last.
    pub(crate) fn runtime_global_types(&self) -> Vec<GlobalType> {
        self.section.types(self.runtime_schema.module_guards.first)
    }

    /// Borrowed encoding of the globals after the imported prefix. The raw
    /// encoder stays inside this owner and cannot be cloned or extended by a
    /// module assembly caller.
    pub(crate) fn defined_section(&self, imported: u32) -> impl Section + '_ {
        struct DefinedGlobalSection<'a> {
            ledger: &'a GlobalLedger,
            imported: u32,
        }

        impl Encode for DefinedGlobalSection<'_> {
            fn encode(&self, sink: &mut Vec<u8>) {
                self.ledger.section_after(self.imported).encode(sink);
            }
        }

        impl Section for DefinedGlobalSection<'_> {
            fn id(&self) -> u8 {
                wasm_encoder::SectionId::Global.into()
            }
        }

        DefinedGlobalSection {
            ledger: &self.section,
            imported,
        }
    }
}

fn emit_root_get<T: GcHeapType>(function: &mut Function, root: GcRootGlobal<T>) {
    function.instruction(&Instruction::GlobalGet(root.raw()));
}

fn emit_root_set<T: GcHeapType>(function: &mut Function, root: GcRootGlobal<T>) {
    function.instruction(&Instruction::GlobalSet(root.raw()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursive_callable_and_semantic_declarations_validate_together() {
        let registered =
            RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
                crate::RuntimeSourceCompilationProfile::Portable,
            ));
        let runtime = registered.finalize_globals(GlobalLedger::new(), false, 0);
        let mut module = wasm_encoder::Module::new();
        module.section(runtime.types());
        module.section(&runtime.globals().defined_section(0));
        let bytes = module.finish();
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
            .validate_all(&bytes)
            .expect("typed callable fields and semantic edges share one assigned recursion group");
        let mut group_sizes = Vec::new();
        for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
            if let wasmparser::Payload::TypeSection(section) = payload.unwrap() {
                for group in section {
                    let size = group.unwrap().types().len();
                    group_sizes.extend(std::iter::repeat_n(size, size));
                }
            }
        }
        for signature in crate::module::StaticSignature::ALL {
            let size = group_sizes[signature.type_index() as usize];
            if signature.requires_runtime_group() {
                assert!(
                    size > 1,
                    "{signature:?} must retain its concrete recursive references"
                );
            } else {
                assert_eq!(
                    size, 1,
                    "{signature:?} must canonicalize independently for native host binding"
                );
            }
        }
    }

    #[test]
    fn finalized_globals_bind_the_root_to_the_actual_next_index() {
        let base_global_count =
            RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
                crate::RuntimeSourceCompilationProfile::Portable,
            ))
            .finalize_globals(GlobalLedger::new(), false, 0)
            .globals
            .section
            .len();
        for existing_global_count in [0_u32, 1, 7] {
            for snapshot in [false, true] {
                let runtime =
                    RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
                        crate::RuntimeSourceCompilationProfile::Portable,
                    ));
                let mut globals = GlobalLedger::new();
                for _ in 0..existing_global_count {
                    globals.global(
                        GlobalType {
                            val_type: ValType::I64,
                            mutable: true,
                            shared: false,
                        },
                        &ConstExpr::i64_const(0),
                    );
                }

                let finalized = runtime.finalize_globals(globals, snapshot, 0);
                assert_eq!(
                    finalized.globals.runtime_schema.pooled_strings.raw(),
                    existing_global_count,
                    "the typed root must bind the encoded section's actual next index"
                );
                assert_eq!(
                    finalized.globals.runtime_schema.snapshot.is_some(),
                    snapshot
                );
                let snapshot_global_count = if snapshot {
                    2 + u32::try_from(GcSnapshotLayout::ALL.len()).unwrap()
                } else {
                    0
                };
                assert_eq!(
                    finalized.globals.section.len(),
                    existing_global_count + base_global_count + snapshot_global_count,
                    "ordinary roots and opt-in typed inventory/witnesses share the actual section"
                );
            }
        }
    }
}
