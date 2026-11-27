//! Compiler for the copying (semi-space/Cheney) collector.
//!
//! Allocation is performed inline with a bump pointer when possible, falling
//! back to the `gc_alloc_raw` libcall when the active semi-space is full.
//! Reads need no barriers. Stores into old objects record nursery edges when
//! the nursery is enabled. Stack maps let the collector relocate roots.

use super::*;
use crate::TRAP_INTERNAL_ASSERT;
use crate::func_environ::FuncEnvironment;
use crate::translate::TargetEnvironment;
use cranelift_codegen::ir::{self, InstBuilder};
use cranelift_frontend::FunctionBuilder;
use wasmtime_environ::copying::{
    ALIGN, EXCEPTION_TAG_DEFINED_OFFSET, EXCEPTION_TAG_INSTANCE_OFFSET, InlineTraceInfo,
};
use wasmtime_environ::{
    GcTypeLayouts, ModuleInternedTypeIndex, TypeIndex, VMGcKind, WasmHeapTopType, WasmHeapType,
    WasmRefType, WasmResult, WasmStorageType, WasmValType, copying::CopyingTypeLayouts,
};

#[derive(Default)]
pub struct CopyingCompiler {
    layouts: CopyingTypeLayouts,
}

/// Read the `VMFuncRef` that a GC heap field's interned funcref id names.
///
/// The runtime publishes the Store's interned funcref table in
/// `VMCopyingHeapData` (`crate::copying_heap_data`). An id below its length
/// reads the entry inline. A null entry is the null funcref; a non-null entry
/// whose registered type equals the expected engine type (or any entry, when
/// the field is an untyped `funcref`) is the result. Those are exactly the
/// results `get_interned_func_ref` returns without a subtype query. Anything
/// else, an id out of range or a type that differs, calls that libcall, which
/// keeps its complete subtype check and its traps.
pub(super) fn translate_read_interned_func_ref(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    func_ref_id: ir::Value,
    expected_ty: ir::Value,
    typed: bool,
) -> ir::Value {
    let pointer_type = func_env.pointer_type();
    let heap_data = CopyingCompiler::load_vmcopying_heap_data_ptr(func_env, builder);
    let len = func_env
        .alias_regions
        .vmcopying_heap_data_func_refs_len(&mut builder.cursor(), heap_data);

    let entry_block = builder.create_block();
    let slow_block = builder.create_block();
    let done_block = builder.create_block();
    let result = builder.append_block_param(done_block, pointer_type);

    let in_range = builder.ins().icmp(
        ir::condcodes::IntCC::UnsignedLessThan,
        func_ref_id,
        len,
    );
    builder.ins().brif(in_range, entry_block, &[], slow_block, &[]);

    builder.switch_to_block(entry_block);
    builder.seal_block(entry_block);
    let base = func_env
        .alias_regions
        .vmcopying_heap_data_func_refs(&mut builder.cursor(), heap_data);
    let index = if pointer_type == ir::types::I32 {
        func_ref_id
    } else {
        builder.ins().uextend(pointer_type, func_ref_id)
    };
    let offset = builder
        .ins()
        .imul_imm_u(index, i64::from(pointer_type.bytes()));
    let entry_addr = builder.ins().iadd(base, offset);
    let entry = builder
        .ins()
        .load(pointer_type, ir::MemFlagsData::trusted(), entry_addr, 0);
    if typed {
        let check_block = builder.create_block();
        builder
            .ins()
            .brif(entry, check_block, &[], done_block, &[entry.into()]);
        builder.switch_to_block(check_block);
        builder.seal_block(check_block);
        let actual_ty = func_env.alias_regions.vmfuncref_type_index(
            &mut builder.cursor(),
            ir::MemFlagsData::trusted().with_readonly(),
            entry,
        );
        let same = builder.ins().icmp(
            ir::condcodes::IntCC::Equal,
            actual_ty,
            expected_ty,
        );
        builder
            .ins()
            .brif(same, done_block, &[entry.into()], slow_block, &[]);
    } else {
        builder.ins().jump(done_block, &[entry.into()]);
    }

    builder.switch_to_block(slow_block);
    builder.seal_block(slow_block);
    // An id outside the published table or an entry of another type: the
    // libcall answers, out of line.
    builder.set_cold_block(slow_block);
    let vmctx = func_env.vmctx_val(&mut builder.cursor());
    let get_interned_func_ref = func_env
        .builtin_functions
        .get_interned_func_ref(builder.func);
    let call_inst = builder
        .ins()
        .call(get_interned_func_ref, &[vmctx, func_ref_id, expected_ty]);
    let slow_result = builder.func.dfg.first_result(call_inst);
    builder.ins().jump(done_block, &[slow_result.into()]);

    builder.switch_to_block(done_block);
    builder.seal_block(done_block);
    result
}

