//! Typed local operands, struct construction and the live scalar/reference ABI.

use super::*;
use crate::code_sink::LocalConstant;
use crate::WasmRuntimeValueTag;

mod binding_cell;
mod code_unit_kernels;
mod compact_array_recipes;
mod instance_elements;
mod native_direct_eval_compiler;
mod native_function_compiler;
mod native_global_script_compiler;
mod object_header_projection;
mod packed_value_list;
mod program_initialize;
mod program_instance;
mod prototype_reference;
mod string_accumulator;
mod tag_set;
pub(crate) use code_unit_kernels::{
    emit_ascii_narrow_kernel, emit_ascii_widen_helper_body, emit_code_unit_find_kernel,
    emit_json_plain_run_helper_body,
};
pub(crate) use packed_value_list::PackedValueListConstruction;
pub(crate) use program_instance::{
    FunctionProgramOwner, ProgramInstanceHooks, ProgramInstanceImage, ProgramInstanceLocals,
    ProgramLiteralCount, ProgramLiteralIndex, ProgramLiteralRoots, ProgramMetadataBytes,
    ProgramMetadataView,
};
pub(crate) use string_accumulator::StringAccumulatorCursor;
pub(crate) use tag_set::{
    emit_i32_in, emit_tag_in, TagSet, OBJECT_OR_NULL_TAGS, OBJECT_TAGS, PRIMITIVE_TAGS,
};
mod program_string_descriptors;
mod runtime_string_descriptors;
pub(crate) use program_string_descriptors::{
    ProgramStringDescriptorRowLocals, ProgramStringDescriptorTable,
};
pub(crate) use runtime_string_descriptors::RuntimeStringDescriptorRowLocals;
mod weak_host;
pub(crate) use weak_host::WeakEntryIdentities;

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
    pub(crate) fn if_equal_escape(
        self,
        value: i32,
        function: &mut Function,
        escape: impl FnOnce(&mut Function),
    ) {
        function.if_i32_equal_escape(self.index, value, escape);
    }

    pub(crate) fn set_constant(self, value: i32, function: &mut Function) {
        function.store_constant(self.index, LocalConstant::I32(value));
    }
}

impl I64Local {
    pub(crate) fn set_constant(self, value: i64, function: &mut Function) {
        function.store_constant(self.index, LocalConstant::I64(value));
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
    /// Ends this holder: the local returns to the pool for its next holder
    /// of the same type, without a store. A released local is never read
    /// again by this holder, and a GC root is an SSA value live across a
    /// safepoint (Cranelift's stack maps), so a store of null here retained
    /// nothing less. A next holder writes the local before reading it, which
    /// the code sink checks in test and debug builds (`gc_local_writes`).
    pub(crate) fn clear(self, function: &mut Function) {
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
    /// Stores null, unless the local already holds it on every path.
    pub(crate) fn clear(&self, function: &mut Function) {
        function.store_constant(
            self.index,
            LocalConstant::Null(HeapType::Abstract {
                shared: false,
                ty: wasm_encoder::AbstractHeapType::Eq,
            }),
        );
    }
    /// The reference as the sealed layout `T`, after branching to `other`
    /// for every other reference, null included.
    pub(crate) fn sealed_reference_or_branch<T: JavaScriptReference>(
        &self,
        schema: &RuntimeSchema,
        other: crate::LabelDepth,
        function: &mut Function,
    ) -> GcStackReference<T> {
        sealed_cast_or_branch::<T>(
            Instruction::LocalGet(self.index),
            RefType::EQREF,
            schema,
            other,
            function,
        )
    }
    /// Clears a parameter-owned local and returns its slot.
    pub(crate) fn release(self, function: &mut Function) {
        self.clear(function);
        function.release_typed_local(self.index);
    }
}

/// An initialized rooted local. Its index cannot be used as a scalar operand.
#[derive(Debug)]
pub(crate) struct GcLocal<T: GcHeapType, N: GcFieldNullability = NonNullable> {
    index: u32,
    owner: LocalOwner,
    shape: PhantomData<fn() -> (T, N)>,
}

/// Which Wasm local a [`GcLocal`] holds, and so what ending it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalOwner {
    /// A local reserved from the body's pool, with this declaration; the
    /// holder's end returns it to the pool.
    Pooled(LocalDeclaration),
    /// The function's own parameter, declared with exactly the holder's
    /// type (`(ref T)` or `(ref null T)`), aliased rather than copied: the
    /// sink binds each parameter to at most one holder
    /// (`Function::bind_parameter`). A parameter holder is never written
    /// (`replace` and `set_null` refuse it), so every raw read of the
    /// parameter sees the argument, and it is never released.
    Parameter,
}

/// The Wasm type a typed GC local is declared with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalDeclaration {
    /// `(ref null T)`: pooled with every other holder of `T`; a read that
    /// reaches a new block is a new value, which Wasmtime checks for null
    /// again at each access.
    Nullable,
    /// `(ref T)`: validation proves every read non-null, so Wasmtime emits
    /// no null check for it. Validation also confines each read to the
    /// block that wrote the local, or to a block nested in it, but it
    /// accepts a write by any earlier holder of the pooled local; that this
    /// holder wrote it is the code sink's GC local write check
    /// (`code_sink/gc_local_writes.rs`), as for nullable locals.
    NonNull,
}

impl<T: GcHeapType, N: GcFieldNullability> GcLocal<T, N> {
    pub(crate) fn load(
        &self,
        _schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<T, N> {
        function.instruction(&Instruction::LocalGet(self.index));
        // The local declaration already proves its concrete heap type.
        // Logical non-null ownership only needs a null refinement here,
        // which a non-null declaration or parameter already proves. The
        // sink drops it before a consumer that accepts null (see
        // `Function::refine_non_null`): the holder is non-null by
        // construction, so only a compiler bug could make it null.
        if !N::NULLABLE && self.owner == LocalOwner::Pooled(LocalDeclaration::Nullable) {
            function.refine_non_null();
        }
        GcStackReference::new()
    }
    pub(crate) fn replace(&self, value: GcStackReference<T, N>, function: &mut Function) {
        let GcStackReference { .. } = value;
        self.require_pooled("replace");
        function.instruction(&Instruction::LocalSet(self.index));
    }
    /// Refuses a write to an aliased parameter: its raw readers (an
    /// adapter forwarding the argument, a tail call) would see the
    /// replacement, which only the holder's own reads may observe.
    fn require_pooled(&self, write: &str) {
        assert!(
            matches!(self.owner, LocalOwner::Pooled(_)),
            "a parameter holder is never written ({write} of parameter {})",
            self.index
        );
    }
    /// Ends this holder: a pooled local returns to the pool for its next
    /// holder of the same type, without a store. A released local is never
    /// read again by this holder, and a GC root is an SSA value live across
    /// a safepoint (Cranelift's stack maps), so a store of null here
    /// retained nothing less. A next holder writes the local before reading
    /// it, which the code sink checks in test and debug builds
    /// (`gc_local_writes`). An aliased parameter has nothing to return: it
    /// keeps its value, which is equally dead to Cranelift once unread.
    pub(crate) fn clear(self, function: &mut Function) {
        match self.owner {
            LocalOwner::Pooled(_) => function.release_typed_local(self.index),
            LocalOwner::Parameter => {}
        }
    }
}
impl<T: GcHeapType, N: GcFieldNullability> GcLocal<T, N> {
    /// Pushes the reference as the operand of an instruction that itself
    /// traps on null (`struct.get`/`set`, `array.get`/`set`/`len`/`copy`).
    /// The `ref.as_non_null` of [`Self::load`] would repeat that check:
    /// Wasmtime emits both, and either one traps a null reference alike.
    fn access(&self, function: &mut Function) {
        function.instruction(&Instruction::LocalGet(self.index));
    }
}
impl<T: GcHeapType> GcLocal<T, Nullable> {
    pub(crate) fn set_null(&self, schema: &RuntimeSchema, function: &mut Function) {
        self.require_pooled("set_null");
        function.store_constant(
            self.index,
            LocalConstant::Null(
                schema
                    .reference_type::<T>(GcNullability::Nullable)
                    .heap_type,
            ),
        );
    }

