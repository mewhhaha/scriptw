//! Interface to compiling GC-related things.

use crate::TRAP_ARRAY_OUT_OF_BOUNDS;
use crate::bounds_checks::BoundsCheck;
use crate::func_environ::{CheckedEntity, Extension, FuncEnvironment};
use crate::translate::{
    Heap, HeapData, MemoryKind, StructFieldsVec, TargetEnvironment, VmctxLoadChain,
};
use crate::trap::TranslateTrap;
use crate::{Reachability, TRAP_GC_HEAP_CORRUPT, TRAP_INTERNAL_ASSERT};
use cranelift_codegen::ir::{BlockArg, ExceptionTableData, ExceptionTableItem};
use cranelift_codegen::{
    cursor::{Cursor as _, FuncCursor},
    dominator_tree::DominatorTree,
    flowgraph::ControlFlowGraph,
    ir::{self, InstBuilder, condcodes::IntCC},
};
use cranelift_entity::packed_option::ReservedValue;
use cranelift_frontend::FunctionBuilder;
use smallvec::{SmallVec, smallvec};
use wasmtime_environ::{
    Collector, GcArrayLayout, GcLayout, GcStructLayout, GcTypeLayouts, I31_DISCRIMINANT,
    ModuleInternedTypeIndex, TagIndex, TypeIndex, VMGcKind, VMSharedTypeIndex,
    WasmCompositeInnerType,
    WasmHeapTopType, WasmHeapType, WasmRefType, WasmResult, WasmStorageType, WasmValType,
    wasm_unsupported,
};

/// A trait for different collectors to emit any GC barriers they might require.
pub trait GcCompiler {
    /// Get the GC type layouts for this GC compiler.
    fn layouts(&self) -> &dyn GcTypeLayouts;

    /// Whether or not this `GcCompiler`'s collector can move objects.
    fn is_moving_collector(&self) -> bool;

    /// Emit code to allocate a new array.
    ///
    /// The array should be of the given type and its elements initialized as
    /// described by the given `ArrayInit`.
    fn alloc_uninit_array(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        array_type_index: TypeIndex,
        len: ir::Value,
    ) -> WasmResult<ir::Value>;

    /// Emit code to allocate a new struct.
    ///
    /// The struct should be of the given type and its fields initialized to the
    /// given values.
    fn alloc_struct(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        struct_type_index: TypeIndex,
        fields: &[ir::Value],
    ) -> WasmResult<ir::Value>;

    /// Emit code to allocate a new exception object.
    ///
    /// The exception object should be of the given type and its
    /// fields initialized to the given values. The tag field is left
    /// uninitialized; that is the responsibility of generated code to
    /// fill in. `tag_index` is used only to look up the appropriate
    /// exception object type.
    fn alloc_exn(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        tag_index: TagIndex,
        fields: &[ir::Value],
        instance_id: ir::Value,
        tag: ir::Value,
    ) -> WasmResult<ir::Value>;

    /// Emit a read barrier for when we are cloning a GC reference onto the Wasm
    /// stack.
    ///
    /// This is used, for example, when reading from a global or a table
    /// element.
    ///
    /// In pseudocode, this is the following operation:
    ///
    /// ```ignore
    /// x = *src;
    /// ```
    ///
    /// Parameters:
    ///
    /// * `func_env`: The function environment that this GC compiler is
    ///   operating within.
    ///
    /// * `builder`: Function builder. Currently at the position where the read
    ///   should be inserted. Upon return, should be positioned where control
    ///   continues just after the read completes. Any intermediate blocks
    ///   created in the process of emitting the read barrier should be added to
    ///   the layout and sealed.
    ///
    /// * `ty`: The Wasm reference type that is being read.
    ///
    /// * `src`: A pointer to the GC reference that should be read; this is an
    ///   instance of a `*mut Option<VMGcRef>`.
    ///
    /// * `flags`: The memory flags that should be used when accessing `src`.
    ///
    /// This method should return the cloned GC reference (an instance of
    /// `VMGcRef`) of type `i32`.
    fn translate_read_gc_reference(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder,
        ty: WasmRefType,
        src: ir::Value,
        flags: ir::MemFlagsData,
    ) -> WasmResult<ir::Value>;

    /// Emit a write barrier for when we are writing a GC reference over another
    /// GC reference.
    ///
    /// This is used, for example, when writing to a global or a table element.
    ///
    /// In pseudocode, this is the following operation:
    ///
    /// ```ignore
    /// *dst = new_val;
    /// ```
    ///
    /// Parameters:
    ///
    /// * `func_env`: The function environment that this GC compiler is
    ///   operating within.
    ///
    /// * `builder`: Function builder. Currently at the position where the write
    ///   should be inserted. Upon return, should be positioned where control
    ///   continues just after the write completes. Any intermediate blocks
    ///   created in the process of emitting the read barrier should be added to
    ///   the layout and sealed.
    ///
    /// * `ty`: The Wasm reference type that is being written.
    ///
    /// * `dst`: A pointer to the GC reference that will be overwritten; note
    ///   that is this is an instance of a `*mut VMGcRef`, *not* a `VMGcRef`
    ///   itself or a `*mut VMGcHeader`!
    ///
    /// * `new_val`: The new value that should be written into `dst`. This is a
    ///   `VMGcRef` of Cranelift type `i32`; not a `*mut VMGcRef`.
    ///
    /// * `flags`: The memory flags that should be used when accessing `dst`.
    fn translate_write_gc_reference(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder,
        ty: WasmRefType,
        dst: ir::Value,
        new_val: ir::Value,
        flags: ir::MemFlagsData,
    ) -> WasmResult<()>;

    /// Same as [`Self::translate_write_gc_reference`] except that the
    /// destination address has not been previously initialized.
    ///
    /// This will, for example, skip barriers on the destination address.
    fn translate_init_gc_reference(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder,
        ty: WasmRefType,
        dst: ir::Value,
        new_val: ir::Value,
        flags: ir::MemFlagsData,
    ) -> WasmResult<()>;

    /// Stores `val` into `addr` with the `ty` specified.
    ///
    /// This initializes a previously-uninitialized field, so barriers on the
    /// value previously stored at `addr`, if necessary, should not be executed.
    fn init_field(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder,
        ty: WasmStorageType,
        addr: ir::Value,
        val: ir::Value,
    ) -> WasmResult<()>;
}

mod base_home;
mod copying;
mod frame_reloads;
mod drc;
mod null;

pub(crate) use base_home::FrameHome;

/// Get the default GC compiler.
pub fn gc_compiler(func_env: &mut FuncEnvironment<'_>) -> WasmResult<Box<dyn GcCompiler>> {
    // If this function requires a GC compiler, that is not too bad of an
    // over-approximation for it requiring a GC heap.
    func_env.needs_gc_heap = true;

    match func_env.tunables.collector {
        Some(Collector::DeferredReferenceCounting) => Ok(Box::new(drc::DrcCompiler::default())),

        Some(Collector::Null) => Ok(Box::new(null::NullCompiler::default())),

        Some(Collector::Copying) => Ok(Box::new(copying::CopyingCompiler::default())),

        None => Err(wasm_unsupported!(
            "support for GC types disabled at configuration time"
        )),
    }
}

fn unbarriered_load_gc_ref(
    builder: &mut FunctionBuilder,
    ty: WasmHeapType,
    ptr_to_gc_ref: ir::Value,
    flags: ir::MemFlagsData,
) -> WasmResult<ir::Value> {
    debug_assert!(ty.is_vmgcref_type());
    let gc_ref = builder.ins().load(ir::types::I32, flags, ptr_to_gc_ref, 0);
    if ty != WasmHeapType::I31 {
        builder.declare_value_needs_stack_map(gc_ref);
    }
    Ok(gc_ref)
}

fn unbarriered_store_gc_ref(
    builder: &mut FunctionBuilder,
    ty: WasmHeapType,
    dst: ir::Value,
    gc_ref: ir::Value,
    flags: ir::MemFlagsData,
) -> WasmResult<()> {
    debug_assert!(ty.is_vmgcref_type());
    builder.ins().store(flags, gc_ref, dst, 0);
    Ok(())
}

/// Emit CLIF to call the `gc_alloc_raw` libcall.
fn emit_gc_raw_alloc(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    kind: VMGcKind,
    ty: ModuleInternedTypeIndex,
    size: ir::Value,
    align: u32,
    reserved_bits: u32,
) -> ir::Value {
    let gc_alloc_raw_builtin = func_env.builtin_functions.gc_alloc_raw(builder.func);
    let vmctx = func_env.vmctx_val(&mut builder.cursor());

    let kind = builder
        .ins()
        .iconst(ir::types::I32, i64::from(kind.as_u32() | reserved_bits));

    let ty = func_env.module_interned_to_shared_ty(&mut builder.cursor(), ty);

    assert!(align.is_power_of_two());
    let align = builder.ins().iconst(ir::types::I32, i64::from(align));

    let call_inst = builder
        .ins()
        .call(gc_alloc_raw_builtin, &[vmctx, kind, ty, size, align]);

    let gc_ref = builder.func.dfg.first_result(call_inst);
    builder.declare_value_needs_stack_map(gc_ref);
    gc_ref
}

/// Null-check `gc_ref` for a GC access to its bytes `..access_end`.
///
/// Nothing is emitted when validation proves the operand non-null
/// (`non_null`: its static type is a non-nullable reference). When the access
/// is the first through `gc_ref` and lies within the heap's inaccessible null
/// guard (`access_end`, see `implicit_null_check`), the access itself is the
/// check: the returned trap code is the one it must carry. Otherwise this
/// emits an explicit check.
///
/// (Scriptw patch) An access through the operand of a `ref.as_non_null`
/// whose check was deferred to it (see [`DeferredNullCheck`]) makes that
/// check here, the same way, although validation then knows the operand to
/// be non-null.
fn emit_null_check(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    gc_ref: ir::Value,
    non_null: bool,
    access_end: Option<u32>,
) -> Option<ir::TrapCode> {
    let deferred = func_env
        .proven_references
        .deferred
        .take_if(|deferred| deferred.value == gc_ref);
    if let Some(deferred) = deferred {
        func_env.prove(gc_ref, Proven::NonNull);
        if access_end.is_some_and(|end| implicit_null_check(func_env, end)) {
            return Some(crate::TRAP_NULL_REFERENCE);
        }
        deferred.emit(func_env, builder);
        return None;
    }
    if non_null || func_env.is_proven(gc_ref, Proven::NonNull) {
        return None;
    }
    // Either way the caller's access follows at once, on every path, so
    // code translated after it has passed the check.
    func_env.prove(gc_ref, Proven::NonNull);
    if access_end.is_some_and(|end| implicit_null_check(func_env, end)) {
        return Some(crate::TRAP_NULL_REFERENCE);
    }
    func_env.trapz(builder, gc_ref, crate::TRAP_NULL_REFERENCE);
    None
}

/// Whether a field of this storage type holds a `funcref`, which is interned
/// before it is stored.
fn is_func_ref_storage(ty: &WasmStorageType) -> bool {
    matches!(
        ty,
        WasmStorageType::Val(WasmValType::Ref(r)) if r.heap_type.top() == WasmHeapTopType::Func
    )
}

/// Whether an access to bytes `..end` of a GC object may leave its null check
/// to the fault an access through a null reference (index 0) takes in the
/// copying heap's inaccessible null guard. That needs: signal-based traps,
/// no explicit heap bounds check before the access (the address is computed
/// directly from the reference, as it is when the 32-bit reservation plus
/// guard covers every reference), a heap that never moves (the runtime then
/// keeps the guard), and the access within the guard.
fn implicit_null_check(func_env: &mut FuncEnvironment<'_>, end: u32) -> bool {
    let memory_tunables =
        wasmtime_environ::MemoryTunables::new(func_env.tunables, MemoryKind::GcHeap);
    func_env.tunables.collector == Some(Collector::Copying)
        && end <= crate::copying_heap_data::NULL_GUARD_BYTES
        && gc_heap_confinement_limit(func_env).is_some()
        && !func_env
            .tunables
            .gc_heap_memory_type()
            .memory_may_move(&memory_tunables)
}