/// The copying heap's generational write barrier, for a reference `value`
/// stored into the GC heap slot at native address `slot`.
///
/// The nursery is the heap below `copying_heap_data::nursery_end`, so a
/// store into the old generation is one at or above the heap base plus that
/// end: one compare against a value hoisted per function, taken only by old
/// targets. Such a store of a nursery reference (a non-null, non-`i31` value
/// below the end) records the slot's heap index in the remembered-slot buffer
/// the runtime publishes, or flags the buffer's overflow. Fresh objects need
/// none: they are in the nursery, in the pretenuring window (the next
/// collection must be full), or registered for a whole-object scan by the
/// slow allocator. This exemption expires at the first potential safepoint.
pub(super) fn emit_old_to_young_barrier(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder,
    ty: WasmRefType,
    slot: ir::Value,
    value: ir::Value,
) -> WasmResult<()> {
    let nursery_end = crate::copying_heap_data::nursery_end(func_env.tunables.gc_heap_initial_size);
    if nursery_end == 0
        || !ty.heap_type.is_vmgcref_type_and_not_i31()
        || ty.heap_type.is_bottom()
        || FuncEnvironment::value_as_const_int(builder, value).is_some_and(|v| v as u32 == 0)
    {
        return Ok(());
    }
    use ir::condcodes::IntCC;
    let pointer_type = func_env.pointer_type();
    let pointer_bytes = pointer_type.bytes();
    let base = func_env.get_gc_heap_base(builder)?;
    let boundary = builder.ins().iadd_imm_u(base, i64::from(nursery_end));
    let is_old = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, slot, boundary);

    let old_block = builder.create_block();
    let record_block = builder.create_block();
    let push_block = builder.create_block();
    let overflow_block = builder.create_block();
    let continue_block = builder.create_block();
    for block in [old_block, record_block, push_block, overflow_block] {
        builder.set_cold_block(block);
    }
    builder.ins().brif(is_old, old_block, &[], continue_block, &[]);

    // An old slot: is the value a nursery object?
    builder.switch_to_block(old_block);
    builder.seal_block(old_block);
    let (index, limit) = if !ty.nullable || func_env.is_proven(value, Proven::NonNull) {
        (value, nursery_end)
    } else {
        // One unsigned comparison rejects both zero (which wraps) and old
        // space references.
        (builder.ins().iadd_imm_s(value, -1), nursery_end - 1)
    };
    let in_nursery = builder
        .ins()
        .icmp_imm_u(IntCC::UnsignedLessThan, index, i64::from(limit));
    let young = if gc_heap_type_might_be_i31(ty.heap_type) {
        let low_bit = builder.ins().band_imm_u(value, 1);
        let not_i31 = builder.ins().icmp_imm_u(IntCC::Equal, low_bit, 0);
        builder.ins().band(in_nursery, not_i31)
    } else {
        in_nursery
    };
    builder
        .ins()
        .brif(young, record_block, &[], continue_block, &[]);

    // Record the slot, or flag the overflow.
    builder.switch_to_block(record_block);
    builder.seal_block(record_block);
    let heap_data = CopyingCompiler::load_vmcopying_heap_data_ptr(func_env, builder);
    let len_offset = crate::copying_heap_data::remembered_len(pointer_bytes);
    let cap_offset = crate::copying_heap_data::remembered_cap(pointer_bytes);
    let overflow_offset = crate::copying_heap_data::remembered_overflow(pointer_bytes);
    let buffer_offset = crate::copying_heap_data::remembered(pointer_bytes);
    let field = |func_env: &mut FuncEnvironment<'_>, builder: &mut FunctionBuilder, offset| {
        func_env
            .alias_regions
            .vmcopying_heap_data_field_flags(&mut builder.func, offset)
    };
    let len_flags = field(func_env, builder, len_offset);
    let cap_flags = field(func_env, builder, cap_offset);
    let overflow_flags = field(func_env, builder, overflow_offset);
    let buffer_flags = field(func_env, builder, buffer_offset);
    let len = builder.ins().load(
        ir::types::I32,
        len_flags,
        heap_data,
        i32::try_from(len_offset).unwrap(),
    );
    let cap = builder.ins().load(
        ir::types::I32,
        cap_flags,
        heap_data,
        i32::try_from(cap_offset).unwrap(),
    );
    let has_room = builder.ins().icmp(IntCC::UnsignedLessThan, len, cap);
    builder
        .ins()
        .brif(has_room, push_block, &[], overflow_block, &[]);

    builder.switch_to_block(push_block);
    builder.seal_block(push_block);
    let buffer = builder.ins().load(
        pointer_type,
        buffer_flags,
        heap_data,
        i32::try_from(buffer_offset).unwrap(),
    );
    let index = builder.ins().isub(slot, base);
    let index = if pointer_type == ir::types::I32 {
        index
    } else {
        builder.ins().ireduce(ir::types::I32, index)
    };
    let entry = if pointer_type == ir::types::I32 {
        len
    } else {
        builder.ins().uextend(pointer_type, len)
    };
    let entry = builder.ins().ishl_imm_u(entry, 2);
    let entry = builder.ins().iadd(buffer, entry);
    let entry_flags = func_env
        .alias_regions
        .remembered_slots_flags(&mut builder.func);
    builder.ins().store(entry_flags, index, entry, 0);
    let next = builder.ins().iadd_imm_u(len, 1);
    builder
        .ins()
        .store(len_flags, next, heap_data, i32::try_from(len_offset).unwrap());
    builder.ins().jump(continue_block, &[]);

    builder.switch_to_block(overflow_block);
    builder.seal_block(overflow_block);
    let one = builder.ins().iconst(ir::types::I32, 1);
    builder.ins().store(
        overflow_flags,
        one,
        heap_data,
        i32::try_from(overflow_offset).unwrap(),
    );
    builder.ins().jump(continue_block, &[]);

    builder.switch_to_block(continue_block);
    builder.seal_block(continue_block);
    Ok(())
}