    /// The reference, after branching to `absent` when it is null.
    pub(crate) fn non_null_or_branch(
        &self,
        absent: crate::LabelDepth,
        function: &mut Function,
    ) -> GcStackReference<T> {
        function.instruction(&Instruction::LocalGet(self.index));
        function.branch_on_null_to_label(absent);
        GcStackReference::new()
    }

    /// The reference, or for null the one `absent` pushes in its place,
    /// without writing this local.
    pub(crate) fn non_null_or_else(
        &self,
        schema: &RuntimeSchema,
        function: &mut Function,
        absent: impl FnOnce(&mut Function) -> GcStackReference<T>,
    ) -> GcStackReference<T> {
        let output = schema.reference_type::<T>(GcNullability::NonNullable);
        function.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(output))));
        function.instruction(&Instruction::LocalGet(self.index));
        function.instruction(&Instruction::BrOnNonNull(0));
        let GcStackReference { .. } = absent(function);
        function.instruction(&Instruction::End);
        GcStackReference::new()
    }
}
impl<T: GcHeapType, N: GcFieldNullability> GcLocal<T, N> {
    /// The reference as the sealed layout `Sub`, after branching to `other`
    /// for null and for every other layout: one inline header test, where a
    /// test followed by a cast would compare the header twice.
    pub(crate) fn sealed_subtype_or_branch<Sub: GcSubtype<T>>(
        &self,
        schema: &RuntimeSchema,
        other: crate::LabelDepth,
        function: &mut Function,
    ) -> GcStackReference<Sub> {
        sealed_cast_or_branch::<Sub>(
            Instruction::LocalGet(self.index),
            schema.reference_type::<T>(GcNullability::Nullable),
            schema,
            other,
            function,
        )
    }

    /// `sealed_subtype_or_branch` for a sealed layout two declared levels
    /// below `T`: the one test against `Sub` also proves `Middle`.
    pub(crate) fn sealed_nested_subtype_or_branch<Middle: GcSubtype<T>, Sub: GcSubtype<Middle>>(
        &self,
        schema: &RuntimeSchema,
        other: crate::LabelDepth,
        function: &mut Function,
    ) -> GcStackReference<Sub> {
        sealed_cast_or_branch::<Sub>(
            Instruction::LocalGet(self.index),
            schema.reference_type::<T>(GcNullability::Nullable),
            schema,
            other,
            function,
        )
    }
}

/// Casts the reference that `load` pushes, of type `from`, to the sealed
/// layout `T`, or branches to `other` without it.
fn sealed_cast_or_branch<T: GcHeapType>(
    load: Instruction<'static>,
    from: RefType,
    schema: &RuntimeSchema,
    other: crate::LabelDepth,
    function: &mut Function,
) -> GcStackReference<T> {
    assert!(
        T::LAYOUT.sealed_descendants().is_empty(),
        "one inline test identifies only a sealed layout"
    );
    let target = schema.reference_type::<T>(GcNullability::NonNullable);
    function.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(target))));
    function.instruction(&load);
    function.instruction(&Instruction::BrOnCast {
        relative_depth: 0,
        from_ref_type: from,
        to_ref_type: target,
    });
    function.instruction(&Instruction::Drop);
    function.branch_to_label(other);
    function.instruction(&Instruction::End);
    GcStackReference::new()
}

/// A block whose result is a non-null `T`: each branch to it carries one,
/// and none falls through its end, which leaves the reference on the stack.
/// It hands a value proved by a cast in one of several places to the one
/// place that uses it, without a nullable local.
#[derive(Debug)]
pub(crate) struct ReferenceResultBlock<T: GcHeapType> {
    label: crate::LabelDepth,
    shape: PhantomData<fn() -> T>,
}
impl<T: GcHeapType> ReferenceResultBlock<T> {
    /// The block `FunctionBuilder::open_reference_block` has just opened
    /// with the result type `(ref T)` at `label`.
    pub(crate) fn opened(label: crate::LabelDepth) -> Self {
        Self {
            label,
            shape: PhantomData,
        }
    }

    /// Branches to the block's end with `value` as its result.
    pub(crate) fn branch_with(&self, value: GcStackReference<T>, function: &mut Function) {
        let GcStackReference { .. } = value;
        function.branch_to_label(self.label);
    }

    /// Ends the block, whose result is then on the stack. The caller pops
    /// the builder's frame for it.
    pub(crate) fn close(self, function: &mut Function) -> GcStackReference<T> {
        function.instruction(&Instruction::End);
        GcStackReference::new()
    }
}