/// The exclusive end, in bytes from the GC heap base, of the address range in
/// which every byte is either GC heap or traps when accessed: the heap's
/// reservation plus its guard. `None` when accesses beyond the heap's length
/// cannot be relied on to fault (no signal-based traps, a 32-bit host, or
/// pages larger than the heap's), in which case callers keep explicit
/// checks against the heap's current bound.
fn gc_heap_confinement_limit(func_env: &mut FuncEnvironment<'_>) -> Option<u64> {
    if func_env.pointer_type() != ir::types::I64 || !func_env.clif_memory_traps_enabled() {
        return None;
    }
    let memory = func_env.tunables.gc_heap_memory_type();
    let memory_tunables =
        wasmtime_environ::MemoryTunables::new(func_env.tunables, MemoryKind::GcHeap);
    let page_size_log2 = func_env.target_config().page_size_align_log2;
    // The same condition under which `bounds_check_field_access` already
    // omits checks for a 32-bit reference plus a static offset.
    if !memory.can_elide_bounds_check(&memory_tunables, page_size_log2) {
        return None;
    }
    Some(memory_tunables.reservation() + memory_tunables.guard_size())
}

/// Emit inline CLIF code that asserts an object's `VMGcKind` matches the
/// expected kind. Only emits code when `cfg(gc_zeal)` is enabled.
///
/// `gc_ref` must be a non-null, non-i31 GC reference (i32 heap index).
fn emit_gc_kind_assert(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    gc_ref: ir::Value,
    expected_kind: VMGcKind,
) {
    if !cfg!(gc_zeal) {
        return;
    }

    func_env.trapz(builder, gc_ref, crate::TRAP_NULL_REFERENCE);

    let kind_addr = func_env.prepare_gc_ref_access(
        builder,
        gc_ref,
        BoundsCheck::StaticObjectField {
            offset: wasmtime_environ::VM_GC_HEADER_KIND_OFFSET,
            access_size: wasmtime_environ::VM_GC_KIND_SIZE,
            object_size: wasmtime_environ::VM_GC_HEADER_SIZE,
        },
    );
    let flags = func_env.gc_memflags(&mut builder.func).with_readonly();
    let kind_and_reserved_bits = builder.ins().load(ir::types::I32, flags, kind_addr, 0);
    let kind_mask = builder
        .ins()
        .iconst(ir::types::I32, i64::from(VMGcKind::MASK));
    let actual_kind = builder.ins().band(kind_and_reserved_bits, kind_mask);

    let expected_kind = builder
        .ins()
        .iconst(ir::types::I32, i64::from(expected_kind.as_u32()));

    // NB: Do a subtype check rather than a strict equality check. See
    // `VMGcKind::matches` for details.
    let and = builder.ins().band(actual_kind, expected_kind);
    let matches = builder.ins().icmp(IntCC::Equal, and, expected_kind);

    builder.ins().trapz(matches, TRAP_INTERNAL_ASSERT);
}

/// Read a struct field or array element from its raw address in the GC heap.
///
/// The given address MUST have already been bounds-checked via
/// `prepare_gc_ref_access`.
pub fn read_field_at_addr(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    ty: WasmStorageType,
    addr: ir::Value,
    null_trap: Option<ir::TrapCode>,
    extension: Option<Extension>,
) -> WasmResult<ir::Value> {
    assert_eq!(extension.is_none(), matches!(ty, WasmStorageType::Val(_)));
    assert_eq!(
        extension.is_some(),
        matches!(ty, WasmStorageType::I8 | WasmStorageType::I16)
    );

    // Data inside GC objects is always little endian. An access that is its
    // reference's null check carries that check's trap code.
    let mut flags = func_env
        .gc_memflags(&mut builder.func)
        .with_endianness(ir::Endianness::Little);
    if let Some(code) = null_trap {
        flags = flags.with_trap_code(Some(code));
    }

    let value = match ty {
        WasmStorageType::I8 => builder.ins().load(ir::types::I8, flags, addr, 0),
        WasmStorageType::I16 => builder.ins().load(ir::types::I16, flags, addr, 0),
        WasmStorageType::Val(v) => match v {
            WasmValType::I32 => builder.ins().load(ir::types::I32, flags, addr, 0),
            WasmValType::I64 => builder.ins().load(ir::types::I64, flags, addr, 0),
            WasmValType::F32 => builder.ins().load(ir::types::F32, flags, addr, 0),
            WasmValType::F64 => builder.ins().load(ir::types::F64, flags, addr, 0),
            WasmValType::V128 => builder.ins().load(ir::types::I8X16, flags, addr, 0),
            WasmValType::Ref(r) => match r.heap_type.top() {
                WasmHeapTopType::Any | WasmHeapTopType::Extern | WasmHeapTopType::Exn => {
                    gc_compiler(func_env)?
                        .translate_read_gc_reference(func_env, builder, r, addr, flags)?
                }
                WasmHeapTopType::Func => {
                    // The libcall checks the interned function against the
                    // engine-level type, which this module's `VMContext`
                    // already maps from the field's declared type; the
                    // reserved index asks for an untyped `funcref`.
                    let expected_ty = match r.heap_type {
                        WasmHeapType::Func => builder.ins().iconst(
                            ir::types::I32,
                            i64::from(VMSharedTypeIndex::reserved_value().as_bits()),
                        ),
                        WasmHeapType::ConcreteFunc(ty) => func_env.module_interned_to_shared_ty(
                            &mut builder.cursor(),
                            ty.unwrap_module_type_index(),
                        ),
                        WasmHeapType::NoFunc => {
                            let null = builder.ins().iconst(func_env.pointer_type(), 0);
                            // The field's value is known without reading it,
                            // but a read that is its reference's null check
                            // still has to be made.
                            if null_trap.is_some() {
                                let _ = builder.ins().load(ir::types::I32, flags, addr, 0);
                            }
                            if !r.nullable {
                                // Because `nofunc` is uninhabited, and this
                                // reference is non-null, this is unreachable
                                // code. Unconditionally trap via conditional
                                // trap instructions to avoid inserting block
                                // terminators in the middle of this block.
                                builder.ins().trapz(null, TRAP_INTERNAL_ASSERT);
                            }
                            return Ok(null);
                        }
                        _ => unreachable!("not a function heap type"),
                    };

                    let func_ref_id = builder.ins().load(ir::types::I32, flags, addr, 0);
                    if func_env.tunables.collector == Some(Collector::Copying) {
                        copying::translate_read_interned_func_ref(
                            func_env,
                            builder,
                            func_ref_id,
                            expected_ty,
                            matches!(r.heap_type, WasmHeapType::ConcreteFunc(_)),
                        )
                    } else {
                        let vmctx = func_env.vmctx_val(&mut builder.cursor());
                        let get_interned_func_ref = func_env
                            .builtin_functions
                            .get_interned_func_ref(builder.func);
                        let call_inst = builder
                            .ins()
                            .call(get_interned_func_ref, &[vmctx, func_ref_id, expected_ty]);
                        builder.func.dfg.first_result(call_inst)
                    }
                }
                WasmHeapTopType::Cont => {
                    // TODO(#10248) GC integration for stack switching
                    return stack_switching_unsupported();
                }
            },
        },
    };

    let value = match extension {
        Some(Extension::Sign) => builder.ins().sextend(ir::types::I32, value),
        Some(Extension::Zero) => builder.ins().uextend(ir::types::I32, value),
        None => value,
    };

    Ok(value)
}

pub fn intern_func_ref(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    ref_type: WasmRefType,
    func_ref: ir::Value,
) -> WasmResult<ir::Value> {
    assert_eq!(ref_type.heap_type.top(), WasmHeapTopType::Func);

    let vmctx = func_env.vmctx_val(&mut builder.cursor());

    let intern_func_ref_for_gc_heap = func_env
        .builtin_functions
        .intern_func_ref_for_gc_heap(builder.func);

    let func_ref = if ref_type.heap_type == WasmHeapType::NoFunc {
        let null = builder.ins().iconst(func_env.pointer_type(), 0);
        if !ref_type.nullable {
            // Because `nofunc` is uninhabited, and this reference is
            // non-null, this is unreachable code. Unconditionally trap
            // via conditional trap instructions to avoid inserting
            // block terminators in the middle of this block.
            builder.ins().trapz(null, TRAP_INTERNAL_ASSERT);
        }
        null
    } else {
        func_ref
    };

    // With the copying collector the null funcref and an address the
    // runtime's cache already holds need no call.
    if func_env.tunables.collector == Some(Collector::Copying) {
        return Ok(copying::translate_intern_func_ref(
            func_env, builder, func_ref,
        ));
    }

    // Convert the raw `funcref` into a `FuncRefTableId` for use in the
    // GC heap.
    let call_inst = builder
        .ins()
        .call(intern_func_ref_for_gc_heap, &[vmctx, func_ref]);
    let func_ref_id = builder.func.dfg.first_result(call_inst);
    Ok(builder.ins().ireduce(ir::types::I32, func_ref_id))
}

fn write_func_ref_at_addr(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    ref_type: WasmRefType,
    flags: ir::MemFlagsData,
    field_addr: ir::Value,
    func_ref: ir::Value,
) -> WasmResult<()> {
    // Convert the raw `funcref` into a `FuncRefTableId` for use in the
    // GC heap.
    let func_ref_id = intern_func_ref(func_env, builder, ref_type, func_ref)?;

    // Store the id in the field.
    builder.ins().store(flags, func_ref_id, field_addr, 0);

    Ok(())
}

pub fn init_field_at_addr(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    field_ty: WasmStorageType,
    field_addr: ir::Value,
    new_val: ir::Value,
) -> WasmResult<()> {
    gc_compiler(func_env)?.init_field(func_env, builder, field_ty, field_addr, new_val)
}

pub fn write_field_at_addr(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    field_ty: WasmStorageType,
    field_addr: ir::Value,
    null_trap: Option<ir::TrapCode>,
    new_val: ir::Value,
    value_ty: Option<WasmRefType>,
) -> WasmResult<()> {
    write_or_init_moving_field_at_addr(
        func_env, builder, field_ty, field_addr, null_trap, new_val, value_ty, false,
    )
}

/// Initialize an uninitialized slot in a moving GC object that may already have
/// been promoted to old space by an earlier poll. Stores the payload before
/// publishing the copying-GC remembered-set edge so a fault between the two
/// never exposes uninitialized bytes to minor collection.
pub fn init_moving_field_at_addr(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    field_ty: WasmStorageType,
    field_addr: ir::Value,
    new_val: ir::Value,
    value_ty: Option<WasmRefType>,
) -> WasmResult<()> {
    write_or_init_moving_field_at_addr(
        func_env, builder, field_ty, field_addr, None, new_val, value_ty, true,
    )
}