/// The id of a `VMFuncRef` in the Store's interned funcref table, for a
/// store into the GC heap.
///
/// The null funcref is id zero: the runtime's table interns it first. For
/// any other address the runtime keeps a direct-mapped cache of ids that
/// `intern_func_ref_for_gc_heap` has already returned, published in
/// `VMCopyingHeapData` (`crate::copying_heap_data`). An entry whose address
/// word equals this address holds that address's id, which never changes
/// while the table lives. Every other case, an empty entry or one that
/// another address took, calls the libcall, which interns and fills the
/// entry. The cache is host memory that only the host writes.
pub(super) fn translate_intern_func_ref(
    func_env: &mut FuncEnvironment<'_>,
    builder: &mut FunctionBuilder<'_>,
    func_ref: ir::Value,
) -> ir::Value {
    use crate::copying_heap_data::{
        FUNC_REF_CACHE_ENTRIES, func_ref_cache_entry_bytes, func_ref_cache_shift,
    };
    let pointer_type = func_env.pointer_type();
    let pointer_bytes = pointer_type.bytes();

    let lookup_block = builder.create_block();
    let hit_block = builder.create_block();
    let slow_block = builder.create_block();
    let done_block = builder.create_block();
    let result = builder.append_block_param(done_block, ir::types::I32);

    let null_id = builder.ins().iconst(ir::types::I32, 0);
    builder
        .ins()
        .brif(func_ref, lookup_block, &[], done_block, &[null_id.into()]);

    builder.switch_to_block(lookup_block);
    builder.seal_block(lookup_block);
    let heap_data = CopyingCompiler::load_vmcopying_heap_data_ptr(func_env, builder);
    let cache = func_env
        .alias_regions
        .vmcopying_heap_data_func_ref_cache(&mut builder.cursor(), heap_data);
    let shifted = builder
        .ins()
        .ushr_imm_u(func_ref, i64::from(func_ref_cache_shift(pointer_bytes)));
    let index = builder
        .ins()
        .band_imm_u(shifted, i64::from(FUNC_REF_CACHE_ENTRIES - 1));
    let offset = builder
        .ins()
        .imul_imm_u(index, i64::from(func_ref_cache_entry_bytes(pointer_bytes)));
    let entry = builder.ins().iadd(cache, offset);
    // The host writes these words between calls; plain trusted loads are
    // not carried across one.
    let address = builder
        .ins()
        .load(pointer_type, ir::MemFlagsData::trusted(), entry, 0);
    let hit = builder
        .ins()
        .icmp(ir::condcodes::IntCC::Equal, address, func_ref);
    builder.ins().brif(hit, hit_block, &[], slow_block, &[]);

    builder.switch_to_block(hit_block);
    builder.seal_block(hit_block);
    let cached = builder.ins().load(
        ir::types::I32,
        ir::MemFlagsData::trusted(),
        entry,
        i32::try_from(pointer_bytes).unwrap(),
    );
    builder.ins().jump(done_block, &[cached.into()]);

    builder.switch_to_block(slow_block);
    builder.seal_block(slow_block);
    let vmctx = func_env.vmctx_val(&mut builder.cursor());
    let intern_func_ref_for_gc_heap = func_env
        .builtin_functions
        .intern_func_ref_for_gc_heap(builder.func);
    let call_inst = builder
        .ins()
        .call(intern_func_ref_for_gc_heap, &[vmctx, func_ref]);
    let interned = builder.func.dfg.first_result(call_inst);
    let interned = builder.ins().ireduce(ir::types::I32, interned);
    builder.ins().jump(done_block, &[interned.into()]);

    builder.switch_to_block(done_block);
    builder.seal_block(done_block);
    result
}