/// A declaration is consumed when a reference is initialized. Non-null locals
/// cannot be read while their Wasm default value is still null.
#[derive(Debug)]
pub(crate) struct GcLocalSlot<T: GcHeapType, N: GcFieldNullability = NonNullable> {
    index: u32,
    declaration: LocalDeclaration,
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
            owner: LocalOwner::Pooled(self.declaration),
            shape: PhantomData,
        }
    }
}
impl<T: GcHeapType, N: GcFieldNullability> GcLocalSlot<T, N> {
    /// Writes null into a slot that only one path initializes, so that every
    /// path to a read the program reaches only after that initialization
    /// (one a flag local guards) has written it. A pooled local is not
    /// cleared on release; without this such a read would be structurally
    /// reachable with a previous holder's reference. The slot is still
    /// initialized as before; it must be a nullable declaration.
    pub(crate) fn hold_null_until_initialized(
        &self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        assert_eq!(
            self.declaration,
            LocalDeclaration::Nullable,
            "a (ref T) local cannot hold null"
        );
        function.store_constant(
            self.index,
            LocalConstant::Null(
                schema
                    .reference_type::<T>(GcNullability::Nullable)
                    .heap_type,
            ),
        );
    }
}
impl<T: GcHeapType> GcLocalSlot<T, Nullable> {
    pub(crate) fn initialize_null(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcLocal<T, Nullable> {
        function.store_constant(
            self.index,
            LocalConstant::Null(
                schema
                    .reference_type::<T>(GcNullability::Nullable)
                    .heap_type,
            ),
        );
        GcLocal {
            index: self.index,
            owner: LocalOwner::Pooled(self.declaration),
            shape: PhantomData,
        }
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
                emit_i32_in(
                    input,
                    Self::ALL
                        .iter()
                        .filter(|code| code.protocol().is_constructable())
                        .map(|code| code.encoding()),
                    function,
                );
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
enum_i32_codec!(crate::heap::TemporalInitializationState; Dormant => 0, Building => 1, Ready => 2);
enum_i32_codec!(crate::heap::IntlInitializationState; Dormant => 0, Building => 1, Ready => 2);
enum_i32_codec!(crate::heap::ErrorInitializationState; Dormant => 0, Building => 1, Ready => 2);
enum_i32_codec!(crate::heap::DateInitializationState; Dormant => 0, Building => 1, Ready => 2);
enum_i32_codec!(crate::heap::RegExpInitializationState; Dormant => 0, Building => 1, Ready => 2);
enum_i32_codec!(crate::heap::ErrorIntrinsicConstructor; Error => 0, EvalError => 1, AggregateError => 2, SuppressedError => 3, RangeError => 4, SyntaxError => 5, TypeError => 6, URIError => 7, ReferenceError => 8, DOMException => 9);
impl GcI32Constant for crate::heap::DemandGlobalRecipe {
    fn encode(self) -> i32 {
        self.word()
    }
}
enum_i32_codec!(crate::heap::BinaryDataInitializationState; Dormant => 0, Building => 1, Ready => 2);
enum_i32_codec!(crate::heap::BinaryDataConstructor; ArrayBuffer => 0, SharedArrayBuffer => 1, DataView => 2, Float64Array => 3, Float32Array => 4, Float16Array => 5, Int32Array => 6, Int16Array => 7, Int8Array => 8, Uint32Array => 9, Uint16Array => 10, Uint8Array => 11, Uint8ClampedArray => 12, BigInt64Array => 13, BigUint64Array => 14);
enum_i32_codec!(crate::heap::PromiseReactionType; Fulfill => 1, Reject => 2);
enum_i32_codec!(crate::heap::PromiseReactionCallbackKind; Default => 0, AsyncFunction => 1,
    AsyncGeneratorAwaitReturn => 2, AsyncGeneratorAwait => 3, AsyncGeneratorYield => 4,
    AsyncGeneratorYieldReturn => 5, ModuleBody => 6, ModuleJoin => 7, AsyncFromSyncIterator => 8);
enum_i32_codec!(crate::heap::PromiseJobKind; Reaction => 1, ResolveThenable => 2, FinalizationCleanup => 3, Callback => 4);
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
enum_i32_codec!(HostResourceKind; SharedBuffer => 0, WeakRef => 3, FinalizationCell => 4);
enum_i32_codec!(BufferOwnerKind; ArrayBuffer => 0, SharedArrayBuffer => 1);
enum_i32_codec!(IteratorBufferMode; Chunks => 0, WindowsOnlyFull => 1, WindowsAllowPartial => 2);
enum_i32_codec!(IteratorZipMode; Shortest => 0, Longest => 1, Strict => 2);
enum_i32_codec!(ArrayFromAsyncStage; InputValue => 0, MappedValue => 1, AsyncIteratorResult => 2,
    AsyncCloseResult => 4);
enum_i32_codec!(ArrayFromAsyncSourceMode; ArrayLike => 0, AsyncIterator => 1, SyncIterator => 2);
enum_i32_codec!(ReadableStreamState; Readable => 0, Closed => 1, Errored => 2);
enum_i32_codec!(BuiltinClosureCaptureKind; ReadableStreamController => 17, IntlCollator => 0, IntlNumberFormat => 1,
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
    pub(crate) fn replace(&self, value: FunctionStackReference<R, N>, function: &mut Function) {
        let FunctionStackReference { .. } = value;
        function.instruction(&Instruction::LocalSet(self.index));
    }
    /// Ends this holder: the local returns to the pool for its next holder
    /// of the same type, without a store. A released local is never read
    /// again by this holder, and a GC root is an SSA value live across a
    /// safepoint (Cranelift's stack maps), so a store of null here retained
    /// nothing less. A next holder writes the local before reading it, which
    /// the code sink checks in test and debug builds (`gc_local_writes`).
    pub(crate) fn clear(self, function: &mut Function) {
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
impl<R: GcFunctionRole> FunctionLocalSlot<R, Nullable> {
    pub(crate) fn initialize_null(self, function: &mut Function) -> FunctionLocal<R, Nullable> {
        function.store_constant(
            self.index,
            LocalConstant::Null(HeapType::Concrete(R::SIGNATURE.type_index())),
        );
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
        binary_numeric: Option<&FunctionLocal<BinaryNumericCallback>>,
        local_arguments: Option<&FunctionLocal<LocalArgumentsCallable>>,
        wide_local_arguments: Option<&FunctionLocal<WideLocalArgumentsCallable>>,
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
        let local_arguments_valid =
            match (local_arguments.is_some(), wide_local_arguments.is_some()) {
                (false, false) => true,
                (true, true) => false,
                (narrow, _) => {
                    entry_execution == scriptw_ir::FunctionExecutionKind::Ordinary
                        && metadata.class_element_execution
                            == scriptw_ir::ClassElementExecutionKind::None
                        && match metadata.kind {
                            ExecutableCodeKind::JavaScript => matches!(
                                metadata.protocol,
                                scriptw_ir::FunctionProtocolIr::OrdinaryCallAndConstruct
                                    | scriptw_ir::FunctionProtocolIr::Arrow
                            ),
                            ExecutableCodeKind::StandardBuiltin => {
                                crate::function_entry::NativeLocalArgumentsBuiltin::from_builtin_option(
                                    metadata.standard_builtin,
                                )
                                .is_some_and(|native| {
                                    (native.arity()
                                        == crate::function_entry::LocalArgumentsArity::Narrow)
                                        == narrow
                                })
                                    && metadata.protocol
                                        == scriptw_ir::FunctionProtocolIr::OrdinaryCallOnly
                            }
                            ExecutableCodeKind::HostBuiltin => false,
                        }
                }
            };
        if metadata.protocol.execution_kind() != entry_execution
            || !origin_valid
            || (binary_numeric.is_some()
                && (metadata.kind != ExecutableCodeKind::JavaScript
                    || entry_execution != scriptw_ir::FunctionExecutionKind::Ordinary))
            || !local_arguments_valid
        {
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
                match binary_numeric {
                    Some(entry) => GcOperand::function_reference(entry),
                    None => GcOperand::null_function(),
                },
                match local_arguments {
                    Some(entry) => GcOperand::function_reference(entry),
                    None => GcOperand::null_function(),
                },
                match wide_local_arguments {
                    Some(entry) => GcOperand::function_reference(entry),
                    None => GcOperand::null_function(),
                },
            ),
            function,
        ))
    }
}

pub(crate) enum BuiltinClosurePayload<'a> {
    ReadableStreamController(&'a GcLocal<ReadableByteStreamControllerObject>),
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
            BuiltinClosurePayload::ReadableStreamController(value) => self.construct(
                (
                    GcOperand::constant(BuiltinClosureCaptureKind::ReadableStreamController),
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
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(value, schema),
                ),
                function,
            ),
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
                    GcOperand::null(schema),
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
    pub(super) fn new() -> Self {
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

    /// Traps unless the reference has `Sub`'s layout. An open `Sub` with few
    /// sealed descendants is reached through them, leaving its own cast as
    /// the trapping remainder.
    pub(crate) fn cast_subtype<Sub: GcSubtype<T>>(
        self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<Sub> {
        let target = schema.reference_type::<Sub>(GcNullability::NonNullable);
        let Some(alternatives) = schema.sealed_test_alternatives::<Sub>() else {
            function.instruction(&Instruction::RefCastNonNull(target.heap_type));
            return GcStackReference::new();
        };
        let source = schema
            .reserve_gc_local::<T, N>(function)
            .initialize(self, function);
        let from_ref_type = schema.reference_type::<T>(GcNullability::Nullable);
        function.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(target))));
        function.instruction(&Instruction::LocalGet(source.index));
        for to_ref_type in alternatives {
            function.instruction(&Instruction::BrOnCast {
                relative_depth: 0,
                from_ref_type,
                to_ref_type,
            });
        }
        function.instruction(&Instruction::RefCastNonNull(target.heap_type));
        function.instruction(&Instruction::End);
        source.clear(function);
        GcStackReference::new()
    }

    /// Traps unless the reference has `Sub`'s layout, two declared levels
    /// below `T`. One cast to `Sub` already proves `Middle`.
    pub(crate) fn cast_nested_subtype<Middle: GcSubtype<T>, Sub: GcSubtype<Middle>>(
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
        let Some(alternatives) = schema.sealed_test_alternatives::<Sub>() else {
            self.load(schema, function);
            function.instruction(&Instruction::RefTestNonNull(
                schema
                    .reference_type::<Sub>(GcNullability::NonNullable)
                    .heap_type,
            ));
            return;
        };
        for (position, alternative) in alternatives.into_iter().enumerate() {
            self.load(schema, function);
            function.instruction(&Instruction::RefTestNonNull(alternative.heap_type));
            if position != 0 {
                function.instruction(&Instruction::I32Or);
            }
        }
    }
}

/// An open layout with more sealed descendants than this keeps its one host
/// subtype query instead of a longer inline chain.
const MAX_SEALED_TEST_ALTERNATIVES: usize = 4;

impl RuntimeSchema {
    /// The sealed reference types that exactly cover an open layout, when
    /// testing each of them inline replaces a host subtype query.
    fn sealed_test_alternatives<T: GcHeapType>(&self) -> Option<Vec<RefType>> {
        let layouts = T::LAYOUT.sealed_descendants();
        (1..=MAX_SEALED_TEST_ALTERNATIVES)
            .contains(&layouts.len())
            .then(|| {
                layouts
                    .into_iter()
                    .map(|layout| {
                        self.types
                            .layouts
                            .reference(layout, GcNullability::NonNullable)
                    })
                    .collect()
            })
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
    /// The reference, after branching to `absent` when it is null; the
    /// branch carries nothing.
    pub(crate) fn non_null_or_branch(
        self,
        absent: crate::LabelDepth,
        function: &mut Function,
    ) -> GcStackReference<T> {
        function.branch_on_null_to_label(absent);
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
        self.tag.set_constant(tag as i32, function);
        self.scalar.set_constant(scalar, function);
        self.reference.clear(function);
    }
    pub(crate) fn copy_from(&self, source: &Self, function: &mut Function) {
        // A value copied onto its own locals is already there.
        if self.tag.index == source.tag.index {
            assert!(
                self.scalar.index == source.scalar.index
                    && self.reference.index == source.reference.index,
                "value locals are reserved together"
            );
            return;
        }
        source.tag.load(function);
        self.tag.store(function);
        source.scalar.load(function);
        self.scalar.store(function);
        source.reference.load(function);
        function.instruction(&Instruction::LocalSet(self.reference.index));
    }
    pub(crate) fn set_boolean(&self, source: I32Local, function: &mut Function) {
        self.tag
            .set_constant(WasmRuntimeValueTag::Boolean as i32, function);
        source.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I64ExtendI32U);
        self.scalar.store(function);
        self.reference.clear(function);
    }
    pub(crate) fn set_number(&self, bits: I64Local, function: &mut Function) {
        self.tag
            .set_constant(WasmRuntimeValueTag::Number as i32, function);
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
    /// The reference as the sealed layout `T`, after branching to `other`
    /// for every other reference, null included. Like `cast_reference`, the
    /// test reads the reference itself, never the value tag.
    pub(crate) fn sealed_reference_or_branch<T: JavaScriptReference>(
        &self,
        schema: &RuntimeSchema,
        other: crate::LabelDepth,
        function: &mut Function,
    ) -> GcStackReference<T> {
        sealed_cast_or_branch::<T>(
            Instruction::LocalGet(self.reference.index),
            RefType::EQREF,
            schema,
            other,
            function,
        )
    }
    /// The reference as the sealed layout `T`, after branching to `other`
    /// for every other reference, null included, with that reference as an
    /// eqref: `other`'s label carries one. One inline test proves and casts.
    pub(crate) fn sealed_reference_or_fail_to<T: JavaScriptReference>(
        &self,
        schema: &RuntimeSchema,
        other: crate::LabelDepth,
        function: &mut Function,
    ) -> GcStackReference<T> {
        assert!(
            T::LAYOUT.sealed_descendants().is_empty(),
            "one inline test identifies only a sealed layout"
        );
        function.instruction(&Instruction::LocalGet(self.reference.index));
        function.branch_on_cast_fail_to_label(
            other,
            RefType::EQREF,
            schema.reference_type::<T>(GcNullability::NonNullable),
        );
        GcStackReference::new()
    }
    pub(crate) fn set_reference<T: JavaScriptReference>(
        &self,
        source: &GcLocal<T>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.tag.set_constant(T::TAG as i32, function);
        self.scalar.set_constant(0, function);
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
        if self.kind.index == source.kind.index && self.target.index == source.target.index {
            return;
        }
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
        self.target.set_constant(0, function);
    }
    pub(crate) fn set_kind(&self, kind: crate::emit::CompletionKind, function: &mut Function) {
        self.kind.set_constant(kind.code() as i32, function);
    }
    pub(crate) fn set_normal(&self, value: &ValueLocals, function: &mut Function) {
        self.value.copy_from(value, function);
        self.set_kind(crate::emit::CompletionKind::Normal, function);
        self.target.set_constant(0, function);
    }
    pub(crate) fn set_throw(&self, value: &ValueLocals, function: &mut Function) {
        // A Throw completion is being made.
        function.rarely_reached();
        self.value.copy_from(value, function);
        self.set_kind(crate::emit::CompletionKind::Throw, function);
        self.target.set_constant(0, function);
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
        storage: &GcLocal<ArrayObject>,
        mode: ArrayIndexedStorageMode,
        function: &mut Function,
    ) {
        storage.load(self, function);
        self.field(ArrayObjectSchema::MODE).emit_get(function);
        function.instruction(&Instruction::I32Const(mode.encode()));
        function.instruction(&Instruction::I32Eq);
    }

    pub(crate) fn array_storage_is_packed(
        &self,
        storage: &GcLocal<ArrayObject>,
        function: &mut Function,
    ) {
        self.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedNumbers, function);
        self.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedValues, function);
        function.instruction(&Instruction::I32Or);
    }

    pub(crate) fn set_array_storage_mode(
        &self,
        storage: &GcLocal<ArrayObject>,
        mode: ArrayIndexedStorageMode,
        function: &mut Function,
    ) {
        self.field(ArrayObjectSchema::MODE).write(
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
    pub(crate) fn load_timer_queue(&self, function: &mut Function) -> GcLocal<TimerQueue> {
        let root = self.load_existing_timer_queue(function);
        root.load(self, function).is_null(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let queue = self.reserve_gc_local(function).initialize(
            self.struct_type::<TimerQueue>().construct(
                (GcOperand::null(self), GcOperand::i64(1), GcOperand::i64(0)),
                function,
            ),
            function,
        );
        queue.load(self, function);
        emit_root_set(function, self.timer_queue);
        root.replace(queue.load(self, function).nullable(), function);
        queue.clear(function);
        function.instruction(&Instruction::End);
        let queue = self.reserve_gc_local(function).initialize(
            root.load(self, function).require_non_null(function),
            function,
        );
        root.clear(function);
        queue
    }
    pub(crate) fn load_existing_timer_queue(
        &self,
        function: &mut Function,
    ) -> GcLocal<TimerQueue, Nullable> {
        let root = self.reserve_gc_local::<TimerQueue, Nullable>(function);
        emit_root_get(function, self.timer_queue);
        root.initialize(GcStackReference::new(), function)
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
    pub(crate) fn load_symbol_registry(
        &self,
        function: &mut Function,
    ) -> GcLocal<RegisteredSymbolTable, Nullable> {
        let slot = self.reserve_gc_local::<RegisteredSymbolTable, Nullable>(function);
        emit_root_get(function, self.registered_symbols);
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
        // Published before any program code runs: null only by a bug.
        function.refine_non_null();
        GcStackReference::new()
    }
    pub(crate) fn pooled_string_reference(
        &self,
        index: crate::data::PooledStringIndex,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        // A published entry is read in place. Only the first use of an
        // ordinal enters the owner, which publishes it.
        let output = self.reference_type::<StringValue>(GcNullability::NonNullable);
        function.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(output))));
        emit_root_get(function, self.pooled_strings);
        // `array.get` traps on a null table itself, and the index push
        // between them has no effect: a `ref.as_non_null` would repeat it.
        function.instruction(&Instruction::I32Const(index.ordinal() as i32));
        function.instruction(&Instruction::ArrayGet(
            self.array_type::<PooledStringTable>().type_index().raw(),
        ));
        // Null only until the ordinal's first use.
        function.likely(&Instruction::BrOnNonNull(0));
        // The ordinal is the miss call's immediate argument.
        let _ = self.resolve_pooled_string_reference(
            GcIndex::Constant(index.ordinal()),
            base,
            function,
        );
        function.instruction(&Instruction::End);
        GcStackReference::new()
    }
    pub(crate) fn resolve_pooled_string_reference(
        &self,
        ordinal: impl Into<GcIndex>,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        self.helper_reference_on_stack(self.call_helper(
            crate::runtime_helpers::PooledStringResolveArguments::new(ordinal),
            base,
            function,
        ))
    }
    /// A fresh Realm usually sees an empty pool slot for each global name.
    /// Publish short names through the same cache and descriptor hash, but
    /// build the known code units directly instead of copying passive data.
    pub(crate) fn try_bootstrap_pooled_name(
        &self,
        table: &GcLocal<PooledStringTable>,
        pool: &crate::data::StringPool,
        name: &str,
        position: u32,
        function: &mut Function,
    ) -> Result<Option<GcStackReference<StringValue>>, crate::EmitError> {
        let index = pool.pooled_string_index(name)?;
        let crate::data::PooledLiteralDomain::Runtime(index) = pool.pooled_string_domain(index)?
        else {
            return Ok(None);
        };
        let units = scriptw_ir::decode_js_string_utf16(name);
        // Bound the emitted cold path independently of source identifier length.
        if units.len() > 24 {
            return Ok(None);
        }
        let descriptors = pool.runtime_string_descriptors()?;
        assert!(index.ordinal() < descriptors.count());
        let slot = GcIndex::Constant(index.ordinal());
        let strings = self.array_type::<PooledStringTable>();
        let output = self.reference_type::<StringValue>(GcNullability::NonNullable);
        function.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(output))));
        let _ = strings.read(table, slot, self, function);
        function.instruction(&Instruction::BrOnNonNull(0));

