use super::*;

/// Byte copies shorter than this stay an inline loop: a bulk `array.copy` of
/// a dynamic length is a host `memmove` call with a fixed transition cost.
const BULK_BYTE_COPY_MINIMUM: i64 = 16;

/// A current byte-store witness contains collector/native resources, never a
/// semantic pointer. It is acquired only after the operation's callbacks.
#[must_use]
pub(in crate::builtins) struct BufferAccess {
    pub(in crate::builtins) owner: GcLocal<BufferOwner>,
    pub(in crate::builtins) bytes: GcLocal<ByteArray, Nullable>,
    pub(in crate::builtins) resource: GcLocal<HostResource, Nullable>,
    pub(in crate::builtins) length: I64Local,
    pub(in crate::builtins) valid: I32Local,
    pub(in crate::builtins) shared: I32Local,
}
impl BufferAccess {
    pub(in crate::builtins) fn clear(self, s: &RuntimeSchema, f: &mut Function) {
        s.release_i32_local(self.shared, f);
        s.release_i32_local(self.valid, f);
        s.release_i64_local(self.length, f);
        self.resource.clear(f);
        self.bytes.clear(f);
        self.owner.clear(f);
    }
    pub(in crate::builtins) fn read_byte(
        &self,
        offset: I64Local,
        byte: I32Local,
        b: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = b.runtime_schema();
        self.shared.load(f);
        b.open_frame(ControlFrameKind::If, f);
        if let Some(import) = b
            .functions
            .gc_host_imports()
            .get(GcHostImport::SharedBufferBase)
        {
            let _ = s
                .field(HostResourceSchema::RESOURCE)
                .read(&self.resource, s, f);
            import.emit_call_instruction(f);
            offset.load(f);
            f.instruction(&Instruction::I64Add);
            f.instruction(&Instruction::I32WrapI64);
            f.instruction(&Instruction::I32Load8U(FunctionBuilder::shared_memarg8(0)));
            byte.store(f);
        } else {
            f.instruction(&Instruction::Unreachable);
        }
        f.instruction(&Instruction::Else);
        let index = s.reserve_i32_local(f);
        offset.load(f);
        f.instruction(&Instruction::I32WrapI64);
        index.store(f);
        s.array_type::<ByteArray>()
            .read(&self.bytes, index, s, f)
            .store(byte, f);
        s.release_i32_local(index, f);
        b.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    pub(in crate::builtins) fn write_byte(
        &self,
        offset: I64Local,
        byte: I32Local,
        b: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = b.runtime_schema();
        self.shared.load(f);
        b.open_frame(ControlFrameKind::If, f);
        if let Some(import) = b
            .functions
            .gc_host_imports()
            .get(GcHostImport::SharedBufferBase)
        {
            let _ = s
                .field(HostResourceSchema::RESOURCE)
                .read(&self.resource, s, f);
            import.emit_call_instruction(f);
            offset.load(f);
            f.instruction(&Instruction::I64Add);
            f.instruction(&Instruction::I32WrapI64);
            byte.load(f);
            f.instruction(&Instruction::I32Store8(FunctionBuilder::shared_memarg8(0)));
        } else {
            f.instruction(&Instruction::Unreachable);
        }
        f.instruction(&Instruction::Else);
        let index = s.reserve_i32_local(f);
        offset.load(f);
        f.instruction(&Instruction::I32WrapI64);
        index.store(f);
        s.array_type::<ByteArray>()
            .write(&self.bytes, index, GcOperand::i32_local(byte), s, f);
        s.release_i32_local(index, f);
        b.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_binary_buffer_resizable_i32(
        &mut self,
        owner: &GcLocal<BufferOwner>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let kind = s.reserve_i32_local(f);
        s.field(BufferOwnerSchema::KIND)
            .read(owner, s, f)
            .store(kind, f);
        kind.load(f);
        f.instruction(&Instruction::I32Const(
            BufferOwnerKind::ArrayBuffer.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        let buffer = s.reserve_gc_local(f).initialize(
            s.field(BufferOwnerSchema::ARRAY_BUFFER)
                .read(owner, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let flag = s.reserve_i32_local(f);
        s.field(ArrayBufferSchema::RESIZABLE)
            .read(&buffer, s, f)
            .store(flag, f);
        flag.load(f);
        s.release_i32_local(flag, f);
        buffer.clear(f);
        f.instruction(&Instruction::Else);
        if let Some(import) = self
            .functions
            .gc_host_imports()
            .get(GcHostImport::SharedBufferGrowable)
        {
            let buffer = s.reserve_gc_local(f).initialize(
                s.field(BufferOwnerSchema::SHARED_ARRAY_BUFFER)
                    .read(owner, s, f)
                    .reference()
                    .require_non_null(f),
                f,
            );
            let resource = s.reserve_gc_local(f).initialize(
                s.field(SharedArrayBufferSchema::BACKING_RESOURCE)
                    .read(&buffer, s, f)
                    .reference(),
                f,
            );
            let _ = s.field(HostResourceSchema::RESOURCE).read(&resource, s, f);
            import.emit_call_instruction(f);
            resource.clear(f);
            buffer.clear(f);
        } else {
            f.instruction(&Instruction::Unreachable);
        }
        f.instruction(&Instruction::End);
        s.release_i32_local(kind, f);
    }
    pub(in crate::builtins) fn emit_binary_buffer_owner(
        &mut self,
        input: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<BufferOwner>, EmitError> {
        let s = self.runtime_schema();
        let owner = s
            .reserve_gc_local::<BufferOwner, Nullable>(f)
            .initialize_null(s, f);
        input.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayBuffer>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let buffer = s
            .reserve_gc_local(f)
            .initialize(input.cast_reference::<ArrayBuffer>(s, f), f);
        owner.replace(
            s.struct_type::<BufferOwner>()
                .construct(
                    (
                        GcOperand::constant(BufferOwnerKind::ArrayBuffer),
                        GcOperand::nullable_reference(&buffer, s),
                        GcOperand::null(s),
                    ),
                    f,
                )
                .nullable(),
            f,
        );
        buffer.clear(f);
        f.instruction(&Instruction::Else);
        let buffer = self.emit_binary_require_ref::<SharedArrayBuffer>(
            input,
            RuntimeErrorMessage::DATAVIEW_CONSTRUCTOR_REQUIRES_ARRAYBUFFER,
            output,
            exit,
            f,
        )?;
        owner.replace(
            s.struct_type::<BufferOwner>()
                .construct(
                    (
                        GcOperand::constant(BufferOwnerKind::SharedArrayBuffer),
                        GcOperand::null(s),
                        GcOperand::nullable_reference(&buffer, s),
                    ),
                    f,
                )
                .nullable(),
            f,
        );
        buffer.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let completed = s
            .reserve_gc_local(f)
            .initialize(owner.load(s, f).require_non_null(f), f);
        owner.clear(f);
        Ok(completed)
    }

    pub(in crate::builtins) fn emit_binary_buffer_access(
        &mut self,
        input: &GcLocal<BufferOwner>,
        f: &mut Function,
    ) -> BufferAccess {
        let s = self.runtime_schema();
        let owner = s.reserve_gc_local(f).initialize(input.load(s, f), f);
        let bytes = s
            .reserve_gc_local::<ByteArray, Nullable>(f)
            .initialize_null(s, f);
        let resource = s
            .reserve_gc_local::<HostResource, Nullable>(f)
            .initialize_null(s, f);
        let length = s.reserve_i64_local(f);
        let valid = s.reserve_i32_local(f);
        let shared = s.reserve_i32_local(f);
        f.instruction(&Instruction::I64Const(0));
        length.store(f);
        f.instruction(&Instruction::I32Const(0));
        valid.store(f);
        f.instruction(&Instruction::I32Const(0));
        shared.store(f);
        let kind = s.reserve_i32_local(f);
        s.field(BufferOwnerSchema::KIND)
            .read(&owner, s, f)
            .store(kind, f);
        kind.load(f);
        f.instruction(&Instruction::I32Const(
            BufferOwnerKind::ArrayBuffer.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let buffer = s.reserve_gc_local(f).initialize(
            s.field(BufferOwnerSchema::ARRAY_BUFFER)
                .read(&owner, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        bytes.replace(
            s.field(ArrayBufferSchema::BYTES)
                .read(&buffer, s, f)
                .reference(),
            f,
        );
        bytes.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.field(ArrayBufferSchema::BYTE_LENGTH)
            .read(&buffer, s, f)
            .store_i64(length, f);
        f.instruction(&Instruction::I32Const(1));
        valid.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        buffer.clear(f);
        f.instruction(&Instruction::Else);
        if let Some(import) = self
            .functions
            .gc_host_imports()
            .get(GcHostImport::SharedBufferLength)
        {
            let buffer = s.reserve_gc_local(f).initialize(
                s.field(BufferOwnerSchema::SHARED_ARRAY_BUFFER)
                    .read(&owner, s, f)
                    .reference()
                    .require_non_null(f),
                f,
            );
            resource.replace(
                s.field(SharedArrayBufferSchema::BACKING_RESOURCE)
                    .read(&buffer, s, f)
                    .reference()
                    .nullable(),
                f,
            );
            let _ = s.field(HostResourceSchema::RESOURCE).read(&resource, s, f);
            import.emit_call_instruction(f);
            length.store(f);
            f.instruction(&Instruction::I32Const(1));
            valid.store(f);
            f.instruction(&Instruction::I32Const(1));
            shared.store(f);
            buffer.clear(f);
        } else {
            f.instruction(&Instruction::Unreachable);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(kind, f);
        BufferAccess {
            owner,
            bytes,
            resource,
            length,
            valid,
            shared,
        }
    }

    pub(in crate::builtins) fn emit_binary_buffer_value(
        &mut self,
        owner: &GcLocal<BufferOwner>,
        output: &ValueLocals,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let kind = s.reserve_i32_local(f);
        s.field(BufferOwnerSchema::KIND)
            .read(owner, s, f)
            .store(kind, f);
        kind.load(f);
        f.instruction(&Instruction::I32Const(
            BufferOwnerKind::ArrayBuffer.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let buffer = s.reserve_gc_local(f).initialize(
            s.field(BufferOwnerSchema::ARRAY_BUFFER)
                .read(owner, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        output.set_reference(&buffer, s, f);
        buffer.clear(f);
        f.instruction(&Instruction::Else);
        let buffer = s.reserve_gc_local(f).initialize(
            s.field(BufferOwnerSchema::SHARED_ARRAY_BUFFER)
                .read(owner, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        output.set_reference(&buffer, s, f);
        buffer.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(kind, f);
    }

    pub(in crate::builtins) fn emit_binary_buffer_immutable_i32(
        &mut self,
        owner: &GcLocal<BufferOwner>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let kind = s.reserve_i32_local(f);
        s.field(BufferOwnerSchema::KIND)
            .read(owner, s, f)
            .store(kind, f);
        kind.load(f);
        f.instruction(&Instruction::I32Const(
            BufferOwnerKind::ArrayBuffer.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        let buffer = s.reserve_gc_local(f).initialize(
            s.field(BufferOwnerSchema::ARRAY_BUFFER)
                .read(owner, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let immutable = s.reserve_i32_local(f);
        s.field(ArrayBufferSchema::IMMUTABLE)
            .read(&buffer, s, f)
            .store(immutable, f);
        immutable.load(f);
        s.release_i32_local(immutable, f);
        buffer.clear(f);
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::End);
        s.release_i32_local(kind, f);
    }

    pub(in crate::builtins) fn emit_binary_view_length(
        &mut self,
        view: &GcLocal<BufferView>,
        access: &BufferAccess,
        length: I64Local,
        valid: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let offset = s.reserve_i64_local(f);
        let fixed = s.reserve_i64_local(f);
        let tracking = s.reserve_i32_local(f);
        s.field(BufferViewSchema::BYTE_OFFSET)
            .read(view, s, f)
            .store_i64(offset, f);
        s.field(BufferViewSchema::FIXED_BYTE_LENGTH)
            .read(view, s, f)
            .store_i64(fixed, f);
        s.field(BufferViewSchema::LENGTH_TRACKING)
            .read(view, s, f)
            .store(tracking, f);
        f.instruction(&Instruction::I64Const(0));
        length.store(f);
        access.valid.load(f);
        offset.load(f);
        access.length.load(f);
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        valid.store(f);
        valid.load(f);
        self.open_frame(ControlFrameKind::If, f);
        tracking.load(f);
        self.open_frame(ControlFrameKind::If, f);
        access.length.load(f);
        offset.load(f);
        f.instruction(&Instruction::I64Sub);
        length.store(f);
        f.instruction(&Instruction::Else);
        fixed.load(f);
        access.length.load(f);
        offset.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64LeU);
        valid.store(f);
        valid.load(f);
        self.open_frame(ControlFrameKind::If, f);
        fixed.load(f);
        length.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(tracking, f);
        s.release_i64_local(fixed, f);
        s.release_i64_local(offset, f);
    }

    pub(in crate::builtins) fn emit_binary_copy_bytes(
        &mut self,
        source: &BufferAccess,
        from: I64Local,
        target: &BufferAccess,
        to: I64Local,
        count: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let cursor = s.reserve_i64_local(f);
        let source_index = s.reserve_i64_local(f);
        let target_index = s.reserve_i64_local(f);
        let byte = s.reserve_i32_local(f);
        let copied = self.open_frame(ControlFrameKind::Block, f);
        let bytewise = self.open_frame(ControlFrameKind::Block, f);
        // Two distinct collector-owned backings copy in one bulk move: with
        // no shared memory and no aliasing, the ascending byte loop below
        // stores exactly these bytes. A shared block keeps its ordered
        // host-memory accesses, one backing keeps the ascending order that
        // an overlapping range observes, and a short copy is cheaper inline
        // than the engine's bulk transition.
        source.shared.load(f);
        target.shared.load(f);
        f.instruction(&Instruction::I32Or);
        self.emit_branch_if_to_target(bytewise, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(BULK_BYTE_COPY_MINIMUM));
        f.instruction(&Instruction::I64LtU);
        self.emit_branch_if_to_target(bytewise, f);
        source.bytes.load(s, f).is_null(f);
        self.emit_branch_if_to_target(bytewise, f);
        target.bytes.load(s, f).is_null(f);
        self.emit_branch_if_to_target(bytewise, f);
        let _ = source.bytes.load(s, f);
        let _ = target.bytes.load(s, f);
        f.instruction(&Instruction::RefEq);
        self.emit_branch_if_to_target(bytewise, f);
        let destination = s
            .reserve_gc_local(f)
            .initialize(target.bytes.load(s, f).require_non_null(f), f);
        let destination_start = s.reserve_i32_local(f);
        let source_start = s.reserve_i32_local(f);
        let extent = s.reserve_i32_local(f);
        for (wide, narrow) in [
            (to, destination_start),
            (from, source_start),
            (count, extent),
        ] {
            wide.load(f);
            f.instruction(&Instruction::I32WrapI64);
            narrow.store(f);
        }
        s.array_type::<ByteArray>().copy_range_from(
            &destination,
            destination_start,
            &source.bytes,
            source_start,
            extent,
            s,
            f,
        );
        for local in [extent, source_start, destination_start] {
            s.release_i32_local(local, f);
        }
        destination.clear(f);
        self.emit_branch_to_target(copied, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I64Const(0));
        cursor.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        from.load(f);
        cursor.load(f);
        f.instruction(&Instruction::I64Add);
        source_index.store(f);
        to.load(f);
        cursor.load(f);
        f.instruction(&Instruction::I64Add);
        target_index.store(f);
        source.read_byte(source_index, byte, self, f)?;
        target.write_byte(target_index, byte, self, f)?;
        self.emit_increment_local(cursor, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(byte, f);
        s.release_i64_local(target_index, f);
        s.release_i64_local(source_index, f);
        s.release_i64_local(cursor, f);
        Ok(())
    }
}