impl CopyingCompiler {
    /// Load the pointer to the `VMCopyingHeapData` from vmctx.
    fn load_vmcopying_heap_data_ptr(
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder,
    ) -> ir::Value {
        let vmctx = func_env.vmctx_val(&mut builder.cursor());
        func_env
            .alias_regions
            .vmctx_gc_heap_data(&mut builder.cursor(), vmctx)
    }

    /// Load the current bump pointer and active-space end from a `*mut
    /// VMCopyingHeapData`.
    ///
    /// Returns `(bump_ptr, active_space_end)` as `i32` values.
    fn load_bump_state(
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder,
        ptr_to_heap_data: ir::Value,
    ) -> (ir::Value, ir::Value) {
        let bump_ptr = func_env
            .alias_regions
            .vmcopying_heap_data_bump_ptr(&mut builder.cursor(), ptr_to_heap_data);
        let active_space_end = func_env
            .alias_regions
            .vmcopying_heap_data_active_space_end(&mut builder.cursor(), ptr_to_heap_data);
        (bump_ptr, active_space_end)
    }

    /// Store the new bump pointer into the given `*mut VMCopyingHeapData`.
    fn store_bump_pointer(
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        heap_data_ptr: ir::Value,
        end_of_object: ir::Value,
    ) {
        func_env.alias_regions.store_vmcopying_heap_data_bump_ptr(
            &mut builder.cursor(),
            heap_data_ptr,
            end_of_object,
        );
    }

    /// Round `size` (an `i32`) up to `ALIGN`, returning the result as an `i64`.
    ///
    /// Uses `i64` arithmetic so that overflow produces a value larger than any
    /// valid heap index, which sends us to the slow allocation path instead of
    /// wrapping around.
    fn aligned_size(builder: &mut FunctionBuilder, size: ir::Value) -> ir::Value {
        let size_64 = builder.ins().uextend(ir::types::I64, size);
        let align_mask = builder.ins().iconst(ir::types::I64, i64::from(ALIGN - 1));
        let inv_align_mask = builder.ins().iconst(ir::types::I64, !i64::from(ALIGN - 1));
        let size_plus_mask = builder.ins().iadd(size_64, align_mask);
        builder.ins().band(size_plus_mask, inv_align_mask)
    }