fn write_or_init_moving_field_at_addr(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    field_ty: WasmStorageType,
    field_addr: ir::Value,
    null_trap: Option<ir::TrapCode>,
    new_val: ir::Value,
    value_ty: Option<WasmRefType>,
    store_before_barrier: bool,
) -> WasmResult<()> {
    // Data inside GC objects is always little endian. An access that is its
    // reference's null check carries that check's trap code; a `funcref` is
    // interned before it is stored, so it never is one.
    let mut flags = func_env
        .gc_memflags(&mut builder.func)
        .with_endianness(ir::Endianness::Little);
    if let Some(code) = null_trap {
        debug_assert!(!is_func_ref_storage(&field_ty));
        flags = flags.with_trap_code(Some(code));
    }

    match field_ty {
        WasmStorageType::I8 => {
            builder.ins().istore8(flags, new_val, field_addr, 0);
        }
        WasmStorageType::I16 => {
            builder.ins().istore16(flags, new_val, field_addr, 0);
        }
        WasmStorageType::Val(WasmValType::Ref(r)) => match r.heap_type.top() {
            WasmHeapTopType::Func => {
                write_func_ref_at_addr(func_env, builder, r, flags, field_addr, new_val)?
            }
            WasmHeapTopType::Extern | WasmHeapTopType::Any | WasmHeapTopType::Exn => {
                if store_before_barrier {
                    gc_compiler(func_env)?.translate_write_gc_reference(
                        func_env, builder, r, field_addr, new_val, flags,
                    )?;
                }
                if func_env.tunables.collector == Some(Collector::Copying) {
                    // The validator may know more about the operand than the
                    // destination field requires. Use it only for the barrier.
                    copying::emit_old_to_young_barrier(
                        func_env,
                        builder,
                        value_ty.unwrap_or(r),
                        field_addr,
                        new_val,
                    )?;
                }
                if !store_before_barrier {
                    gc_compiler(func_env)?.translate_write_gc_reference(
                        func_env, builder, r, field_addr, new_val, flags,
                    )?;
                }
            }
            WasmHeapTopType::Cont => return stack_switching_unsupported(),
        },
        WasmStorageType::Val(_) => {
            assert_eq!(
                builder.func.dfg.value_type(new_val).bytes(),
                wasmtime_environ::byte_size_of_wasm_ty_in_gc_heap(&field_ty)
            );
            builder.ins().store(flags, new_val, field_addr, 0);
        }
    }
    Ok(())
}

pub fn translate_struct_new(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    struct_type_index: TypeIndex,
    fields: &[ir::Value],
) -> WasmResult<ir::Value> {
    gc_compiler(func_env)?.alloc_struct(func_env, builder, struct_type_index, &fields)
}

fn default_value(
    cursor: &mut FuncCursor,
    func_env: &FuncEnvironment<'_>,
    ty: &WasmStorageType,
) -> ir::Value {
    match ty {
        WasmStorageType::I8 | WasmStorageType::I16 => cursor.ins().iconst(ir::types::I32, 0),
        WasmStorageType::Val(v) => match v {
            WasmValType::I32 => cursor.ins().iconst(ir::types::I32, 0),
            WasmValType::I64 => cursor.ins().iconst(ir::types::I64, 0),
            WasmValType::F32 => cursor.ins().f32const(0.0),
            WasmValType::F64 => cursor.ins().f64const(0.0),
            WasmValType::V128 => {
                let c = cursor.func.dfg.constants.insert(vec![0; 16].into());
                cursor.ins().vconst(ir::types::I8X16, c)
            }
            WasmValType::Ref(r) => {
                assert!(r.nullable);
                let (ty, needs_stack_map) = func_env.reference_type(r.heap_type);

                // NB: The collector doesn't need to know about null references.
                let _ = needs_stack_map;

                cursor.ins().iconst(ty, 0)
            }
        },
    }
}

pub fn translate_struct_new_default(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    struct_type_index: TypeIndex,
) -> WasmResult<ir::Value> {
    let interned_ty = func_env.module.types[struct_type_index].unwrap_module_type_index();
    let struct_ty = func_env.types.unwrap_struct(interned_ty)?;
    let fields = struct_ty
        .fields
        .iter()
        .map(|f| default_value(&mut builder.cursor(), func_env, &f.element_type))
        .collect::<StructFieldsVec>();
    gc_compiler(func_env)?.alloc_struct(func_env, builder, struct_type_index, &fields)
}

pub fn translate_struct_get(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    struct_type_index: TypeIndex,
    field_index: u32,
    struct_ref: ir::Value,
    non_null: bool,
    extension: Option<Extension>,
) -> WasmResult<ir::Value> {
    log::trace!(
        "translate_struct_get({struct_type_index:?}, {field_index:?}, {struct_ref:?}, {extension:?})"
    );

    let field_index = usize::try_from(field_index).unwrap();
    let interned_type_index = func_env.module.types[struct_type_index].unwrap_module_type_index();

    let struct_layout = func_env.struct_or_exn_layout(interned_type_index);
    let struct_size = struct_layout.size;

    let field_offset = struct_layout.fields[field_index].offset;
    let field_ty = &func_env.types.unwrap_struct(interned_type_index)?.fields[field_index];
    let field_size = wasmtime_environ::byte_size_of_wasm_ty_in_gc_heap(&field_ty.element_type);
    assert!(field_offset + field_size <= struct_size);
    let field_ty = field_ty.element_type;

    let null_trap = emit_null_check(
        func_env,
        builder,
        struct_ref,
        non_null,
        Some(field_offset + field_size),
    );

    emit_gc_kind_assert(func_env, builder, struct_ref, VMGcKind::StructRef);

    let field_addr = func_env.prepare_gc_ref_access(
        builder,
        struct_ref,
        BoundsCheck::StaticObjectField {
            offset: field_offset,
            access_size: u8::try_from(field_size).unwrap(),
            object_size: struct_size,
        },
    );

    let result = read_field_at_addr(func_env, builder, field_ty, field_addr, null_trap, extension);
    log::trace!("translate_struct_get(..) -> {result:?}");
    result
}

pub fn translate_struct_set(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    struct_type_index: TypeIndex,
    field_index: u32,
    struct_ref: ir::Value,
    non_null: bool,
    new_val: ir::Value,
    value_ty: Option<WasmRefType>,
) -> WasmResult<()> {
    log::trace!(
        "translate_struct_set({struct_type_index:?}, {field_index:?}, struct_ref: {struct_ref:?}, new_val: {new_val:?})"
    );

    let field_index = usize::try_from(field_index).unwrap();
    let interned_type_index = func_env.module.types[struct_type_index].unwrap_module_type_index();

    let struct_layout = func_env.struct_or_exn_layout(interned_type_index);
    let struct_size = struct_layout.size;

    let field_offset = struct_layout.fields[field_index].offset;
    let field_ty = &func_env.types.unwrap_struct(interned_type_index)?.fields[field_index];
    let field_size = wasmtime_environ::byte_size_of_wasm_ty_in_gc_heap(&field_ty.element_type);
    assert!(field_offset + field_size <= struct_size);
    let field_ty = field_ty.element_type;

    let null_trap = emit_null_check(
        func_env,
        builder,
        struct_ref,
        non_null,
        (!is_func_ref_storage(&field_ty)).then_some(field_offset + field_size),
    );

    emit_gc_kind_assert(func_env, builder, struct_ref, VMGcKind::StructRef);

    let field_addr = func_env.prepare_gc_ref_access(
        builder,
        struct_ref,
        BoundsCheck::StaticObjectField {
            offset: field_offset,
            access_size: u8::try_from(field_size).unwrap(),
            object_size: struct_size,
        },
    );

    write_field_at_addr(
        func_env, builder, field_ty, field_addr, null_trap, new_val, value_ty,
    )?;

    log::trace!("translate_struct_set: finished");
    Ok(())
}

pub fn translate_exn_unbox(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    tag_index: TagIndex,
    exn_ref: ir::Value,
) -> WasmResult<SmallVec<[ir::Value; 4]>> {
    log::trace!("translate_exn_unbox({tag_index:?}, {exn_ref:?})");

    // We know that the `exn_ref` is not null because we reach this
    // operation only in catch blocks, and throws are initiated from
    // runtime code that checks for nulls first.

    // Get the GcExceptionLayout associated with this tag's
    // function type, and generate loads for each field.
    let exception_ty_idx = func_env
        .exception_type_from_tag(tag_index)
        .unwrap_module_type_index();
    let exception_ty = func_env.types.unwrap_exn(exception_ty_idx)?;
    let exn_layout = func_env.struct_or_exn_layout(exception_ty_idx);
    let exn_size = exn_layout.size;

    // Gather accesses first because these require a borrow on
    // `func_env`, which we later mutate below via
    // `prepare_gc_ref_access()`.
    let mut accesses: SmallVec<[_; 4]> = smallvec![];
    for (field_ty, field_layout) in exception_ty.fields.iter().zip(exn_layout.fields.iter()) {
        accesses.push((field_layout.offset, field_ty.element_type));
    }

    let mut result = smallvec![];
    for (field_offset, field_ty) in accesses {
        let field_size = wasmtime_environ::byte_size_of_wasm_ty_in_gc_heap(&field_ty);
        assert!(field_offset + field_size <= exn_size);
        let field_addr = func_env.prepare_gc_ref_access(
            builder,
            exn_ref,
            BoundsCheck::StaticObjectField {
                offset: field_offset,
                access_size: u8::try_from(field_size).unwrap(),
                object_size: exn_size,
            },
        );

        let value = read_field_at_addr(func_env, builder, field_ty, field_addr, None, None)?;
        result.push(value);
    }

    log::trace!("translate_exn_unbox(..) -> {result:?}");
    Ok(result)
}

pub fn translate_exn_throw(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    tag_index: TagIndex,
    args: &[ir::Value],
) -> WasmResult<()> {
    let (instance_id, defined_tag_id) = func_env.get_instance_and_tag(builder, tag_index);
    let exnref = gc_compiler(func_env)?.alloc_exn(
        func_env,
        builder,
        tag_index,
        args,
        instance_id,
        defined_tag_id,
    )?;
    translate_exn_throw_ref(func_env, builder, exnref)
}

pub fn translate_exn_throw_ref(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    exnref: ir::Value,
) -> WasmResult<()> {
    let builtin = func_env.builtin_functions.throw_ref(builder.func);
    let sig = builder.func.dfg.ext_funcs[builtin].signature;
    let vmctx = func_env.vmctx_val(&mut builder.cursor());

    // Generate a `try_call` with handlers from the current
    // stack. This libcall is unique among libcall implementations of
    // opcodes: we know the others will not throw, but `throw_ref`'s
    // entire purpose is to throw. So if there are any handlers in the
    // local function body, we need to attach them to this callsite
    // like any other.
    let continuation = builder.create_block();
    let current_block = builder.current_block().unwrap();
    builder.insert_block_after(continuation, current_block);
    let continuation_call = builder.func.dfg.block_call(continuation, &[]);
    let mut table_items = vec![ExceptionTableItem::Context(vmctx)];
    for (tag, block) in func_env.stacks.handlers.handlers() {
        let block_call = builder
            .func
            .dfg
            .block_call(block, &[BlockArg::TryCallExn(0)]);
        table_items.push(match tag {
            Some(tag) => ExceptionTableItem::Tag(tag, block_call),
            None => ExceptionTableItem::Default(block_call),
        });
    }
    let etd = ExceptionTableData::new(sig, continuation_call, table_items);
    let et = builder.func.dfg.exception_tables.push(etd);

    builder.ins().try_call(builtin, &[vmctx, exnref], et);

    builder.switch_to_block(continuation);
    builder.seal_block(continuation);
    func_env.trap(builder, crate::TRAP_UNREACHABLE);

    Ok(())
}

pub fn translate_array_new(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder,
    array_type_index: TypeIndex,
    elem: ir::Value,
    len: ir::Value,
) -> WasmResult<ir::Value> {
    log::trace!("translate_array_new({array_type_index:?}, {elem:?}, {len:?})");
    let result =
        gc_compiler(func_env)?.alloc_uninit_array(func_env, builder, array_type_index, len)?;
    let zero = builder.ins().iconst(ir::types::I32, 0);
    let ty = func_env.module.types[array_type_index].unwrap_module_type_index();
    func_env.translate_entity_fill(
        builder,
        CheckedEntity::Array {
            array: result,
            ty,
            initialized: false,
        },
        zero,
        elem,
        len,
    )?;
    log::trace!("translate_array_new(..) -> {result:?}");
    Ok(result)
}

pub fn translate_array_new_default(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder,
    array_type_index: TypeIndex,
    len: ir::Value,
) -> WasmResult<ir::Value> {
    log::trace!("translate_array_new_default({array_type_index:?}, {len:?})");

    let interned_ty = func_env.module.types[array_type_index].unwrap_module_type_index();
    let array_ty = func_env.types.unwrap_array(interned_ty)?;
    let result =
        gc_compiler(func_env)?.alloc_uninit_array(func_env, builder, array_type_index, len)?;
    log::trace!("translate_array_new_default(..) -> {result:?}");
    let elem = default_value(&mut builder.cursor(), func_env, &array_ty.0.element_type);
    let zero = builder.ins().iconst(ir::types::I32, 0);
    func_env.translate_entity_fill(
        builder,
        CheckedEntity::Array {
            array: result,
            ty: interned_ty,
            initialized: false,
        },
        zero,
        elem,
        len,
    )?;
    Ok(result)
}