        let units = self.reserve_gc_local(function).initialize(
            self.array_type::<CodeUnitArray>().fixed(
                units
                    .into_iter()
                    .map(|unit| GcOperand::i32(i32::from(unit))),
                function,
            ),
            function,
        );
        let raw_hash = self.reserve_i64_local(function);
        function.instruction(&Instruction::I32Const(descriptors.address() as i32));
        function.instruction(&Instruction::I64Load(wasm_encoder::MemArg {
            offset: u64::from(index.ordinal())
                * u64::from(crate::data::RuntimeStringDescriptors::ROW_BYTES),
            align: 0,
            memory_index: 0,
        }));
        raw_hash.store(function);
        let name = self.reserve_gc_local(function).initialize(
            self.struct_type::<StringValue>().construct(
                (
                    GcOperand::reference(&units, self),
                    GcOperand::i64_local(raw_hash),
                    // Lookup checks this hint against the live entry and key.
                    GcOperand::i32(position as i32),
                ),
                function,
            ),
            function,
        );
        self.release_i64_local(raw_hash, function);
        units.clear(function);
        strings.write(
            table,
            slot,
            GcOperand::nullable_reference(&name, self),
            self,
            function,
        );
        let result = name.load(self, function);
        name.clear(function);
        function.instruction(&Instruction::End);
        Ok(Some(result))
    }
    pub(crate) fn well_known_symbol_reference(
        &self,
        symbol: scriptw_ir::WellKnownSymbol,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<SymbolValue> {
        self.resolve_well_known_symbol_reference(GcIndex::Constant(symbol as u32), base, function)
    }
    /// The table is published by R before any Realm is initialized.
    pub(crate) fn well_known_symbol_table_reference(
        &self,
        function: &mut Function,
    ) -> GcStackReference<WellKnownSymbolTable, Nullable> {
        emit_root_get(function, self.well_known_symbols);
        GcStackReference::new()
    }
    /// Used after the checked installation key has selected its Symbol arm.
    pub(crate) fn installation_symbol_reference(
        &self,
        key: &crate::runtime_helpers::RuntimeHelperIntrinsicKeyParameter,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<SymbolValue> {
        self.resolve_well_known_symbol_reference(key.ordinal(), base, function)
    }
    /// A sealed value row uses the same Store-wide Symbol identity as keys.
    pub(crate) fn property_layout_symbol_reference(
        &self,
        row: &crate::runtime_helpers::RuntimeHelperPropertyLayoutRow,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<SymbolValue> {
        self.resolve_well_known_symbol_reference(
            row.well_known_symbol_ordinal(function),
            base,
            function,
        )
    }
    /// Only the R helper publishes a missing symbol.
    fn resolve_well_known_symbol_reference(
        &self,
        ordinal: impl Into<GcIndex>,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<SymbolValue> {
        let ordinal = ordinal.into();
        let output = self.reference_type::<SymbolValue>(GcNullability::NonNullable);
        function.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(output))));
        self.array_type::<WellKnownSymbolTable>().read_top(
            self.well_known_symbol_table_reference(function),
            ordinal,
            function,
        );
        function.likely(&Instruction::BrOnNonNull(0));
        let _ = self.helper_reference_on_stack(self.call_helper(
            crate::runtime_helpers::WellKnownSymbolResolveArguments::new(ordinal),
            base,
            function,
        ));
        function.instruction(&Instruction::End);
        GcStackReference::new()
    }

    /// A missing symbol has never been hashed. The instanceof proof can
    /// check this fact without allocating the symbol.
    pub(crate) fn emit_well_known_symbol_never_hashed(
        &self,
        symbol: scriptw_ir::WellKnownSymbol,
        function: &mut Function,
    ) {
        let present = self.reference_type::<SymbolValue>(GcNullability::NonNullable);
        function.instruction(&Instruction::Block(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(present))));
        self.array_type::<WellKnownSymbolTable>().read_top(
            self.well_known_symbol_table_reference(function),
            GcIndex::Constant(symbol as u32),
            function,
        );
        function.likely(&Instruction::BrOnNonNull(0));
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        self.field(SymbolValueSchema::KEY_HASH_ID)
            .read_top(GcStackReference::<SymbolValue>::new(), function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::End);
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
        // Installed before any program code runs: null only by a bug.
        function.refine_non_null();
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
    #[track_caller]
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
    pub(crate) fn reference_binary_numeric_entry(
        &self,
        entry: &crate::function_entry::PlannedBinaryNumericEntry,
        function: &mut Function,
    ) -> FunctionStackReference<BinaryNumericCallback> {
        entry.emit_ref_func(function);
        FunctionStackReference { shape: PhantomData }
    }

    pub(crate) fn reference_local_arguments_entry<R: GcEntryFunctionRole>(
        &self,
        entry: &crate::function_entry::PlannedLocalArgumentsEntry,
        function: &mut Function,
    ) -> FunctionStackReference<R> {
        assert_eq!(
            GcFunctionSignature::Entry(entry.signature()),
            R::SIGNATURE,
            "local argument callable role matches the source-bound signature"
        );
        entry.emit_ref_func(function);
        FunctionStackReference { shape: PhantomData }
    }

    /// This scalar role owns its two-result order; it has no Completion route.
    pub(crate) fn call_binary_numeric_reference(
        &self,
        entry: &FunctionLocal<BinaryNumericCallback>,
        left: &ValueLocals,
        right: &ValueLocals,
        handled: I32Local,
        output: F64Local,
        function: &mut Function,
    ) {
        for value in [left, right] {
            value.scalar().load(function);
            function.instruction(&Instruction::F64ReinterpretI64);
        }
        entry.load(function);
        function.instruction(&Instruction::CallRef(
            BinaryNumericCallback::SIGNATURE.type_index(),
        ));
        output.store(function);
        handled.store(function);
    }

    /// The defining P thunk publishes the proof in its admission result. The
    /// arithmetic result is discarded, so the test operands prove nothing.
    pub(crate) fn emit_binary_numeric_subtraction_admitted(
        &self,
        entry: &FunctionLocal<BinaryNumericCallback>,
        function: &mut Function,
    ) {
        for _ in 0..2 {
            function.instruction(&Instruction::F64Const(0.0.into()));
        }
        entry.load(function);
        function.instruction(&Instruction::CallRef(
            BinaryNumericCallback::SIGNATURE.type_index(),
        ));
        function.instruction(&Instruction::Drop);
        function.instruction(&Instruction::I32Const(
            BinaryNumericCallback::SUBTRACTION_ADMITTED,
        ));
        function.instruction(&Instruction::I32Eq);
    }

    pub(crate) fn call_entry(
        &self,
        entry: &crate::function_entry::PlannedFunctionEntry,
        function: &mut Function,
    ) -> GcCallResult {
        entry.emit_direct_call_instruction(function);
        GcCallResult::emitted()
    }
    pub(crate) fn call_local_arguments_entry(
        &self,
        entry: &crate::function_entry::PlannedLocalArgumentsEntry,
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
    #[track_caller]
    pub(crate) fn reserve_gc_local<T: GcHeapType, N: GcFieldNullability>(
        &self,
        function: &mut Function,
    ) -> GcLocalSlot<T, N> {
        let reference_type = self.reference_type::<T>(GcNullability::Nullable);
        GcLocalSlot {
            index: function.reserve_typed_local(ValType::Ref(reference_type)),
            declaration: LocalDeclaration::Nullable,
            shape: PhantomData,
        }
    }

    /// A local declared `(ref T)`, for a hot holder read in many blocks:
    /// no read of it is checked for null. Wasm validation accepts a read
    /// only inside the block whose `local.set` initialized it (or a block
    /// nested in that one), so the holder must be initialized in the
    /// outermost block that reads it.
    #[track_caller]
    pub(crate) fn reserve_non_null_gc_local<T: GcHeapType>(
        &self,
        function: &mut Function,
    ) -> GcLocalSlot<T> {
        let reference_type = self.reference_type::<T>(GcNullability::NonNullable);
        GcLocalSlot {
            index: function.reserve_typed_local(ValType::Ref(reference_type)),
            declaration: LocalDeclaration::NonNull,
            shape: PhantomData,
        }
    }

    /// Holds `value` in a local initialized here. At the function's top
    /// level the local is declared `(ref T)`: every later read of the
    /// holder is then nested in the block that wrote it, as validation
    /// requires, and Wasmtime checks none of them for null. Elsewhere it is
    /// a pooled `(ref null T)` local whose reads are refined.
    #[track_caller]
    pub(crate) fn hold_gc_local<T: GcHeapType>(
        &self,
        value: GcStackReference<T>,
        function: &mut Function,
    ) -> GcLocal<T> {
        let slot = if function.at_function_level() {
            self.reserve_non_null_gc_local(function)
        } else {
            self.reserve_gc_local(function)
        };
        slot.initialize(value, function)
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
    /// A lone reference operand, such as a value's reference word passed
    /// without its tag and scalar.
    pub(crate) fn parameter_eqref(&self, index: u32, function: &mut Function) -> EqRefLocal {
        assert_eq!(
            function.parameter_type(index),
            Some(ValType::Ref(RefType::EQREF)),
            "reference parameter mismatch"
        );
        let local = EqRefLocal {
            index: function.reserve_typed_local(ValType::Ref(RefType::EQREF)),
        };
        function.instruction(&Instruction::LocalGet(index));
        function.instruction(&Instruction::LocalSet(local.index));
        local
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
        // The parameter is declared with exactly the holder's type: it is
        // the holder, with no copy into a pooled local.
        function.bind_parameter(index);
        GcLocal {
            index,
            owner: LocalOwner::Parameter,
            shape: PhantomData,
        }
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
    ProgramLiteralIndex(ProgramLiteralIndex),
    I64(i64),
    F64(f64),
    Local(u32),
    NonNullLocal(u32),
    BooleanLocal(u32),
    Null(HeapType),
    DataDescriptor {
        writable: DescriptorWordFlag,
        enumerable: DescriptorWordFlag,
        configurable: DescriptorWordFlag,
    },
    AccessorDescriptor {
        enumerable: DescriptorWordFlag,
        configurable: DescriptorWordFlag,
    },
}
/// One attribute of a descriptor word: known at emission, or an i32 local
/// whose nonzero value sets it. Known attributes fold into the word's
/// immediate; only local ones are tested at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DescriptorWordFlag {
    Known(bool),
    Local(I32Local),
}
impl From<I32Local> for DescriptorWordFlag {
    fn from(local: I32Local) -> Self {
        Self::Local(local)
    }
}
impl From<bool> for DescriptorWordFlag {
    fn from(value: bool) -> Self {
        Self::Known(value)
    }
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
            OperandSource::ProgramLiteralIndex(value) => {
                function.program_literal_index(value);
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
                let base = StoredPropertyAttributes::Data {
                    writable: false,
                    enumerable: false,
                    configurable: false,
                }
                .descriptor_word();
                emit_descriptor_word(
                    base,
                    [
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
                    ],
                    function,
                );
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
                emit_descriptor_word(
                    base,
                    [
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
                    ],
                    function,
                );
                return;
            }
        };
        function.instruction(&instruction);
    }
}
/// Pushes `base` with each set attribute's bit. Known attributes fold into
/// one immediate; each local one adds `(local != 0) * bit`. A zero immediate
/// is pushed only when no local attribute remains to be joined.
fn emit_descriptor_word<const N: usize>(
    base: crate::heap::DescriptorWord,
    flags: [(DescriptorWordFlag, crate::heap::DescriptorWord); N],
    function: &mut Function,
) {
    let known = flags
        .iter()
        .filter(|(flag, _)| *flag == DescriptorWordFlag::Known(true))
        .fold(base.as_i32(), |word, (_, bit)| word | bit.as_i32());
    let locals = flags.iter().filter_map(|(flag, bit)| match flag {
        DescriptorWordFlag::Local(local) => Some((*local, *bit)),
        DescriptorWordFlag::Known(_) => None,
    });
    let mut pushed = false;
    if known != 0
        || flags
            .iter()
            .all(|(flag, _)| matches!(flag, DescriptorWordFlag::Known(_)))
    {
        function.instruction(&Instruction::I32Const(known));
        pushed = true;
    }
    for (local, bit) in locals {
        local.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Const(bit.as_i32()));
        function.instruction(&Instruction::I32Mul);
        if pushed {
            function.instruction(&Instruction::I32Or);
        }
        pushed = true;
    }
}
pub(crate) trait GcI32Field: sealed::Sealed {}
pub(crate) trait GcNumericI32Field: GcI32Field {}
impl GcNumericI32Field for I32Value {}
impl GcNumericI32Field for I8Value {}
impl GcNumericI32Field for I16Value {}
pub(crate) trait GcI32Constant: GcI32Field {
    fn encode(self) -> i32;
}
impl GcOperand<'_, scriptw_ir::StandardBuiltinId, NonNullable> {
    /// The helper ABI accepts a closed builtin constant, rather than an I32.
    pub(crate) fn helper_standard_builtin(
        parameter: &crate::runtime_helpers::RuntimeHelperStandardBuiltinParameter,
    ) -> Self {
        Self {
            source: OperandSource::Local(parameter.local().index),
            shape: PhantomData,
        }
    }
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
        writable: impl Into<DescriptorWordFlag>,
        enumerable: impl Into<DescriptorWordFlag>,
        configurable: impl Into<DescriptorWordFlag>,
    ) -> Self {
        Self {
            source: OperandSource::DataDescriptor {
                writable: writable.into(),
                enumerable: enumerable.into(),
                configurable: configurable.into(),
            },
            shape: PhantomData,
        }
    }
    pub(crate) fn accessor_descriptor_flags(
        enumerable: impl Into<DescriptorWordFlag>,
        configurable: impl Into<DescriptorWordFlag>,
    ) -> Self {
        Self {
            source: OperandSource::AccessorDescriptor {
                enumerable: enumerable.into(),
                configurable: configurable.into(),
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

    pub(crate) fn intrinsic_layout_flags(
        row: &crate::runtime_helpers::RuntimeHelperPropertyLayoutRow,
    ) -> Self {
        Self {
            source: OperandSource::Local(row.flags().index),
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
    /// A P literal ordinal stored as data, relocated like every literal site.
    pub(crate) fn program_literal_index(value: ProgramLiteralIndex) -> Self {
        Self {
            source: OperandSource::ProgramLiteralIndex(value),
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
                value.non_null_source()
            },
            shape: PhantomData,
        }
    }
}
impl<T: GcHeapType, N: GcFieldNullability> GcLocal<T, N> {
    /// A non-null operand read from this logically non-null local: a
    /// nullable declaration refines the read, which a non-null declaration
    /// already proves.
    fn non_null_source(&self) -> OperandSource {
        match self.owner {
            LocalOwner::Pooled(LocalDeclaration::Nullable) => {
                OperandSource::NonNullLocal(self.index)
            }
            LocalOwner::Pooled(LocalDeclaration::NonNull) | LocalOwner::Parameter => {
                OperandSource::Local(self.index)
            }
        }
    }
}
impl<'a, Base: GcHeapType> GcOperand<'a, GcRef<Base>, NonNullable> {
    pub(crate) fn subtype_reference<Sub: GcSubtype<Base>>(
        value: &'a GcLocal<Sub>,
        _schema: &RuntimeSchema,
    ) -> Self {
        Self {
            // A GC local is physically nullable unless declared `(ref T)`.
            // A nominal upcast preserves the logical non-null requirement.
            source: value.non_null_source(),
            shape: PhantomData,
        }
    }
}
impl<'a, T: GcHeapType> GcOperand<'a, GcRef<T>, Nullable> {
    pub(crate) fn nullable_reference(value: &'a GcLocal<T>, _schema: &RuntimeSchema) -> Self {
        Self {
            source: value.non_null_source(),
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
    pub(crate) fn null_eq() -> Self {
        Self {
            source: OperandSource::Null(RefType::EQREF.heap_type),
            shape: PhantomData,
        }
    }

    pub(crate) fn eq_reference(value: &'a EqRefLocal) -> Self {
        Self {
            source: OperandSource::Local(value.index),
            shape: PhantomData,
        }
    }

    pub(crate) fn gc_reference<T: GcHeapType>(value: &'a GcLocal<T>, _schema: &RuntimeSchema) -> Self {
        Self {
            source: value.non_null_source(),
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
    pub(crate) fn read_into<N: GcFieldNullability>(
        &self,
        stored: &GcLocal<StoredValue, N>,
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

    pub(crate) fn from_string(
        self,
        text: &GcLocal<StringValue>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<StoredValue> {
        self.construct(
            (
                GcOperand::i64(0),
                GcOperand::i32(WasmRuntimeValueTag::String.tag()),
                GcOperand::gc_reference(text, schema),
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
        _schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcFieldRead<V, N> {
        owner.access(function);
        self.emit_get(function);
        GcFieldRead { shape: PhantomData }
    }
    /// Reads the field of the owner reference on top of the stack, which
    /// no local needs to hold.
    pub(crate) fn read_top<R: GcFieldNullability>(
        &self,
        owner: GcStackReference<O, R>,
        function: &mut Function,
    ) -> GcFieldRead<V, N> {
        let GcStackReference { .. } = owner;
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
        _schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        owner.access(function);
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
        _schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        owner.access(function);
        function.instruction(&Instruction::ArrayLen);
    }
    /// The length of the array reference on top of the stack, which no
    /// local needs to hold.
    pub(crate) fn length_top<R: GcFieldNullability>(
        &self,
        owner: GcStackReference<T, R>,
        function: &mut Function,
    ) {
        let GcStackReference { .. } = owner;
        function.instruction(&Instruction::ArrayLen);
    }
    pub(crate) fn read<R: GcFieldNullability>(
        &self,
        owner: &GcLocal<T, R>,
        index: impl Into<GcIndex>,
        _schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcFieldRead<V, N> {
        owner.access(function);
        index.into().emit(function);
        self.emit_get(function);
        GcFieldRead { shape: PhantomData }
    }
    /// Reads `owner[index]` of the array reference on top of the stack.
    pub(crate) fn read_top<R: GcFieldNullability>(
        &self,
        owner: GcStackReference<T, R>,
        index: impl Into<GcIndex>,
        function: &mut Function,
    ) -> GcFieldRead<V, N> {
        let GcStackReference { .. } = owner;
        index.into().emit(function);
        self.emit_get(function);
        GcFieldRead { shape: PhantomData }
    }
}

/// An array index or i32 helper argument. A compile-time value is pushed as
/// its immediate instead of through a local only this operand reads. A P
/// literal ordinal is relocated when the program is linked: it is pushed
/// through the sink's relocation record and is never a constant, so no fact
/// or fold may observe its unrelocated value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GcIndex {
    Local(I32Local),
    Constant(u32),
    Literal(ProgramLiteralIndex),
}
impl GcIndex {
    pub(crate) fn emit(self, function: &mut Function) {
        match self {
            Self::Local(local) => local.load(function),
            // Wasm indexes are unsigned: the bits are the u32 value.
            Self::Constant(value) => {
                function.instruction(&Instruction::I32Const(value as i32));
            }
            Self::Literal(index) => function.program_literal_index(index),
        }
    }
}
impl From<I32Local> for GcIndex {
    fn from(local: I32Local) -> Self {
        Self::Local(local)
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
        index: impl Into<GcIndex>,
        value: GcOperand<'_, V, N>,
        _schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        owner.access(function);
        index.into().emit(function);
        value.emit(function);
        self.emit_set(function);
    }
    /// Writes `owner[index]` of the array reference on top of the stack.
    pub(crate) fn write_top<R: GcFieldNullability>(
        &self,
        owner: GcStackReference<T, R>,
        index: impl Into<GcIndex>,
        value: GcOperand<'_, V, N>,
        function: &mut Function,
    ) {
        let GcStackReference { .. } = owner;
        index.into().emit(function);
        value.emit(function);
        self.emit_set(function);
    }

    /// Copies `length` elements from `source[source_index..]` to
    /// `destination[destination_index..]`; the engine checks both ranges.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn copy_range_from<R: GcFieldNullability>(
        &self,
        destination: &GcLocal<T>,
        destination_index: I32Local,
        source: &GcLocal<T, R>,
        source_index: I32Local,
        length: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        destination.load(schema, function);
        destination_index.load(function);
        source.load(schema, function);
        source_index.load(function);
        length.load(function);
        function.instruction(&Instruction::ArrayCopy {
            array_type_index_dst: self.type_index().raw(),
            array_type_index_src: self.type_index().raw(),
        });
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

/// The longest String the implementation creates, derived from the heap a
/// product Store has. ECMA-262 allows 2^53 - 1 code units and V8 2^29 - 24,
/// but a Store's GC heap is capped at 1 GiB (`WASM_STORE_MEMORY_CAP_BYTES`,
/// the pooled heap size and the embedding's `StoreLimits`) and its copying
/// collector keeps half of it as the idle semi-space, so at most 512 MiB is
/// live. A String is a flat `CodeUnitArray` of two bytes per unit, and
/// building one by concatenation keeps its operands (as many units again)
/// live beside it: four bytes per result unit. Measured on the product CLI
/// (2026-11-20), two distinct halves concatenate into at most 129.8 million
/// units, 495 MiB live, so the runtime's own live data is about 17 MiB; with
/// 24 MiB reserved the bound is (512 - 24) MiB / 4 = 2^27 - 6 * 2^20 code
/// units. Past it an operation throws a catchable RangeError before
/// allocating, where the allocation itself would trap; V8's length cannot be
/// allocated in this heap at all. A String within the limit can still
/// exhaust a heap that other live values fill (or that earlier growth
/// fragmented), which traps as a Store resource limit. Builders that keep
/// more than the two operands live, such as `join` beside its element
/// Strings (about six bytes per unit), can trap below the limit for the same
/// reason.
///
/// The 1 GiB premise holds for the CLI, pooled Engines and an embedder that
/// attaches the Store's own limiter (`limits_mut`, holding the cap). An
/// embedder chooses its limiter: with a smaller cap a String within the limit
/// traps instead of throwing; with none, an on-demand Store's heap is bounded
/// only by its 4 GiB reservation and the limit is merely conservative.
pub(crate) const MAX_STRING_LENGTH: i64 = STRING_LIVE_HEAP_BYTES / 4;
/// The live semi-space of the 1 GiB Store heap, less the runtime's reserve.
const STRING_LIVE_HEAP_BYTES: i64 = (512 << 20) - (24 << 20);

/// Code-unit copies at or below this length run element by element. A bulk
/// `array.copy` of a dynamic length is a host `memmove` call whose fixed
/// transition cost exceeds a short inline loop.
const INLINE_CODE_UNIT_COPY_LIMIT: i32 = 8;
/// The same limit for the parts of a concatenation. One unit of the inline
/// loop runs 22 or 23 native instructions (two null checks, two bounds
/// checks and the reloads between them), and a bulk copy about 85 whatever
/// its length, so four units already cost more inline.
const INLINE_CONCATENATION_COPY_LIMIT: i32 = 3;
/// Backings at or below this length are zero-filled inline. A constant zero
/// fill operand makes the engine call its host `memset` for any length.
const INLINE_CODE_UNIT_FILL_LIMIT: i32 = 16;
/// The same inline zero fill for short array-index key tables.
const INLINE_INDEX_KEY_FILL_LIMIT: i32 = 32;

/// Leaves a zero-filled CodeUnitArray of `length` units on the stack.
fn emit_code_unit_array_new(length: I32Local, schema: &RuntimeSchema, function: &mut Function) {
    let array = schema.array_type::<CodeUnitArray>().type_index().raw();
    let output = schema.reference_type::<CodeUnitArray>(GcNullability::NonNullable);
    length.load(function);
    function.instruction(&Instruction::I32Const(INLINE_CODE_UNIT_FILL_LIMIT));
    function.instruction(&Instruction::I32LeU);
    function.instruction(&Instruction::If(BlockType::Result(ValType::Ref(output))));
    // Zero for every length in this arm, but not an engine constant.
    length.load(function);
    function.instruction(&Instruction::I32Const(5));
    function.instruction(&Instruction::I32ShrU);
    length.load(function);
    function.instruction(&Instruction::ArrayNew(array));
    function.instruction(&Instruction::Else);
    function.instruction(&Instruction::I32Const(0));
    length.load(function);
    function.instruction(&Instruction::ArrayNew(array));
    function.instruction(&Instruction::End);
}

/// Copies `length` units from `source[source_index..]` (index zero when
/// absent) to `destination[destination_index..]`. Both ranges are checked by
/// the engine's array bounds checks.
fn emit_bulk_code_unit_copy(
    destination: &GcLocal<CodeUnitArray>,
    destination_index: I32Local,
    source: &GcLocal<CodeUnitArray>,
    source_index: I32Local,
    length: I32Local,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    let type_index = schema.array_type::<CodeUnitArray>().type_index().raw();
    destination.load(schema, function);
    destination_index.load(function);
    source.load(schema, function);
    source_index.load(function);
    length.load(function);
    function.instruction(&Instruction::ArrayCopy {
        array_type_index_dst: type_index,
        array_type_index_src: type_index,
    });
}

fn emit_code_unit_copy(
    destination: &GcLocal<CodeUnitArray>,
    destination_index: Option<I32Local>,
    source: &GcLocal<CodeUnitArray>,
    source_index: Option<I32Local>,
    length: I32Local,
    inline_limit: i32,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    let array = schema.array_type::<CodeUnitArray>();
    let start = |index: Option<I32Local>, function: &mut Function| match index {
        Some(index) => index.load(function),
        None => {
            function.instruction(&Instruction::I32Const(0));
        }
    };
    length.load(function);
    function.instruction(&Instruction::I32Const(inline_limit));
    function.instruction(&Instruction::I32LeU);
    function.instruction(&Instruction::If(BlockType::Empty));
    let offset = schema.reserve_i32_local(function);
    function.instruction(&Instruction::I32Const(0));
    offset.store(function);
    function.instruction(&Instruction::Block(BlockType::Empty));
    function.instruction(&Instruction::Loop(BlockType::Empty));
    offset.load(function);
    length.load(function);
    function.instruction(&Instruction::I32GeU);
    function.instruction(&Instruction::BrIf(1));
    destination.load(schema, function);
    start(destination_index, function);
    offset.load(function);
    function.instruction(&Instruction::I32Add);
    source.load(schema, function);
    start(source_index, function);
    offset.load(function);
    function.instruction(&Instruction::I32Add);
    array.emit_get(function);
    array.emit_set(function);
    offset.load(function);
    function.instruction(&Instruction::I32Const(1));
    function.instruction(&Instruction::I32Add);
    offset.store(function);
    function.instruction(&Instruction::Br(0));
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::End);
    schema.release_i32_local(offset, function);
    function.instruction(&Instruction::Else);
    destination.load(schema, function);
    start(destination_index, function);
    source.load(schema, function);
    start(source_index, function);
    length.load(function);
    let index = array.type_index().raw();
    function.instruction(&Instruction::ArrayCopy {
        array_type_index_dst: index,
        array_type_index_src: index,
    });
    function.instruction(&Instruction::End);
}

impl GcStructType<StringValue> {
    /// Centralize physical field order for both fresh and prehashed strings.
    fn construct(
        self,
        (code_units, raw_hash, entry_hint): (
            GcOperand<'_, GcRef<CodeUnitArray>, NonNullable>,
            GcOperand<'_, I64Value, NonNullable>,
            GcOperand<'_, I32Value, NonNullable>,
        ),
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        self.construct_hash_first((raw_hash, code_units, entry_hint), function)
    }
}

pub(crate) struct StringConstruction {
    units: GcLocal<CodeUnitArray>,
}
impl StringConstruction {
    /// Copy the pool's checked literal span into a fresh private GC backing.
    pub(crate) fn from_pooled_code_units(
        schema: &RuntimeSchema,
        slot: GcLocalSlot<CodeUnitArray>,
        units: &crate::data::PooledCodeUnits,
        function: &mut Function,
    ) -> Self {
        units.emit_bounds(function);
        function.instruction(&Instruction::ArrayNewData {
            array_type_index: schema.array_type::<CodeUnitArray>().type_index().raw(),
            array_data_index: crate::data::PooledCodeUnits::DATA_SEGMENT.index(),
        });
        Self {
            units: slot.initialize(GcStackReference::new(), function),
        }
    }

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

    /// Construct two exact UTF-16 units without a fill followed by a copy.
    pub(crate) fn from_code_unit_pair(
        schema: &RuntimeSchema,
        slot: GcLocalSlot<CodeUnitArray>,
        first: I32Local,
        second: I32Local,
        function: &mut Function,
    ) -> Self {
        Self {
            units: slot.initialize(
                schema.array_type::<CodeUnitArray>().fixed(
                    [GcOperand::i32_local(first), GcOperand::i32_local(second)],
                    function,
                ),
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
        emit_code_unit_array_new(length, schema, function);
        Self {
            units: slot.initialize(GcStackReference::new(), function),
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
        emit_code_unit_copy(
            &self.units,
            Some(destination_index),
            source,
            None,
            length,
            INLINE_CODE_UNIT_COPY_LIMIT,
            schema,
            function,
        );
    }
    /// Copy one part of a concatenation into the unpublished backing array.
    pub(crate) fn copy_concatenated_units_from(
        &self,
        destination_index: I32Local,
        source: &GcLocal<CodeUnitArray>,
        length: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        length.load(function);
        function.instruction(&Instruction::I32Const(4));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.units.load(schema, function);
        destination_index.load(function);
        source.load(schema, function);
        function.instruction(&Instruction::I32Const(0));
        // Wasmtime can inline an immediate four-unit copy as an 8-byte copy.
        function.instruction(&Instruction::I32Const(4));
        let array = schema.array_type::<CodeUnitArray>().type_index().raw();
        function.instruction(&Instruction::ArrayCopy {
            array_type_index_dst: array,
            array_type_index_src: array,
        });
        function.instruction(&Instruction::Else);
        emit_code_unit_copy(
            &self.units,
            Some(destination_index),
            source,
            None,
            length,
            INLINE_CONCATENATION_COPY_LIMIT,
            schema,
            function,
        );
        function.instruction(&Instruction::End);
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
        emit_code_unit_copy(
            &self.units,
            Some(destination_index),
            source,
            Some(source_index),
            length,
            INLINE_CODE_UNIT_COPY_LIMIT,
            schema,
            function,
        );
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
            (
                GcOperand::reference(&self.units, schema),
                GcOperand::i64(0),
                GcOperand::i32(-1),
            ),
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

/// Construct the actual Array with its indexed state inline. Registered
/// mutation owners alone publish closed backing modes; logical Array length
/// never determines a backing allocation size.
impl GcStructType<ArrayObject> {
    pub(crate) fn with_empty_index_storage(
        self,
        header: &GcLocal<OrdinaryObject>,
        length: I64Local,
        length_writable: bool,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<ArrayObject> {
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
                GcOperand::reference(header, schema),
                GcOperand::boolean(length_writable),
                GcOperand::i64_local(length),
                GcOperand::i64(0),
                GcOperand::reference(&buckets, schema),
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
        let array = schema.array_type::<ArrayIndexKeyTable>().type_index().raw();
        let output = schema.reference_type::<ArrayIndexKeyTable>(GcNullability::NonNullable);
        length.load(function);
        function.instruction(&Instruction::I32Const(INLINE_INDEX_KEY_FILL_LIMIT));
        function.instruction(&Instruction::I32LeU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::Ref(output))));
        // Zero in this arm, but not an engine constant: fill inline rather
        // than through the host memset call.
        length.load(function);
        function.instruction(&Instruction::I32Const(6));
        function.instruction(&Instruction::I32ShrU);
        function.instruction(&Instruction::I64ExtendI32U);
        length.load(function);
        function.instruction(&Instruction::ArrayNew(array));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        length.load(function);
        function.instruction(&Instruction::ArrayNew(array));
        function.instruction(&Instruction::End);
        let keys = schema
            .reserve_gc_local(function)
            .initialize(GcStackReference::new(), function);
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
    /// The completed empty List. It has no slot, so it needs no placeholder.
    pub(crate) fn empty(
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<PropertyKeyTable> {
        schema
            .array_type::<PropertyKeyTable>()
            .fixed(std::iter::empty(), function)
    }
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
    /// Stores `snapshot`, an immutable StoredValue whose value is exactly
    /// `key`, instead of wrapping the same key in a fresh snapshot.
    pub(crate) fn write_snapshot(
        &self,
        index: I32Local,
        snapshot: &GcLocal<StoredValue>,
        key: &crate::operations::PropertyKeyLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let _ = key;
        self.keys.load(schema, function);
        index.load(function);
        snapshot.load(schema, function);
        schema.array_type::<PropertyKeyTable>().emit_set(function);
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