    /// Emit inline bump allocation, falling back to `gc_alloc_raw` on failure.
    ///
    /// `size` must be an `i32` value >= `size_of(VMCopyingHeader)`.
    ///
    /// Returns `(gc_ref, raw_ptr_to_object)` where `gc_ref` is the `i32` GC
    /// heap index and `raw_ptr_to_object` is a native pointer into the GC heap.
    fn emit_inline_alloc(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder,
        kind: VMGcKind,
        ty: ModuleInternedTypeIndex,
        size: ir::Value,
        reserved_bits: u32,
    ) -> WasmResult<(ir::Value, ir::Value)> {
        debug_assert_ne!(kind, VMGcKind::ExternRef);
        debug_assert!(!ty.is_reserved_value());
        assert_eq!(builder.func.dfg.value_type(size), ir::types::I32);

        let pointer_type = func_env.pointer_type();
        let current_block = builder.current_block().unwrap();
        let fast_block = builder.create_block();
        let slow_block = builder.create_block();
        let merge_block = builder.create_block();

        builder.ensure_inserted_block();
        builder.insert_block_after(fast_block, current_block);
        builder.insert_block_after(slow_block, fast_block);
        builder.insert_block_after(merge_block, slow_block);

        let ptr_to_heap_data = Self::load_vmcopying_heap_data_ptr(func_env, builder);
        let (bump_ptr, active_space_end) =
            Self::load_bump_state(func_env, builder, ptr_to_heap_data);
        let constant_size = FuncEnvironment::value_as_const_int(builder, size)
            .and_then(|size| (size as u32).checked_next_multiple_of(ALIGN));

        let narrow_size = constant_size.filter(|_| pointer_type == ir::types::I32);
        let (fits, end_of_object, object_size) = if let Some(size) = narrow_size {
            // The runtime publishes bump <= limit and inline allocation
            // preserves it. Checking the remaining capacity first makes the
            // subsequent i32 addition non-wrapping on the fast edge.
            let remaining = builder.ins().isub(active_space_end, bump_ptr);
            let fits = builder.ins().icmp_imm_u(
                ir::condcodes::IntCC::UnsignedGreaterThanOrEqual,
                remaining,
                i64::from(size),
            );
            let end = builder.ins().iadd_imm_u(bump_ptr, i64::from(size));
            let size = builder.ins().iconst(ir::types::I32, i64::from(size));
            (fits, end, size)
        } else {
            // Compare the end we will store directly with the limit. Both
            // start and limit are unsigned 32-bit indices, so the wide add
            // also rules out overflow without a separate capacity subtraction.
            let aligned_size = match constant_size {
                Some(size) => builder.ins().iconst(ir::types::I64, i64::from(size)),
                None => Self::aligned_size(builder, size),
            };
            let bump = builder.ins().uextend(ir::types::I64, bump_ptr);
            let end = builder.ins().iadd(bump, aligned_size);
            let limit = builder.ins().uextend(ir::types::I64, active_space_end);
            let fits =
                builder
                    .ins()
                    .icmp(ir::condcodes::IntCC::UnsignedLessThanOrEqual, end, limit);
            let end = builder.ins().ireduce(ir::types::I32, end);
            let size = builder.ins().ireduce(ir::types::I32, aligned_size);
            (fits, end, size)
        };
        builder.ins().brif(fits, fast_block, &[], slow_block, &[]);

        // Slow path: when there isn't enough room in the bump region, call the
        // `gc_alloc_raw` libcall, which will collect or grow the GC heap as
        // necessary.
        {
            builder.switch_to_block(slow_block);
            builder.seal_block(slow_block);
            builder.set_cold_block(slow_block);
            let gc_ref = emit_gc_raw_alloc(func_env, builder, kind, ty, size, ALIGN, reserved_bits);
            let base = func_env.get_gc_heap_base(builder)?;
            let heap_offset = uextend_i32_to_pointer_type(builder, pointer_type, gc_ref);
            let obj_ptr = builder.ins().iadd(base, heap_offset);
            builder
                .ins()
                .jump(merge_block, &[gc_ref.into(), obj_ptr.into()]);
        }

        // Fast path: there is capacity for the requested object in the bump
        // region, so finish the allocation inline, update our bump pointer,
        // etc...
        {
            builder.switch_to_block(fast_block);
            builder.seal_block(fast_block);

            // The old bump_ptr is the start of the new object.
            let gc_ref = bump_ptr;

            // Update the bump pointer.
            Self::store_bump_pointer(func_env, builder, ptr_to_heap_data, end_of_object);

            // Compute the raw pointer to the new object.
            let base = func_env.get_gc_heap_base(builder)?;
            let heap_offset = uextend_i32_to_pointer_type(builder, pointer_type, gc_ref);
            let obj_ptr = builder.ins().iadd(base, heap_offset);

            // These header writes target the GC heap, so tag them with the
            // GC-heap region (like the null collector does).
            let header_flags = func_env.gc_memflags(&mut builder.func);

            // Write `VMGcHeader::kind` with inline trace info bits included.
            let kind_val = builder
                .ins()
                .iconst(ir::types::I32, i64::from(kind.as_u32() | reserved_bits));
            builder.ins().store(
                header_flags,
                kind_val,
                obj_ptr,
                i32::try_from(wasmtime_environ::VM_GC_HEADER_KIND_OFFSET).unwrap(),
            );

            // Write `VMGcHeader::type_index`.
            let shared_ty = func_env.module_interned_to_shared_ty(&mut builder.cursor(), ty);
            builder.ins().store(
                header_flags,
                shared_ty,
                obj_ptr,
                i32::try_from(wasmtime_environ::VM_GC_HEADER_TYPE_INDEX_OFFSET).unwrap(),
            );

            // Write `VMCopyingHeader::object_size`.
            builder.ins().store(
                header_flags,
                object_size,
                obj_ptr,
                i32::try_from(wasmtime_environ::VM_GC_HEADER_SIZE).unwrap(),
            );

            builder
                .ins()
                .jump(merge_block, &[gc_ref.into(), obj_ptr.into()]);
        }

        // Merge block: takes the GC ref and the raw pointer to the GC object as
        // block parameters.
        builder.switch_to_block(merge_block);
        let gc_ref = builder.append_block_param(merge_block, ir::types::I32);
        let ptr_to_object = builder.append_block_param(merge_block, pointer_type);
        builder.seal_block(merge_block);
        builder.declare_value_needs_stack_map(gc_ref);
        func_env.fresh_gc_allocation = Some(FreshAllocation::new(builder, gc_ref));

        Ok((gc_ref, ptr_to_object))
    }