pub fn translate_array_new_fixed(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder,
    array_type_index: TypeIndex,
    elems: &[ir::Value],
) -> WasmResult<ir::Value> {
    log::trace!("translate_array_new_fixed({array_type_index:?}, {elems:?})");
    let len = builder
        .ins()
        .iconst(ir::types::I32, i64::try_from(elems.len()).unwrap());
    let result =
        gc_compiler(func_env)?.alloc_uninit_array(func_env, builder, array_type_index, len)?;
    log::trace!("translate_array_new_fixed(..) -> {result:?}");
    let ty = func_env.module.types[array_type_index].unwrap_module_type_index();
    let array_ty = func_env.types.unwrap_array(ty)?;

    for (i, elem) in elems.iter().enumerate() {
        let index = builder
            .ins()
            .iconst(ir::types::I32, i64::try_from(i).unwrap());
        let addr = array_elem_addr(func_env, builder, ty, result, true, index)?;
        gc_compiler(func_env)?.init_field(
            func_env,
            builder,
            array_ty.0.element_type,
            addr,
            *elem,
        )?;
    }
    Ok(result)
}

pub fn translate_array_len(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder,
    array_ref: ir::Value,
    non_null: bool,
) -> WasmResult<ir::Value> {
    log::trace!("translate_array_len({array_ref:?})");

    let len_offset = gc_compiler(func_env)?.layouts().array_length_field_offset();
    let null_trap = emit_null_check(
        func_env,
        builder,
        array_ref,
        non_null,
        Some(len_offset + ir::types::I32.bytes()),
    );

    let len_field = func_env.prepare_gc_ref_access(
        builder,
        array_ref,
        // Note: We can't bounds check the whole array object's size because we
        // don't know its length yet. Chicken and egg problem.
        BoundsCheck::StaticOffset {
            offset: len_offset,
            access_size: u8::try_from(ir::types::I32.bytes()).unwrap(),
        },
    );
    let mut flags = func_env.gc_memflags(&mut builder.func).with_readonly();
    if let Some(code) = null_trap {
        flags = flags.with_trap_code(Some(code));
    }
    let result = builder.ins().load(ir::types::I32, flags, len_field, 0);
    log::trace!("translate_array_len(..) -> {result:?}");
    Ok(result)
}

struct ArraySizeInfo {
    /// The `i32` size of the whole array object, in bytes.
    obj_size: ir::Value,

    /// The `i32` size of each one of the array's elements, in bytes.
    one_elem_size: ir::Value,

    /// The `i32` size of the array's base object, in bytes. This is also the
    /// offset from the start of the array object to its elements.
    base_size: ir::Value,
}

/// Emit code to get the dynamic size (in bytes) of a whole array object, along
/// with some other related bits.
fn emit_array_size_info(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    array_type_index: ModuleInternedTypeIndex,
    // `i32` value containing the array's length.
    array_len: ir::Value,
) -> WasmResult<ArraySizeInfo> {
    let array_layout = func_env.array_layout(array_type_index)?;

    // Note that we check for overflow below because we can't trust the array's
    // length: it came from inside the GC heap.
    //
    // We check for 32-bit multiplication overflow by performing a 64-bit
    // multiplication and testing the high bits.
    let one_elem_size = builder
        .ins()
        .iconst(ir::types::I64, i64::from(array_layout.elem_size));
    let array_len = builder.ins().uextend(ir::types::I64, array_len);
    let all_elems_size = builder.ins().imul(one_elem_size, array_len);

    let high_bits = builder.ins().ushr_imm_u(all_elems_size, 32);
    builder.ins().trapnz(high_bits, TRAP_GC_HEAP_CORRUPT);

    let all_elems_size = builder.ins().ireduce(ir::types::I32, all_elems_size);
    let base_size = builder
        .ins()
        .iconst(ir::types::I32, i64::from(array_layout.base_size));
    let obj_size =
        builder
            .ins()
            .uadd_overflow_trap(all_elems_size, base_size, TRAP_GC_HEAP_CORRUPT);

    let one_elem_size = builder.ins().ireduce(ir::types::I32, one_elem_size);

    Ok(ArraySizeInfo {
        obj_size,
        one_elem_size,
        base_size,
    })
}

/// Get the bounds-checked address of an element in an array.
///
/// The emitted code will trap if `index >= array.length`.
///
/// Returns the `ir::Value` containing the address of the `index`th element in
/// the array. You may read or write a value of the array's element type at this
/// address. You may not use it for any other kind of access, nor reuse this
/// value across GC safepoints.
fn array_elem_addr(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    array_type_index: ModuleInternedTypeIndex,
    array_ref: ir::Value,
    non_null: bool,
    index: ir::Value,
) -> WasmResult<ir::Value> {
    // First, assert that `index < array.length`.
    //
    // This check is visible at the Wasm-semantics level.
    //
    // TODO: We should emit spectre-safe bounds checks for array accesses (if
    // configured) but we don't currently have a great way to do that here. The
    // proper solution is to use linear memories to back GC heaps and reuse the
    // code in `bounds_check.rs` to implement these bounds checks. That is all
    // planned, but not yet implemented.

    let len = translate_array_len(func_env, builder, array_ref, non_null).unwrap();

    let in_bounds = builder.ins().icmp(IntCC::UnsignedLessThan, index, len);
    func_env.trapz(builder, in_bounds, TRAP_ARRAY_OUT_OF_BOUNDS);

    // Then the element's address, without trusting the length the check
    // above read from the heap.
    let (elem_size, base_size) = {
        let layout = func_env.array_layout(array_type_index)?;
        (layout.elem_size, layout.base_size)
    };
    if gc_heap_confinement_limit(func_env)
        .is_some_and(|limit| limit >= (1 << 32) + u64::from(elem_size))
    {
        // The element's heap offset is `ref + base_size + index * elem_size`,
        // computed in 32 bits: the product and both sums wrap at 2^32, so the
        // offset is below 2^32 whatever a corrupt length let `index` be. The
        // access then adds at most `elem_size` (at most 16) bytes, so it ends
        // below 2^32 + 16: inside the heap's 4 GiB reservation plus its guard,
        // where every byte is heap or faults. For an intact heap nothing
        // wraps, since the whole array lies inside the 32-bit heap, and the
        // offset is exact.
        let elems = builder.ins().imul_imm_u(index, i64::from(elem_size));
        let offset = builder.ins().iadd(array_ref, elems);
        let offset = builder.ins().iadd_imm_u(offset, i64::from(base_size));
        let offset = builder.ins().uextend(func_env.pointer_type(), offset);
        let base = func_env.get_gc_heap_base(builder)?;
        return Ok(builder.ins().iadd(base, offset));
    }

    // Compute the size (in bytes) of the whole array object.
    let ArraySizeInfo {
        obj_size,
        one_elem_size,
        base_size,
    } = emit_array_size_info(func_env, builder, array_type_index, len)?;

    // Compute the offset of the `index`th element within the array object.
    //
    // NB: no need to check for overflow here, since at this point we know that
    // `len * elem_size + base_size` did not overflow and `i < len`.
    let offset_in_elems = builder.ins().imul(index, one_elem_size);
    let offset_in_array = builder.ins().iadd(offset_in_elems, base_size);

    // Finally, use the object size and element offset we just computed to
    // perform our implementation-internal bounds checks.
    //
    // Checking the whole object's size, rather than the `index`th element's
    // size allows these bounds checks to be deduplicated across repeated
    // accesses to the same array at different indices.
    //
    // This check should not be visible to Wasm, and serve to protect us from
    // our own implementation bugs. The goal is to keep any potential widgets
    // confined within the GC heap, and turn what would otherwise be a security
    // vulnerability into a simple bug.
    //
    // TODO: Ideally we should fold the first Wasm-visible bounds check into
    // this internal bounds check, so that we aren't performing multiple,
    // redundant bounds checks. But we should figure out how to do this in a way
    // that doesn't defeat the object-size bounds checking's deduplication
    // mentioned above.
    Ok(func_env.prepare_gc_ref_access(
        builder,
        array_ref,
        BoundsCheck::DynamicObjectField {
            offset: offset_in_array,
            object_size: obj_size,
        },
    ))
}

pub fn translate_array_get(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder,
    array_type_index: TypeIndex,
    array_ref: ir::Value,
    non_null: bool,
    index: ir::Value,
    extension: Option<Extension>,
) -> WasmResult<ir::Value> {
    log::trace!("translate_array_get({array_type_index:?}, {array_ref:?}, {index:?})");

    emit_gc_kind_assert(func_env, builder, array_ref, VMGcKind::ArrayRef);

    let array_type_index = func_env.module.types[array_type_index].unwrap_module_type_index();
    let elem_addr =
        array_elem_addr(func_env, builder, array_type_index, array_ref, non_null, index)?;

    let array_ty = func_env.types.unwrap_array(array_type_index)?;
    let elem_ty = array_ty.0.element_type;

    let result = read_field_at_addr(func_env, builder, elem_ty, elem_addr, None, extension)?;
    log::trace!("translate_array_get(..) -> {result:?}");
    Ok(result)
}

pub fn translate_array_set(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder,
    array_type_index: TypeIndex,
    array_ref: ir::Value,
    non_null: bool,
    index: ir::Value,
    value: ir::Value,
    value_ty: Option<WasmRefType>,
) -> WasmResult<()> {
    log::trace!("translate_array_set({array_type_index:?}, {array_ref:?}, {index:?}, {value:?})");

    emit_gc_kind_assert(func_env, builder, array_ref, VMGcKind::ArrayRef);

    let array_type_index = func_env.module.types[array_type_index].unwrap_module_type_index();
    let elem_addr =
        array_elem_addr(func_env, builder, array_type_index, array_ref, non_null, index)?;

    let array_ty = func_env.types.unwrap_array(array_type_index)?;
    let elem_ty = array_ty.0.element_type;

    write_field_at_addr(func_env, builder, elem_ty, elem_addr, None, value, value_ty)?;

    log::trace!("translate_array_set: finished");
    Ok(())
}