    fn emit_array_alloc(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        ty: ModuleInternedTypeIndex,
        len: ir::Value,
        size: ir::Value,
        layout: &GcArrayLayout,
    ) -> WasmResult<(ir::Value, ir::Value)> {
        let (reference, address) = self.emit_inline_alloc(
            func_env,
            builder,
            VMGcKind::ArrayRef,
            ty,
            size,
            InlineTraceInfo::array(layout).encode(),
        )?;
        let flags = func_env.gc_memflags(&mut builder.func);
        let len_offset = i32::try_from(self.layouts.array_length_field_offset()).unwrap();
        builder.ins().store(flags, len, address, len_offset);
        Ok((reference, address))
    }

    pub(super) fn try_alloc_fixed_array(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        ty: ModuleInternedTypeIndex,
        elem_ty: WasmStorageType,
        elems: &[ir::Value],
    ) -> WasmResult<Option<ir::Value>> {
        // The raw address below is valid until a safepoint. Only lower
        // element types whose initializing stores cannot call the runtime.
        if let WasmStorageType::Val(WasmValType::Ref(ty)) = elem_ty
            && matches!(
                ty.heap_type.top(),
                WasmHeapTopType::Func | WasmHeapTopType::Cont
            )
        {
            return Ok(None);
        }

        let layout = func_env.array_layout(ty)?.clone();
        let len = u32::try_from(elems.len()).unwrap();
        let Some(size) = layout.size_for_len(len) else {
            return Ok(None);
        };
        let len = builder.ins().iconst(ir::types::I32, i64::from(len));
        let size = builder.ins().iconst(ir::types::I32, i64::from(size));
        let (reference, address) =
            self.emit_array_alloc(func_env, builder, ty, len, size, &layout)?;

        // Each element lies in the freshly reserved object: size_for_len
        // checked base_size + len * elem_size. The allocator has already
        // checked or allocated that range on both the fast and slow edges.
        let mut offset = u64::from(layout.base_size);
        for &elem in elems {
            let field = builder.ins().iadd_imm_u(address, i64::try_from(offset).unwrap());
            self.init_field(func_env, builder, elem_ty, field, elem)?;
            offset += u64::from(layout.elem_size);
        }
        Ok(Some(reference))
    }
}

impl GcCompiler for CopyingCompiler {
    fn layouts(&self) -> &dyn GcTypeLayouts {
        &self.layouts
    }

    fn is_moving_collector(&self) -> bool {
        true
    }

    fn alloc_uninit_array(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        array_type_index: TypeIndex,
        len: ir::Value,
    ) -> WasmResult<ir::Value> {
        let interned_type_index =
            func_env.module.types[array_type_index].unwrap_module_type_index();

        let array_layout = func_env.array_layout(interned_type_index)?.clone();
        let size = emit_array_size(func_env, builder, &array_layout, len);
        let (array_ref, _) =
            self.emit_array_alloc(func_env, builder, interned_type_index, len, size, &array_layout)?;
        Ok(array_ref)
    }

    fn alloc_struct(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        struct_type_index: TypeIndex,
        field_vals: &[ir::Value],
    ) -> WasmResult<ir::Value> {
        let interned_type_index =
            func_env.module.types[struct_type_index].unwrap_module_type_index();
        let struct_layout = func_env.struct_or_exn_layout(interned_type_index);

        let struct_size = struct_layout.size;
        let reserved_bits = InlineTraceInfo::r#struct(&struct_layout).encode();

        let struct_size_val = builder.ins().iconst(ir::types::I32, i64::from(struct_size));

        let (struct_ref, raw_ptr_to_struct) = self.emit_inline_alloc(
            func_env,
            builder,
            VMGcKind::StructRef,
            interned_type_index,
            struct_size_val,
            reserved_bits,
        )?;

        // Initialize fields.
        initialize_struct_fields(
            func_env,
            builder,
            interned_type_index,
            raw_ptr_to_struct,
            field_vals,
        )?;

        Ok(struct_ref)
    }

    fn alloc_exn(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        tag_index: TagIndex,
        field_vals: &[ir::Value],
        instance_id: ir::Value,
        tag: ir::Value,
    ) -> WasmResult<ir::Value> {
        let interned_type_index = func_env.module.tags[tag_index]
            .exception
            .unwrap_module_type_index();
        let exn_layout = func_env.struct_or_exn_layout(interned_type_index);

        let exn_size = exn_layout.size;
        let reserved_bits = InlineTraceInfo::r#struct(&exn_layout).encode();

        let exn_size_val = builder.ins().iconst(ir::types::I32, i64::from(exn_size));

        let (exn_ref, raw_ptr_to_exn) = self.emit_inline_alloc(
            func_env,
            builder,
            VMGcKind::ExnRef,
            interned_type_index,
            exn_size_val,
            reserved_bits,
        )?;

        // Initialize fields.
        initialize_struct_fields(
            func_env,
            builder,
            interned_type_index,
            raw_ptr_to_exn,
            field_vals,
        )?;

        // Initialize tag fields.
        let instance_id_addr = builder
            .ins()
            .iadd_imm_s(raw_ptr_to_exn, i64::from(EXCEPTION_TAG_INSTANCE_OFFSET));
        self.init_field(
            func_env,
            builder,
            WasmStorageType::Val(WasmValType::I32),
            instance_id_addr,
            instance_id,
        )?;
        let tag_addr = builder
            .ins()
            .iadd_imm_s(raw_ptr_to_exn, i64::from(EXCEPTION_TAG_DEFINED_OFFSET));
        self.init_field(
            func_env,
            builder,
            WasmStorageType::Val(WasmValType::I32),
            tag_addr,
            tag,
        )?;

        Ok(exn_ref)
    }