pub fn translate_ref_test(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    test_ty: WasmRefType,
    val: ir::Value,
    val_ty: WasmRefType,
) -> WasmResult<ir::Value> {
    log::trace!("translate_ref_test({test_ty:?}, {val:?})");
    let check_null = val_ty.nullable && !func_env.is_proven(val, Proven::NonNull);

    // First special case: testing for references to bottom types.
    if test_ty.heap_type.is_bottom() {
        let result = if test_ty.nullable && check_null {
            // All null references (within the same type hierarchy) match null
            // references to the bottom type.
            func_env.translate_ref_is_null(builder.cursor(), val, val_ty)?
        } else {
            // A nullable bottom type contains only null. A non-nullable
            // bottom type is uninhabited.
            builder.ins().iconst(ir::types::I32, 0)
        };
        log::trace!("translate_ref_test(..) -> {result:?}");
        return Ok(result);
    }

    // And because `ref.test heap_ty` is only valid on operands whose type is in
    // the same type hierarchy as `heap_ty`, if `heap_ty` is its hierarchy's top
    // type, we only need to worry about whether we are testing for nullability
    // or not.
    if test_ty.heap_type.is_top() {
        let result = if test_ty.nullable || !check_null {
            builder.ins().iconst(ir::types::I32, 1)
        } else {
            let is_null = func_env.translate_ref_is_null(builder.cursor(), val, val_ty)?;
            let zero = builder.ins().iconst(ir::types::I32, 0);
            let one = builder.ins().iconst(ir::types::I32, 1);
            builder.ins().select(is_null, zero, one)
        };
        log::trace!("translate_ref_test(..) -> {result:?}");
        return Ok(result);
    }

    // `i31ref`s are a little interesting because they don't point to GC
    // objects; we test the bit pattern of the reference itself.
    if test_ty.heap_type == WasmHeapType::I31 {
        let i31_mask = builder.ins().iconst(
            ir::types::I32,
            i64::from(wasmtime_environ::I31_DISCRIMINANT),
        );
        let is_i31 = builder.ins().band(val, i31_mask);
        let result = if test_ty.nullable && check_null {
            let is_null = func_env.translate_ref_is_null(builder.cursor(), val, val_ty)?;
            builder.ins().bor(is_null, is_i31)
        } else {
            is_i31
        };
        log::trace!("translate_ref_test(..) -> {result:?}");
        return Ok(result);
    }

    // A value that this function's code has already found to be a non-null
    // instance of the tested type passes (see `ProvenReferences`).
    if let WasmHeapType::ConcreteArray(ty)
    | WasmHeapType::ConcreteStruct(ty)
    | WasmHeapType::ConcreteExn(ty) = test_ty.heap_type
    {
        if func_env.is_proven(val, Proven::Cast(ty.unwrap_module_type_index())) {
            return Ok(builder.ins().iconst(ir::types::I32, 1));
        }
    }

    // Inspect the object's actual type after dispatching any null or i31
    // operands. Only anyref, eqref, and i31ref can hold an i31.
    let check_i31 = test_ty.heap_type.top() == WasmHeapTopType::Any
        && matches!(
            val_ty.heap_type,
            WasmHeapType::Any | WasmHeapType::Eq | WasmHeapType::I31
        );
    let join = (check_null || check_i31).then(|| {
        let block = builder.create_block();
        let result = builder.append_block_param(block, ir::types::I32);
        (block, result)
    });

    if check_null {
        let non_null_block = builder.create_block();
        let is_null = func_env.translate_ref_is_null(builder.cursor(), val, val_ty)?;
        let result_when_is_null = builder
            .ins()
            .iconst(ir::types::I32, test_ty.nullable as i64);
        builder.ins().brif(
            is_null,
            join.unwrap().0,
            &[result_when_is_null.into()],
            non_null_block,
            &[],
        );
        builder.switch_to_block(non_null_block);
        builder.seal_block(non_null_block);
    }

    if check_i31 {
        let heap_ref_block = builder.create_block();
        let i31_mask = builder.ins().iconst(
            ir::types::I32,
            i64::from(wasmtime_environ::I31_DISCRIMINANT),
        );
        let is_i31 = builder.ins().band(val, i31_mask);
        // If it is an `i31`, then create the result value based on whether we
        // want `i31`s to pass the test or not.
        let result_when_is_i31 = builder.ins().iconst(
            ir::types::I32,
            matches!(
                test_ty.heap_type,
                WasmHeapType::Any | WasmHeapType::Eq | WasmHeapType::I31
            ) as i64,
        );
        builder.ins().brif(
            is_i31,
            join.unwrap().0,
            &[result_when_is_i31.into()],
            heap_ref_block,
            &[],
        );
        builder.switch_to_block(heap_ref_block);
        builder.seal_block(heap_ref_block);
    }

    // Read the actual `VMGcKind` or `VMSharedTypeIndex` out of the object's
    // header and check whether it matches the expected type.
    let check_header_kind = |func_env: &mut FuncEnvironment<'_>,
                             builder: &mut FunctionBuilder,
                             val: ir::Value,
                             expected_kind: VMGcKind|
     -> ir::Value {
        let kind_addr = func_env.prepare_gc_ref_access(
            builder,
            val,
            BoundsCheck::StaticObjectField {
                offset: wasmtime_environ::VM_GC_HEADER_KIND_OFFSET,
                access_size: wasmtime_environ::VM_GC_KIND_SIZE,
                object_size: wasmtime_environ::VM_GC_HEADER_SIZE,
            },
        );
        let gc_memflags = func_env.gc_memflags(&mut builder.func);
        let actual_kind =
            builder
                .ins()
                .load(ir::types::I32, gc_memflags.with_readonly(), kind_addr, 0);
        let expected_kind = builder
            .ins()
            .iconst(ir::types::I32, i64::from(expected_kind.as_u32()));
        // Inline version of `VMGcKind::matches`.
        let and = builder.ins().band(actual_kind, expected_kind);
        let kind_matches = builder
            .ins()
            .icmp(ir::condcodes::IntCC::Equal, and, expected_kind);
        builder.ins().uextend(ir::types::I32, kind_matches)
    };
    let result = match test_ty.heap_type {
        WasmHeapType::Any
        | WasmHeapType::None
        | WasmHeapType::Extern
        | WasmHeapType::NoExtern
        | WasmHeapType::Func
        | WasmHeapType::NoFunc
        | WasmHeapType::Cont
        | WasmHeapType::NoCont
        | WasmHeapType::Exn
        | WasmHeapType::NoExn
        | WasmHeapType::I31 => unreachable!("handled top, bottom, and i31 types above"),

        // For these abstract but non-top and non-bottom types, we check the
        // `VMGcKind` that is in the object's header.
        WasmHeapType::Eq => check_header_kind(func_env, builder, val, VMGcKind::EqRef),
        WasmHeapType::Struct => check_header_kind(func_env, builder, val, VMGcKind::StructRef),
        WasmHeapType::Array => check_header_kind(func_env, builder, val, VMGcKind::ArrayRef),

        // For concrete types, we need to do a full subtype check between the
        // `VMSharedTypeIndex` in the object's header and the
        // `ModuleInternedTypeIndex` we have here.
        //
        // TODO: This check should ideally be done inline, but we don't have a
        // good way to access the `TypeRegistry`'s supertypes arrays from Wasm
        // code at the moment.
        WasmHeapType::ConcreteArray(ty)
        | WasmHeapType::ConcreteStruct(ty)
        | WasmHeapType::ConcreteExn(ty) => {
            let expected_interned_ty = ty.unwrap_module_type_index();
            let expected_shared_ty =
                func_env.module_interned_to_shared_ty(&mut builder.cursor(), expected_interned_ty);

            let ty_addr = func_env.prepare_gc_ref_access(
                builder,
                val,
                BoundsCheck::StaticOffset {
                    offset: wasmtime_environ::VM_GC_HEADER_TYPE_INDEX_OFFSET,
                    access_size: func_env.offsets.size_of_vmshared_type_index(),
                },
            );
            let gc_memflags = func_env.gc_memflags(&mut builder.func);
            let actual_shared_ty =
                builder
                    .ins()
                    .load(ir::types::I32, gc_memflags.with_readonly(), ty_addr, 0);

            func_env.is_subtype(
                builder,
                actual_shared_ty,
                expected_shared_ty,
                expected_interned_ty,
            )
        }

        // Same as for concrete arrays and structs except that a `VMFuncRef`
        // doesn't begin with a `VMGcHeader` and is a raw pointer rather than GC
        // heap index.
        WasmHeapType::ConcreteFunc(ty) => {
            let expected_interned_ty = ty.unwrap_module_type_index();
            let expected_shared_ty =
                func_env.module_interned_to_shared_ty(&mut builder.cursor(), expected_interned_ty);

            let actual_shared_ty = func_env.alias_regions.vmfuncref_type_index(
                &mut builder.cursor(),
                ir::MemFlagsData::trusted().with_readonly(),
                val,
            );

            func_env.is_subtype(
                builder,
                actual_shared_ty,
                expected_shared_ty,
                expected_interned_ty,
            )
        }
        WasmHeapType::ConcreteCont(_) => {
            // TODO(#10248) GC integration for stack switching
            return stack_switching_unsupported();
        }
    };
    let result = if let Some((block, joined_result)) = join {
        builder.ins().jump(block, &[result.into()]);
        builder.switch_to_block(block);
        builder.seal_block(block);
        joined_result
    } else {
        result
    };
    log::trace!("translate_ref_test(..) -> {result:?}");

    // Code that runs only when `result` is not zero knows `val` to be a
    // `test_ty`.
    func_env.proven_references.tested = Some((result, val, test_ty));

    Ok(result)
}

/// Translate `ref.cast cast_ty` of `val`, whose static type is `val_ty`.
///
/// A cast to a non-nullable concrete struct, array or exception type is a
/// sequence of checks that each trap: not null, not an `i31`, and the type in
/// the object's header. It has no join of paths that each carry a truth
/// value, which `ref.test` followed by a trap has. Any other cast is that
/// test and a trap.
///
/// A cast that the function's code has already made of the same value, at a
/// place that every path to this one passes, is not made again (see
/// [`ProvenReferences`]).
pub fn translate_ref_cast(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    cast_ty: WasmRefType,
    val: ir::Value,
    val_ty: WasmRefType,
) -> WasmResult<()> {
    log::trace!("translate_ref_cast({cast_ty:?}, {val:?})");
    let concrete = match cast_ty.heap_type {
        WasmHeapType::ConcreteArray(ty)
        | WasmHeapType::ConcreteStruct(ty)
        | WasmHeapType::ConcreteExn(ty) => Some(ty.unwrap_module_type_index()),
        _ => None,
    };
    if let Some(ty) = concrete {
        // A value known to be a non-null `ty` passes the cast to `ty`,
        // nullable or not.
        if func_env.is_proven(val, Proven::Cast(ty)) {
            return Ok(());
        }
    }
    let (Some(expected_interned_ty), false) = (concrete, cast_ty.nullable) else {
        let cast_okay = translate_ref_test(func_env, builder, cast_ty, val, val_ty)?;
        func_env.trapz(builder, cast_okay, crate::TRAP_CAST_FAILURE);
        prove_cast(func_env, val, cast_ty);
        return Ok(());
    };

    // The same checks as `translate_ref_test` makes for such a type; each
    // failure is the cast's, so their order is not observable: a null
    // reference fails the cast whichever check finds it, and so does an
    // `i31`, which is not null.
    //
    // (Scriptw patch) When the header load lies within the copying heap's
    // null guard (see `implicit_null_check`), the load through a null
    // reference faults there, and carrying the cast's trap code it is the
    // null check. The `i31` test then comes first: the null reference, 0,
    // has the `i31` bit clear and passes it to reach the load, an `i31`
    // never reaches the load, and nothing between the two can trap.
    let header_end = wasmtime_environ::VM_GC_HEADER_TYPE_INDEX_OFFSET
        + u32::from(func_env.offsets.size_of_vmshared_type_index());
    let null_trap = if !val_ty.nullable || func_env.is_proven(val, Proven::NonNull) {
        None
    } else if implicit_null_check(func_env, header_end) {
        Some(crate::TRAP_CAST_FAILURE)
    } else {
        let is_null = func_env.translate_ref_is_null(builder.cursor(), val, val_ty)?;
        func_env.trapnz(builder, is_null, crate::TRAP_CAST_FAILURE);
        None
    };
    let val_might_be_i31 = matches!(
        val_ty.heap_type,
        WasmHeapType::Any | WasmHeapType::Eq | WasmHeapType::I31
    );
    if cast_ty.heap_type.top() == WasmHeapTopType::Any && val_might_be_i31 {
        let is_i31 = builder
            .ins()
            .band_imm_u(val, i64::from(wasmtime_environ::I31_DISCRIMINANT));
        func_env.trapnz(builder, is_i31, crate::TRAP_CAST_FAILURE);
    }
    let expected_shared_ty =
        func_env.module_interned_to_shared_ty(&mut builder.cursor(), expected_interned_ty);
    let ty_addr = func_env.prepare_gc_ref_access(
        builder,
        val,
        BoundsCheck::StaticOffset {
            offset: wasmtime_environ::VM_GC_HEADER_TYPE_INDEX_OFFSET,
            access_size: func_env.offsets.size_of_vmshared_type_index(),
        },
    );
    let mut header_flags = func_env.gc_memflags(&mut builder.func).with_readonly();
    if let Some(code) = null_trap {
        header_flags = header_flags.with_trap_code(Some(code));
    }
    let actual_shared_ty = builder
        .ins()
        .load(ir::types::I32, header_flags, ty_addr, 0);
    let cast_okay = func_env.is_subtype(
        builder,
        actual_shared_ty,
        expected_shared_ty,
        expected_interned_ty,
    );
    func_env.trapz(builder, cast_okay, crate::TRAP_CAST_FAILURE);
    prove_cast(func_env, val, cast_ty);
    Ok(())
}

/// Record that the code just emitted is passed only by a `value` that is a
/// `ty`.
pub fn prove_cast(func_env: &mut FuncEnvironment<'_>, value: ir::Value, ty: WasmRefType) {
    // Null is a `(ref null $t)` of every type.
    if ty.nullable {
        return;
    }
    func_env.prove(value, Proven::NonNull);
    if let WasmHeapType::ConcreteArray(concrete)
    | WasmHeapType::ConcreteStruct(concrete)
    | WasmHeapType::ConcreteExn(concrete) = ty.heap_type
    {
        func_env.prove(value, Proven::Cast(concrete.unwrap_module_type_index()));
    }
}

/// Record what code that is entered only when `condition` is not zero
/// (`nonzero`), or only when it is zero, knows of the references that
/// `condition` was computed from (see [`ProvenReferences`]):
///
/// * `ref.is_null r` is zero exactly when `r` is not null;
/// * the latest `ref.test` is not zero exactly when its operand is of the
///   tested type;
/// * `i32.eqz x` is not zero exactly when `x` is zero;
/// * `i32.and a b` (a `band`) is not zero only when neither operand is, and
///   `i32.or a b` (a `bor`) is zero only when both operands are.
///
/// The emitter's `tag == X & ref.test T v` guards, `ref.is_null` and its
/// negation as an `if` or `br_if` condition, and disjunctions of null tests
/// are of these shapes. Each step holds of the values for every bit
/// pattern, so a fact found this way holds wherever control can only be with
/// `condition` zero or not zero, which is where the caller records it.
pub fn prove_condition(
    func_env: &mut FuncEnvironment<'_>,
    dfg: &ir::DataFlowGraph,
    condition: ir::Value,
    nonzero: bool,
) {
    prove_condition_within(func_env, dfg, condition, nonzero, 8);
}

/// [`prove_condition`], following at most `depth` operations.
fn prove_condition_within(
    func_env: &mut FuncEnvironment<'_>,
    dfg: &ir::DataFlowGraph,
    condition: ir::Value,
    nonzero: bool,
    depth: u32,
) {
    let condition = dfg.resolve_aliases(condition);
    if nonzero {
        if let Some((result, value, ty)) = func_env.proven_references.tested {
            if dfg.resolve_aliases(result) == condition {
                prove_cast(func_env, value, ty);
            }
        }
    }
    match func_env.proven_references.truths.get(&condition).copied() {
        Some(Truth::IsNull(reference)) => {
            if !nonzero {
                func_env.prove(reference, Proven::NonNull);
            }
            return;
        }
        Some(Truth::Eqz(operand)) => {
            if depth > 0 {
                prove_condition_within(func_env, dfg, operand, !nonzero, depth - 1);
            }
            return;
        }
        None => {}
    }
    if depth == 0 {
        return;
    }
    let ir::ValueDef::Result(inst, _) = dfg.value_def(condition) else {
        return;
    };
    let ir::InstructionData::Binary { opcode, args } = dfg.insts[inst] else {
        return;
    };
    // A conjunction that is not zero, or a disjunction that is zero, says
    // the same of each operand.
    if (opcode == ir::Opcode::Band && nonzero) || (opcode == ir::Opcode::Bor && !nonzero) {
        for arg in args {
            prove_condition_within(func_env, dfg, arg, nonzero, depth - 1);
        }
    }
}

/// Record that `result` is `ref.is_null` of `reference`, whose static type
/// is `ty`. A non-nullable reference's test is the constant zero, which
/// tells nothing.
pub fn record_ref_is_null(
    func_env: &mut FuncEnvironment<'_>,
    result: ir::Value,
    reference: ir::Value,
    ty: WasmRefType,
) {
    if ty.nullable {
        func_env
            .proven_references
            .truths
            .insert(result, Truth::IsNull(reference));
    }
}

/// Record that `result` is `i32.eqz` of `operand`.
pub fn record_eqz(func_env: &mut FuncEnvironment<'_>, result: ir::Value, operand: ir::Value) {
    func_env
        .proven_references
        .truths
        .insert(result, Truth::Eqz(operand));
}

/// Translate `ref.as_non_null` of `val`, whose static type is `val_ty`,
/// unless the function's code has already checked the same value at a place
/// that every path to this one passes (see [`ProvenReferences`]).
pub fn translate_ref_as_non_null(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    val: ir::Value,
    val_ty: WasmRefType,
) -> WasmResult<()> {
    if func_env.is_proven(val, Proven::NonNull) {
        return Ok(());
    }
    let check = DeferredNullCheck {
        value: val,
        ty: val_ty,
        srcloc: builder.srcloc(),
    };
    if may_defer_null_check(func_env, val_ty) {
        // The previous one was settled before this operator.
        debug_assert!(func_env.proven_references.deferred.is_none());
        func_env.proven_references.deferred = Some(check);
        return Ok(());
    }
    check.emit(func_env, builder);
    func_env.prove(val, Proven::NonNull);
    Ok(())
}

/// Scriptw patch: the check of a `ref.as_non_null` that has not been
/// translated yet.
///
/// The emitter reads a nullable local, applies `ref.as_non_null`, and
/// accesses a field of the object, often with only other locals and
/// constants read or written in between. Through the copying heap's null
/// guard the access is its own null check (see `implicit_null_check`), so
/// the explicit check is then all that `ref.as_non_null` costs. Its
/// translation is therefore deferred while the following operators only
/// read or write locals or push constants (`local.get`, `local.set`,
/// `local.tee`, `*.const`), and made by the next operator: an access through
/// the same value (`struct.get*`, `struct.set`, `array.len`, `array.get*`,
/// `array.set` with it as the object operand) makes it as its own null
/// check, with `ref.as_non_null`'s trap code, `TRAP_NULL_REFERENCE`, which
/// is also that access's own; any other operator first emits it as
/// `ref.as_non_null` would have (see [`settle_deferred_null_check`]).
///
/// # Why this is correct
///
/// The operators in between have no effect that outlives a trap and cannot
/// trap themselves: a trap ends the function's activation, and with it its
/// locals and operand stack, so a null operand traps with the same code
/// before anything observable happens, exactly as it did. A non-null
/// operand passes either check. The access through the value makes the
/// check before anything else it does (`emit_null_check` comes first in
/// each), and an access whose null check cannot be the access itself emits
/// the explicit check there. Only the trap's recorded source location can
/// move, to the access, when the access is the check; Wasm semantics do
/// not depend on it. Nothing is deferred when fuel is consumed or the
/// function keeps debugging state, whose counts and per-operator state a
/// trap would expose, and the fact that the value is not null is recorded
/// only where the check is made.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DeferredNullCheck {
    /// The operand of `ref.as_non_null`.
    value: ir::Value,
    /// Its static type.
    ty: WasmRefType,
    /// Where the `ref.as_non_null` is.
    srcloc: ir::SourceLoc,
}

impl DeferredNullCheck {
    /// Emit the check as `ref.as_non_null` emits it, attributed to the
    /// `ref.as_non_null`.
    fn emit(self, func_env: &mut FuncEnvironment<'_>, builder: &mut FunctionBuilder<'_>) {
        let srcloc = builder.srcloc();
        builder.set_srcloc(self.srcloc);
        let is_null = func_env
            .translate_ref_is_null(builder.cursor(), self.value, self.ty)
            .expect("a GC reference has a null test");
        func_env.trapnz(builder, is_null, crate::TRAP_NULL_REFERENCE);
        builder.set_srcloc(srcloc);
    }
}

/// Whether `ref.as_non_null` of a `ty` may leave its check to a following
/// access (see [`DeferredNullCheck`]).
fn may_defer_null_check(func_env: &mut FuncEnvironment<'_>, ty: WasmRefType) -> bool {
    ty.nullable
        && ty.heap_type.top() == WasmHeapTopType::Any
        && !func_env.tunables.consume_fuel
        && !func_env.tunables.debug_guest
        && func_env.state_slot.is_none()
        && implicit_null_check(func_env, 1)
}

/// Before the reachable operator `op` is translated: keep a deferred
/// `ref.as_non_null` check for it, or emit it first (see
/// [`DeferredNullCheck`]). `true` when `op` is the access that makes the
/// check.
pub fn settle_deferred_null_check(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    op: &wasmparser::Operator<'_>,
) -> bool {
    use wasmparser::Operator;
    let Some(deferred) = func_env.proven_references.deferred else {
        return false;
    };
    let stack = &func_env.stacks.stack;
    // The value `depth` places below the top of the operand stack.
    let operand = |depth: usize| {
        stack
            .len()
            .checked_sub(depth + 1)
            .map(|index| stack[index])
    };
    let object = match op {
        Operator::LocalGet { .. }
        | Operator::LocalSet { .. }
        | Operator::LocalTee { .. }
        | Operator::I32Const { .. }
        | Operator::I64Const { .. }
        | Operator::F32Const { .. }
        | Operator::F64Const { .. } => return false,
        Operator::StructGet { .. }
        | Operator::StructGetS { .. }
        | Operator::StructGetU { .. }
        | Operator::ArrayLen => operand(0),
        Operator::StructSet { .. }
        | Operator::ArrayGet { .. }
        | Operator::ArrayGetS { .. }
        | Operator::ArrayGetU { .. } => operand(1),
        Operator::ArraySet { .. } => operand(2),
        _ => None,
    };
    if object == Some(deferred.value) {
        return true;
    }
    flush_deferred_null_check(func_env, builder);
    false
}

/// Emit a deferred `ref.as_non_null` check, if there is one, where it is
/// due.
pub fn flush_deferred_null_check(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
) {
    if let Some(deferred) = func_env.proven_references.deferred.take() {
        deferred.emit(func_env, builder);
        func_env.prove(deferred.value, Proven::NonNull);
    }
}

/// After an operator that `settle_deferred_null_check` found to make a
/// deferred check: it did.
pub fn assert_deferred_null_check_made(func_env: &FuncEnvironment<'_>) {
    assert!(
        func_env.proven_references.deferred.is_none(),
        "an access through the operand of a deferred `ref.as_non_null` makes its check"
    );
}

/// What a truth value among the function's values is (see
/// [`prove_condition`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Truth {
    /// `ref.is_null` of this nullable reference: not zero exactly when the
    /// reference is null.
    IsNull(ir::Value),
    /// `i32.eqz` of this value: not zero exactly when the value is zero.
    Eqz(ir::Value),
}

/// A property that a check established of a reference value.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Proven {
    /// The reference is not null.
    NonNull,
    /// The reference is a non-null object whose type is a subtype of this
    /// concrete type.
    Cast(ModuleInternedTypeIndex),
}

/// Where a check was translated: the innermost Wasm control frame around it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ProvenAt {
    /// The control stack's height there.
    depth: usize,
    /// The block that follows the innermost frame, which names the frame:
    /// each frame creates its own.
    following: ir::Block,
    /// Whether that frame was an `if` in its `else` arm.
    in_else: bool,
}