    fn translate_read_gc_reference(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder,
        ty: WasmRefType,
        src: ir::Value,
        flags: ir::MemFlagsData,
    ) -> WasmResult<ir::Value> {
        assert!(ty.is_vmgcref_type());

        let (reference_type, _) = func_env.reference_type(ty.heap_type);

        // Special case for references to uninhabited bottom types.
        if let WasmHeapType::None = ty.heap_type {
            let null = builder.ins().iconst(reference_type, 0);
            if flags.trap_code().is_some() {
                let _ = builder.ins().load(reference_type, flags, src, 0);
            }
            if !ty.nullable {
                let zero = builder.ins().iconst(ir::types::I32, 0);
                builder.ins().trapz(zero, TRAP_INTERNAL_ASSERT);
            }
            return Ok(null);
        };

        // Special case for `i31` references: they don't need stack maps.
        if let WasmHeapType::I31 = ty.heap_type {
            return unbarriered_load_gc_ref(builder, ty.heap_type, src, flags);
        }

        // No read barrier needed for the copying collector, but we do need
        // stack maps so the collector can find and relocate roots.
        unbarriered_load_gc_ref(builder, ty.heap_type, src, flags)
    }

    fn translate_write_gc_reference(
        &mut self,
        _func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder,
        ty: WasmRefType,
        dst: ir::Value,
        new_val: ir::Value,
        flags: ir::MemFlagsData,
    ) -> WasmResult<()> {
        // Heap fields record nursery edges in write_field_at_addr. Globals
        // and tables are scanned as roots.
        unbarriered_store_gc_ref(builder, ty.heap_type, dst, new_val, flags)
    }

    fn translate_init_gc_reference(
        &mut self,
        _func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder,
        ty: WasmRefType,
        dst: ir::Value,
        new_val: ir::Value,
        flags: ir::MemFlagsData,
    ) -> WasmResult<()> {
        // No write barrier needed for the copying collector.
        unbarriered_store_gc_ref(builder, ty.heap_type, dst, new_val, flags)
    }

    fn init_field(
        &mut self,
        func_env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        ty: WasmStorageType,
        field_addr: ir::Value,
        val: ir::Value,
    ) -> WasmResult<()> {
        // Data inside GC objects is always little endian.
        let flags = func_env
            .gc_memflags(&mut builder.func)
            .with_endianness(ir::Endianness::Little);

        match ty {
            WasmStorageType::Val(WasmValType::Ref(r)) => match r.heap_type.top() {
                WasmHeapTopType::Func => {
                    write_func_ref_at_addr(func_env, builder, r, flags, field_addr, val)?
                }
                WasmHeapTopType::Extern | WasmHeapTopType::Any | WasmHeapTopType::Exn => {
                    // No init barrier needed for the copying collector; just
                    // store the reference directly.
                    unbarriered_store_gc_ref(builder, r.heap_type, field_addr, val, flags)?;
                }
                WasmHeapTopType::Cont => return super::stack_switching_unsupported(),
            },
            WasmStorageType::I8 => {
                assert_eq!(builder.func.dfg.value_type(val), ir::types::I32);
                builder.ins().istore8(flags, val, field_addr, 0);
            }
            WasmStorageType::I16 => {
                assert_eq!(builder.func.dfg.value_type(val), ir::types::I32);
                builder.ins().istore16(flags, val, field_addr, 0);
            }
            WasmStorageType::Val(_) => {
                let size_of_access = wasmtime_environ::byte_size_of_wasm_ty_in_gc_heap(&ty);
                assert_eq!(builder.func.dfg.value_type(val).bytes(), size_of_access);
                builder.ins().store(flags, val, field_addr, 0);
            }
        }

        Ok(())
    }
}