/// The checks of GC references that the function's translated code has made,
/// so that a later check of the same value that an earlier one implies is
/// not translated again.
///
/// The emitter's Wasm reads a nullable local and applies `ref.as_non_null`
/// or `ref.cast` at each use. Cranelift merges two such checks only when
/// they test one SSA value, and after a call a GC reference is a value read
/// from the frame again; a cast is also several blocks. Here the checks are
/// compared as translated, when the operand is still the one CLIF value
/// that the Wasm local or stack slot holds.
///
/// # Why this is correct
///
/// A fact is recorded for a CLIF value `v` at a point `P` of the Wasm
/// function that control passes only when the fact holds: after the
/// instructions that trap unless it does, on the path a `br_on_null` or
/// `br_on_cast_fail` falls through to, or at the start of a path that only a
/// branch on a truth value computed from `v` enters, with the outcome that
/// implies the fact (see [`prove_condition`]): the consequent of an `if`
/// (its condition is not zero), the alternative of an `if` and the path a
/// `br_if` falls through to (the condition is zero). The alternative's
/// block has the `if`'s head as its only predecessor, and a `br_if`'s
/// fall-through block the `br_if`. It is recorded together
/// with `P`'s innermost control frame `F` and, for an `if`, its arm. It is
/// used at a later point `Q` of the translation only while `F` is still on
/// the control stack in the same arm.
///
/// * `P` is executed before `Q` on every path. Within one arm of one frame,
///   Wasm's structured control flow enters code after `P` only by falling
///   through `P`, by a branch to the end of a block that starts after `P`,
///   or by a branch to the head of a loop: one that starts after `P`, or one
///   around `P`, whose head is before `P`. A branch that skips `P` goes to
///   the end of a block around `P`, which is the end of `F` or of a frame
///   around it, and `Q` is before that end. Exception handlers of
///   `try_table` are such branches. The other arm of an `if` is not reached
///   through `P`, which is why the arm is part of the frame's name. Code
///   that the translator knows to be unreachable is not translated and
///   records nothing.
/// * `v` is an SSA value whose definition dominates `P`, and `P` dominates
///   `Q`, so at `Q` it is the value that the check at `P` saw: had its
///   definition run again since, control would have passed `P` again too.
/// * What was checked cannot change. A reference is null or not for good,
///   and an object's type is fixed when it is allocated; a collection that
///   moves the object keeps both, and the reference the function then reads
///   from its frame is the same Wasm value.
///
/// So the check at `Q` cannot fail, and leaving it out removes no trap.
#[derive(Default)]
pub(crate) struct ProvenReferences {
    facts: std::collections::HashMap<(ir::Value, Proven), ProvenAt>,
    /// The result of the latest `ref.test`, its operand and the tested type:
    /// the result is not zero exactly when the operand is of that type, and
    /// both are values, so that stays true wherever the result is in scope.
    tested: Option<(ir::Value, ir::Value, WasmRefType)>,
    /// The results of `ref.is_null` and `i32.eqz` so far, with their
    /// operands. Each is a pure function of its operand, so what it says
    /// holds wherever it is in scope.
    truths: std::collections::HashMap<ir::Value, Truth>,
    /// A `ref.as_non_null` check left to the next operator (see
    /// [`DeferredNullCheck`]).
    deferred: Option<DeferredNullCheck>,
}

impl FuncEnvironment<'_> {
    /// The innermost control frame at the translator's position.
    fn proven_at(&self) -> Option<ProvenAt> {
        let frame = self.stacks.control_stack.last()?;
        Some(ProvenAt {
            depth: self.stacks.control_stack.len(),
            following: frame.following_code(),
            in_else: frame.in_else_arm(),
        })
    }

    /// Record that the code just emitted traps unless `fact` holds of
    /// `value`.
    pub(crate) fn prove(&mut self, value: ir::Value, fact: Proven) {
        if let Some(at) = self.proven_at() {
            self.proven_references.facts.insert((value, fact), at);
        }
    }

    /// Whether every path to the translator's position has checked `fact`
    /// of `value`.
    pub(crate) fn is_proven(&self, value: ir::Value, fact: Proven) -> bool {
        let Some(at) = self.proven_references.facts.get(&(value, fact)) else {
            return false;
        };
        // The frame the fact was recorded in is still open, in the same arm.
        let Some(frame) = at
            .depth
            .checked_sub(1)
            .and_then(|index| self.stacks.control_stack.get(index))
        else {
            return false;
        };
        frame.following_code() == at.following && at.in_else == frame.in_else_arm()
    }
}

/// Whether a GC reference type uses the immediate i31 representation.
/// Callers handle (ref i31) and non-GC types separately.
fn gc_heap_type_might_be_i31(heap_type: WasmHeapType) -> bool {
    match heap_type {
        // If we are definitely dealing with an i31, we shouldn't be
        // emitting dynamic checks for it, and the caller shouldn't call
        // this function. Should have been caught by the assertion at the
        // start of the function.
        WasmHeapType::I31 => unreachable!(),

        // Could potentially be an i31.
        WasmHeapType::Any | WasmHeapType::Eq => true,

        // If it is definitely a struct, array, or uninhabited type, then it
        // is definitely not an i31.
        WasmHeapType::Array
        | WasmHeapType::ConcreteArray(_)
        | WasmHeapType::Struct
        | WasmHeapType::ConcreteStruct(_)
        | WasmHeapType::None => false,

        // Despite being a different type hierarchy, this *could* be an
        // `i31` if it is the result of
        //
        //     (extern.convert_any (ref.i31 ...))
        WasmHeapType::Extern => true,

        // Can only ever be `null`.
        WasmHeapType::NoExtern => false,

        WasmHeapType::Exn | WasmHeapType::ConcreteExn(_) | WasmHeapType::NoExn => false,

        // Wrong type hierarchy, and also funcrefs are not GC-managed
        // types. Should have been caught by the assertion at the start of
        // the function.
        WasmHeapType::Func | WasmHeapType::ConcreteFunc(_) | WasmHeapType::NoFunc => {
            unreachable!()
        }
        WasmHeapType::Cont | WasmHeapType::ConcreteCont(_) | WasmHeapType::NoCont => {
            unreachable!()
        }
    }
}

fn uextend_i32_to_pointer_type(
    builder: &mut FunctionBuilder,
    pointer_type: ir::Type,
    value: ir::Value,
) -> ir::Value {
    assert_eq!(builder.func.dfg.value_type(value), ir::types::I32);
    match pointer_type {
        ir::types::I32 => value,
        ir::types::I64 => builder.ins().uextend(ir::types::I64, value),
        _ => unreachable!(),
    }
}

/// Emit CLIF to compute an array object's total size, given the dynamic length
/// in its initialization.
///
/// Traps if the size overflows.
fn emit_array_size(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    array_layout: &GcArrayLayout,
    len: ir::Value,
) -> ir::Value {
    debug_assert_eq!(builder.func.dfg.value_type(len), ir::types::I32);
    if let Some(size) = FuncEnvironment::value_as_const_int(builder, len)
        .and_then(|len| array_layout.size_for_len(len as u32))
    {
        return builder.ins().iconst(ir::types::I32, i64::from(size));
    }

    let base_size = builder
        .ins()
        .iconst(ir::types::I32, i64::from(array_layout.base_size));

    // `elems_size = len * elem_size`
    //
    // Check for multiplication overflow and trap if it occurs, since that
    // means Wasm is attempting to allocate an array that is larger than our
    // implementation limits. (Note: there is no standard implementation
    // limit for array length beyond `u32::MAX`.)
    //
    // We implement this check by encoding our logically-32-bit operands as
    // i64 values, doing a 64-bit multiplication, and then checking the high
    // 32 bits of the multiplication's result. If the high 32 bits are not
    // all zeros, then the multiplication overflowed.
    let len = builder.ins().uextend(ir::types::I64, len);
    let elems_size_64 = builder
        .ins()
        .imul_imm_s(len, i64::from(array_layout.elem_size));
    let high_bits = builder.ins().ushr_imm_u(elems_size_64, 32);
    func_env.trapnz(builder, high_bits, crate::TRAP_ALLOCATION_TOO_LARGE);
    let elems_size = builder.ins().ireduce(ir::types::I32, elems_size_64);

    // And if adding the base size and elements size overflows, then the
    // allocation is too large.
    let size = func_env.uadd_overflow_trap(
        builder,
        base_size,
        elems_size,
        crate::TRAP_ALLOCATION_TOO_LARGE,
    );

    size
}

/// Common helper for struct-field initialization that can be reused across
/// collectors.
fn initialize_struct_fields(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    struct_ty: ModuleInternedTypeIndex,
    raw_ptr_to_struct: ir::Value,
    field_values: &[ir::Value],
) -> WasmResult<()> {
    let struct_layout = func_env.struct_or_exn_layout(struct_ty);
    let struct_size = struct_layout.size;
    let field_offsets: SmallVec<[_; 8]> = struct_layout.fields.iter().map(|f| f.offset).collect();
    assert_eq!(field_offsets.len(), field_values.len());

    assert!(!func_env.types[struct_ty].composite_type.shared);
    let fields = match &func_env.types[struct_ty].composite_type.inner {
        WasmCompositeInnerType::Struct(s) => &s.fields,
        WasmCompositeInnerType::Exn(e) => &e.fields,
        _ => panic!("Not a struct or exception type"),
    };

    let field_types: SmallVec<[_; 8]> = fields.iter().cloned().collect();
    assert_eq!(field_types.len(), field_values.len());

    for ((ty, val), offset) in field_types.into_iter().zip(field_values).zip(field_offsets) {
        let size_of_access = wasmtime_environ::byte_size_of_wasm_ty_in_gc_heap(&ty.element_type);
        assert!(offset + size_of_access <= struct_size);
        let field_addr = builder
            .ins()
            .iadd_imm_s(raw_ptr_to_struct, i64::from(offset));
        gc_compiler(func_env)?.init_field(func_env, builder, ty.element_type, field_addr, *val)?;
    }

    Ok(())
}

impl FuncEnvironment<'_> {
    /// Flags to use for general-purpose GC loads/stores.
    ///
    /// This is used for accesses to the GC heap which aren't expected to trap, but
    /// retain internal assertion metadata to report if such a trap happens. This
    /// is here to ensure that in the face of heap corruption that there's no
    /// possible UB within Cranelift and/or the runtime.
    pub(super) fn gc_memflags(&mut self, func: &mut ir::Function) -> ir::MemFlagsData {
        let region = self.alias_regions.gc_heap_region(func);
        ir::MemFlagsData::new()
            .with_trap_code(Some(crate::TRAP_GC_HEAP_CORRUPT))
            .with_alias_region(Some(region))
    }

    fn gc_layout(&mut self, type_index: ModuleInternedTypeIndex) -> &GcLayout {
        // Lazily compute and cache the layout.
        if !self.ty_to_gc_layout.contains_key(&type_index) {
            let ty = &self.types[type_index].composite_type;
            let layout = gc_compiler(self)
                .unwrap()
                .layouts()
                .gc_layout(ty)
                .expect("gc_layout should not OOM at compile time")
                .expect("should only call `FuncEnvironment::gc_layout` for GC types");
            self.ty_to_gc_layout.insert(type_index, layout);
        }

        self.ty_to_gc_layout.get(&type_index).unwrap()
    }

    /// Get the `GcArrayLayout` for the array type at the given `type_index`.
    pub(crate) fn array_layout(
        &mut self,
        type_index: ModuleInternedTypeIndex,
    ) -> WasmResult<&GcArrayLayout> {
        Ok(self.gc_layout(type_index).unwrap_array())
    }

    /// Get the `GcStructLayout` for the struct or exception type at the given `type_index`.
    fn struct_or_exn_layout(&mut self, type_index: ModuleInternedTypeIndex) -> &GcStructLayout {
        let result = self.gc_layout(type_index).unwrap_struct();
        result
    }

    /// Whether the GC heap's base pointer can change while a function runs.
    fn gc_heap_may_move(&self) -> bool {
        let memory_tunables =
            wasmtime_environ::MemoryTunables::new(self.tunables, MemoryKind::GcHeap);
        self.tunables
            .gc_heap_memory_type()
            .memory_may_move(&memory_tunables)
    }

    /// Get the GC heap's base.
    pub(crate) fn get_gc_heap_base(
        &mut self,
        builder: &mut FunctionBuilder,
    ) -> WasmResult<ir::Value> {
        Ok(self.gc_heap_base(builder))
    }

    /// Get the GC heap's base at the builder's position.
    ///
    /// A base that cannot move is read from the function's home slot for it,
    /// which this creates on first use; `place_frame_loads` then decides
    /// where the function really loads it. A base that can move is
    /// loaded from the store context every time.
    pub(crate) fn gc_heap_base(&mut self, builder: &mut FunctionBuilder) -> ir::Value {
        if self.gc_heap_may_move() {
            let store_context_ptr = self.get_vmstore_context_ptr(builder);
            return self.alias_regions.vmstore_context_gc_heap_base(
                &mut builder.cursor(),
                ir::MemFlagsData::trusted(),
                store_context_ptr,
            );
        }
        let home = match self.gc_heap_base_home {
            Some(home) => home,
            None => {
                let home = self.create_gc_heap_base_home(builder.func);
                self.gc_heap_base_home = Some(home);
                home
            }
        };
        home.load(&mut builder.cursor())
    }

    /// Load the immovable GC heap base at the top of `func`'s entry block and
    /// store it to a new home slot there, ahead of every use.
    fn create_gc_heap_base_home(&mut self, func: &mut ir::Function) -> FrameHome {
        let entry = func
            .layout
            .entry_block()
            .expect("the entry block is inserted before any translation");
        let vmctx = func
            .special_param(ir::ArgumentPurpose::VMContext)
            .expect("Missing vmctx parameter");
        let region = self.alias_regions.gc_heap_base_home_region(func);
        let pointer_type = self.pointer_type();
        let mut pos = FuncCursor::new(func).at_first_insertion_point(entry);
        let store_context_ptr = self.alias_regions.vmctx_store_context(&mut pos, vmctx);
        // The base pointer never changes, so its load is `readonly` and can
        // move.
        let mut flags = ir::MemFlagsData::trusted();
        flags.set_readonly();
        flags.set_can_move();
        let base =
            self.alias_regions
                .vmstore_context_gc_heap_base(&mut pos, flags, store_context_ptr);
        FrameHome::store_at_entry(&mut pos, region, pointer_type, base)
    }

    /// After `func` is completely translated and finalized: send each edge
    /// into a block that only branches on a truth value where that value
    /// sends it (`branch_threading`), give the `vmctx` its home
    /// (`base_home`), and keep, of the loads of GC references from their
    /// stack-map slots and of the GC heap base and the `vmctx` from their
    /// homes, one after a call instead of one at every use
    /// (`frame_reloads`). Last, mark the blocks that only unlikely code
    /// reaches (`cold_paths`).
    pub(crate) fn place_frame_loads(&mut self, func: &mut ir::Function) {
        // This changes branches, so it comes before the control-flow graph.
        super::branch_threading::thread(func);
        let base_home = self.gc_heap_base_home.take();
        let type_ids_home = self.type_ids_home.take();
        let vmctx_home = func
            .special_param(ir::ArgumentPurpose::VMContext)
            .and_then(|vmctx| {
                let region = self.alias_regions.vmctx_home_region(func);
                FrameHome::of_parameter(func, region, vmctx)
            });
        let homes: SmallVec<[ir::StackSlot; 3]> = [vmctx_home, base_home, type_ids_home]
            .iter()
            .flatten()
            .map(|home| home.slot())
            .collect();
        if let Some(reloads) = frame_reloads::Reloads::scan(func, &homes) {
            let cfg = ControlFlowGraph::with_function(func);
            let domtree = DominatorTree::with_function(func, &cfg);
            reloads.forward(func, &cfg, &domtree);
        }
        // The passes above change edges and add blocks; which blocks only
        // unlikely code reaches is decided on the function they leave.
        super::cold_paths::spread(func);
    }

    /// Get the GC heap's bound.
    pub(crate) fn get_gc_heap_bound(
        &mut self,
        builder: &mut FunctionBuilder,
    ) -> WasmResult<ir::Value> {
        let store_context_ptr = self.get_vmstore_context_ptr(builder);
        Ok(self
            .alias_regions
            .vmstore_context_gc_heap_current_length(&mut builder.cursor(), store_context_ptr))
    }

    /// Get or create the `Heap` for our GC heap.
    fn get_gc_heap(&mut self, func: &mut ir::Function) -> Heap {
        if let Some(heap) = self.gc_heap {
            return heap;
        }

        let store_ctx = self.alias_regions.vmctx_store_context_load(func);

        // The base pointer's load is `can_move` and `readonly` when the GC
        // heap's base can never move. `bounds_checks::compute_addr` does not
        // emit this chain for the GC heap: it asks `gc_heap_base`, which
        // reads an immovable base from its home slot.
        let mut base_flags = ir::MemFlagsData::trusted();
        if !self.gc_heap_may_move() {
            base_flags.set_readonly();
            base_flags.set_can_move();
        }
        let base = self
            .alias_regions
            .vmstore_context_gc_heap_base_load(func, base_flags);

        let bound = self
            .alias_regions
            .vmstore_context_gc_heap_current_length_load(func);

        let memory = self.tunables.gc_heap_memory_type();
        let heap = self.heaps.push(HeapData {
            base: VmctxLoadChain::new(smallvec![store_ctx, base]),
            bound: VmctxLoadChain::new(smallvec![store_ctx, bound]),
            memory,
            kind: MemoryKind::GcHeap,
        });
        self.gc_heap = Some(heap);
        heap
    }

    /// Get the raw pointer of `gc_ref[offset]` bounds checked for an access of
    /// `size` bytes.
    ///
    /// The given `gc_ref` must be a non-null, non-i31 GC reference.
    ///
    /// If `check` is a `BoundsCheck::Object`, then it is the callers
    /// responsibility to ensure that `offset + access_size <= object_size`.
    ///
    /// Returns a raw pointer to `gc_ref[offset]` -- not a raw pointer to the GC
    /// object itself (unless `offset` happens to be `0`). This raw pointer may
    /// be used to read or write up to as many bytes as described by `bound`. Do
    /// NOT attempt accesses bytes outside of `bound`; that may lead to
    /// unchecked out-of-bounds accesses.
    ///
    /// This method is collector-agnostic.
    fn prepare_gc_ref_access(
        &mut self,
        builder: &mut FunctionBuilder,
        gc_ref: ir::Value,
        bounds_check: BoundsCheck,
    ) -> ir::Value {
        log::trace!("prepare_gc_ref_access({gc_ref:?}, {bounds_check:?})");
        assert_eq!(builder.func.dfg.value_type(gc_ref), ir::types::I32);

        let gc_heap = self.get_gc_heap(&mut builder.func);
        let gc_heap = self.heaps[gc_heap].clone();
        let result = match crate::bounds_checks::bounds_check_and_compute_addr(
            builder,
            self,
            &gc_heap,
            gc_ref,
            bounds_check,
            crate::TRAP_GC_HEAP_CORRUPT,
        ) {
            Reachability::Reachable(v) => v,
            Reachability::Unreachable => {
                // We are now in unreachable code, but we don't want to plumb
                // through a bunch of `Reachability` through all of our callers,
                // so just assert we won't reach here and return `null`
                let null = builder.ins().iconst(self.pointer_type(), 0);
                builder.ins().trapz(null, crate::TRAP_INTERNAL_ASSERT);
                null
            }
        };
        log::trace!("prepare_gc_ref_access(..) -> {result:?}");
        result
    }

    /// Emit checks (if necessary) for whether the given `gc_ref` is null or is
    /// an `i31ref`.
    ///
    /// Takes advantage of static information based on `ty` as to whether the GC
    /// reference is nullable or can ever be an `i31`.
    ///
    /// Returns an `ir::Value` that is an `i32` will be non-zero if the GC
    /// reference is null or is an `i31ref`; otherwise, it will be zero.
    ///
    /// This method is collector-agnostic.
    fn gc_ref_is_null_or_i31(
        &mut self,
        builder: &mut FunctionBuilder,
        ty: WasmRefType,
        gc_ref: ir::Value,
    ) -> ir::Value {
        assert_eq!(builder.func.dfg.value_type(gc_ref), ir::types::I32);
        assert!(ty.is_vmgcref_type_and_not_i31());

        let might_be_i31 = gc_heap_type_might_be_i31(ty.heap_type);

        match (ty.nullable, might_be_i31) {
            // This GC reference statically cannot be null nor an i31. (Let
            // Cranelift's optimizer const-propagate this value and erase any
            // unnecessary control flow resulting from branching on this value.)
            (false, false) => builder.ins().iconst(ir::types::I32, 0),

            // This GC reference is always non-null, but might be an i31.
            (false, true) => builder
                .ins()
                .band_imm_u(gc_ref, i64::from(I31_DISCRIMINANT)),

            // This GC reference might be null, but can never be an i31.
            (true, false) => builder.ins().icmp_imm_s(IntCC::Equal, gc_ref, 0),

            // Fully general case: this GC reference could be either null or an
            // i31.
            (true, true) => {
                let is_i31 = builder
                    .ins()
                    .band_imm_u(gc_ref, i64::from(I31_DISCRIMINANT));
                let is_null = builder.ins().icmp_imm_s(IntCC::Equal, gc_ref, 0);
                let is_null = builder.ins().uextend(ir::types::I32, is_null);
                builder.ins().bor(is_i31, is_null)
            }
        }
    }

    /// Emit code to check whether `a <: b` for two `VMSharedTypeIndex`es.
    pub(crate) fn is_subtype(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        a: ir::Value,
        b: ir::Value,
        b_ty: ModuleInternedTypeIndex,
    ) -> ir::Value {
        log::trace!("is_subtype({a:?}, {b:?})");

        // Current block: fast path for when `a == b`.
        log::trace!("is_subtype: fast path check for exact same types");
        let same_ty = builder.ins().icmp(IntCC::Equal, a, b);
        let same_ty = builder.ins().uextend(ir::types::I32, same_ty);

        // When `b` is final the equality check above is already a complete
        // subtype check, so there is nothing more to do: a final type cannot be
        // the supertype of any other type, so `a <: b` holds if and only if `a
        // == b`; in that case we can avoid emitting the slow-path `is_subtype`
        // libcall and its control flow entirely (the slow path would only ever
        // return `false` here anyway).
        let b_is_final = self.types[b_ty].is_final;
        if b_is_final {
            return same_ty;
        }

        let diff_tys_block = builder.create_block();
        let continue_block = builder.create_block();

        builder.ins().brif(
            same_ty,
            continue_block,
            &[same_ty.into()],
            diff_tys_block,
            &[],
        );

        // Different types block: fall back to the `is_subtype` libcall,
        // out of line.
        builder.switch_to_block(diff_tys_block);
        builder.set_cold_block(diff_tys_block);
        log::trace!("is_subtype: slow path to do full `is_subtype` libcall");
        let is_subtype = self.builtin_functions.is_subtype(builder.func);
        let vmctx = self.vmctx_val(&mut builder.cursor());
        let call_inst = builder.ins().call(is_subtype, &[vmctx, a, b]);
        let result = builder.func.dfg.first_result(call_inst);
        builder.ins().jump(continue_block, &[result.into()]);

        // Continue block: join point for the result.
        builder.switch_to_block(continue_block);
        let result = builder.append_block_param(continue_block, ir::types::I32);
        log::trace!("is_subtype(..) -> {result:?}");

        builder.seal_block(diff_tys_block);
        builder.seal_block(continue_block);

        result
    }
}

fn stack_switching_unsupported<T>() -> WasmResult<T> {
    Err(wasmtime_environ::WasmError::Unsupported(
        "Stack switching feature not compatible with GC, yet".to_string(),
    ))
}

pub fn translate_array_new_entity(
    env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder,
    array_type_index: TypeIndex,
    entity: CheckedEntity,
    entity_offset: ir::Value,
    len: ir::Value,
) -> WasmResult<ir::Value> {
    // Before actually allocating this array first do a bounds-check on the
    // passive entity itself.
    let interned_type_index = env.module.types[array_type_index].unwrap_module_type_index();
    env.translate_entity_bounds_check(builder, entity, entity_offset, len)?;

    let array = gc_compiler(env)?.alloc_uninit_array(env, builder, array_type_index, len)?;
    let dst = builder.ins().iconst(ir::types::I32, 0);
    env.translate_entity_copy(
        builder,
        CheckedEntity::Array {
            array,
            ty: interned_type_index,
            initialized: false,
        },
        entity,
        dst,
        entity_offset,
        len,
    )?;

    Ok(array)
}
