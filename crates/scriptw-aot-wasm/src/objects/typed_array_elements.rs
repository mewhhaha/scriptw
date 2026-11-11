use super::*;
use crate::gc_types::{
    ArrayBufferSchema, BigIntConstruction, BigIntLimbArray, BigIntValue,
    BigIntValueSchema, BufferOwner, BufferOwnerKind, BufferOwnerSchema, BufferView,
    BufferViewSchema, ByteArray, GcHostImport, GcI32Constant, HostResource, HostResourceSchema,
    I64Local, RuntimeSchema, SharedArrayBufferSchema, TypedArrayObject, TypedArrayObjectSchema,
};

/// A view is acquired after observable conversion and owns the exact backing
/// resource and current byte length used by the following element access.
struct ElementAccess {
    view: GcLocal<BufferView>,
    buffer: GcLocal<BufferOwner>,
    bytes: GcLocal<ByteArray, Nullable>,
    resource: GcLocal<HostResource, Nullable>,
    offset: I64Local,
    length: I64Local,
    width: I64Local,
    kind: I32Local,
    shared: I32Local,
    in_bounds: I32Local,
}

impl ElementAccess {
    fn clear(self, schema: &RuntimeSchema, function: &mut Function) {
        schema.release_i32_local(self.in_bounds, function);
        schema.release_i32_local(self.shared, function);
        schema.release_i32_local(self.kind, function);
        schema.release_i64_local(self.width, function);
        schema.release_i64_local(self.length, function);
        schema.release_i64_local(self.offset, function);
        self.resource.clear(function);
        self.bytes.clear(function);
        self.buffer.clear(function);
        self.view.clear(function);
    }
}

impl FunctionBuilder<'_> {
    /// Native write operations reject immutable backing before taking a bounds
    /// witness. Ordinary integer-indexed Set has its separate operation order.
    pub(crate) fn emit_validate_typed_array_write_view(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        length: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_validate_typed_array_writable_buffer(target, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_validate_typed_array_view(target, length, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(in crate::objects) fn emit_typed_array_immutable_buffer_i32(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let immutable = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        immutable.store(function);
        let view = schema.reserve_gc_local(function).initialize(
            schema
                .field(TypedArrayObjectSchema::VIEW)
                .read(target, schema, function)
                .reference(),
            function,
        );
        let buffer = schema.reserve_gc_local(function).initialize(
            schema
                .field(BufferViewSchema::BUFFER)
                .read(&view, schema, function)
                .reference(),
            function,
        );
        let kind = schema.reserve_i32_local(function);
        schema
            .field(BufferOwnerSchema::KIND)
            .read(&buffer, schema, function)
            .store(kind, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(
            BufferOwnerKind::ArrayBuffer.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let array_buffer = schema.reserve_gc_local(function).initialize(
            schema
                .field(BufferOwnerSchema::ARRAY_BUFFER)
                .read(&buffer, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        schema
            .field(ArrayBufferSchema::IMMUTABLE)
            .read(&array_buffer, schema, function)
            .store(immutable, function);
        array_buffer.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(kind, function);
        buffer.clear(function);
        view.clear(function);
        immutable.load(function);
        schema.release_i32_local(immutable, function);
    }

    pub(crate) fn emit_validate_typed_array_writable_buffer(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        result.initialize(function);
        self.emit_typed_array_immutable_buffer_i32(target, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::DATAVIEW_BACKING_BUFFER_IS_IMMUTABLE,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// Same-kind cloning preserves the representation bits, including NaN
    /// payloads, rather than round-tripping through a JavaScript Number.
    pub(crate) fn emit_typed_array_element_bits_read(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        index: I64Local,
        word: I64Local,
        valid: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let access = self.acquire_typed_array_element_access(target, function);
        access.in_bounds.load(function);
        index.load(function);
        access.length.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        valid.store(function);
        function.instruction(&Instruction::I64Const(0));
        word.store(function);
        valid.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_element_word_read(&access, index, word, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        access.clear(schema, function);
    }

    pub(crate) fn emit_typed_array_element_bits_write(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        index: I64Local,
        word: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_validate_typed_array_writable_buffer(target, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let access = self.acquire_typed_array_element_access(target, function);
        access.in_bounds.load(function);
        index.load(function);
        access.length.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_element_word_write(&access, index, word, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        access.clear(schema, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// copyWithin is a byte operation even for integer shared views. Acquire
    /// the real element owner once, then privately interpret its checked range
    /// as Uint8 bytes. No fabricated BufferView or published kind is created.
    pub(crate) fn emit_typed_array_copy_within_bytes(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        from: I64Local,
        to: I64Local,
        count: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_validate_typed_array_writable_buffer(target, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let access = self.acquire_typed_array_element_access(target, function);
        let source = schema.reserve_i64_local(function);
        let destination = schema.reserve_i64_local(function);
        let remaining = schema.reserve_i64_local(function);
        let direction = schema.reserve_i64_local(function);
        let word = schema.reserve_i64_local(function);
        let copied = self.open_frame(ControlFrameKind::Block, function);
        // The native caller clips after its second bounds check. Authenticate
        // the range before multiplying it by the retained element width.
        access.in_bounds.load(function);
        from.load(function);
        access.length.load(function);
        function.instruction(&Instruction::I64LeU);
        function.instruction(&Instruction::I32And);
        to.load(function);
        access.length.load(function);
        function.instruction(&Instruction::I64LeU);
        function.instruction(&Instruction::I32And);
        count.load(function);
        access.length.load(function);
        from.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64LeU);
        function.instruction(&Instruction::I32And);
        count.load(function);
        access.length.load(function);
        to.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64LeU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::TYPEDARRAY_BYTELENGTH_OUT_OF_BOUNDS,
            result,
            function,
        )?;
        self.emit_branch_to_target(copied, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        from.load(function);
        access.width.load(function);
        function.instruction(&Instruction::I64Mul);
        source.store(function);
        to.load(function);
        access.width.load(function);
        function.instruction(&Instruction::I64Mul);
        destination.store(function);
        count.load(function);
        access.width.load(function);
        function.instruction(&Instruction::I64Mul);
        remaining.store(function);
        function.instruction(&Instruction::I64Const(1));
        direction.store(function);
        source.load(function);
        destination.load(function);
        function.instruction(&Instruction::I64LtU);
        destination.load(function);
        source.load(function);
        remaining.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        source.load(function);
        remaining.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        source.store(function);
        destination.load(function);
        remaining.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        destination.store(function);
        function.instruction(&Instruction::I64Const(-1));
        direction.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(1));
        access.width.store(function);
        function.instruction(&Instruction::I32Const(
            TypedArrayElementKind::Uint8.encode(),
        ));
        access.kind.store(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        remaining.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(done, function);
        self.emit_element_word_read(&access, source, word, function);
        self.emit_element_word_write(&access, destination, word, function);
        source.load(function);
        direction.load(function);
        function.instruction(&Instruction::I64Add);
        source.store(function);
        destination.load(function);
        direction.load(function);
        function.instruction(&Instruction::I64Add);
        destination.store(function);
        self.emit_increment_local(remaining, -1, function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i64_local(word, function);
        schema.release_i64_local(direction, function);
        schema.release_i64_local(remaining, function);
        schema.release_i64_local(destination, function);
        schema.release_i64_local(source, function);
        access.clear(schema, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(super) fn emit_typed_array_fixed_length_i32(
        &self,
        target: &GcLocal<TypedArrayObject>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let view = schema.reserve_gc_local(function).initialize(
            schema
                .field(TypedArrayObjectSchema::VIEW)
                .read(target, schema, function)
                .reference(),
            function,
        );
        let buffer = schema.reserve_gc_local(function).initialize(
            schema
                .field(BufferViewSchema::BUFFER)
                .read(&view, schema, function)
                .reference(),
            function,
        );
        let tracking = schema.reserve_i32_local(function);
        let kind = schema.reserve_i32_local(function);
        let resizable = schema.reserve_i32_local(function);
        schema
            .field(BufferViewSchema::LENGTH_TRACKING)
            .read(&view, schema, function)
            .store(tracking, function);
        schema
            .field(BufferOwnerSchema::KIND)
            .read(&buffer, schema, function)
            .store(kind, function);
        function.instruction(&Instruction::I32Const(0));
        resizable.store(function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(
            BufferOwnerKind::ArrayBuffer.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let array_buffer = schema.reserve_gc_local(function).initialize(
            schema
                .field(BufferOwnerSchema::ARRAY_BUFFER)
                .read(&buffer, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        schema
            .field(ArrayBufferSchema::RESIZABLE)
            .read(&array_buffer, schema, function)
            .store(resizable, function);
        array_buffer.clear(function);
        function.instruction(&Instruction::End);
        tracking.load(function);
        resizable.load(function);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        schema.release_i32_local(resizable, function);
        schema.release_i32_local(kind, function);
        schema.release_i32_local(tracking, function);
        buffer.clear(function);
        view.clear(function);
    }

    fn acquire_typed_array_element_access(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        function: &mut Function,
    ) -> ElementAccess {
        let schema = self.runtime_schema();
        let view = schema.reserve_gc_local(function).initialize(
            schema
                .field(TypedArrayObjectSchema::VIEW)
                .read(target, schema, function)
                .reference(),
            function,
        );
        let buffer = schema.reserve_gc_local(function).initialize(
            schema
                .field(BufferViewSchema::BUFFER)
                .read(&view, schema, function)
                .reference(),
            function,
        );
        let bytes = schema
            .reserve_gc_local(function)
            .initialize_null(schema, function);
        let resource = schema
            .reserve_gc_local(function)
            .initialize_null(schema, function);
        let offset = schema.reserve_i64_local(function);
        let length = schema.reserve_i64_local(function);
        let width = schema.reserve_i64_local(function);
        let kind = schema.reserve_i32_local(function);
        let shared = schema.reserve_i32_local(function);
        let in_bounds = schema.reserve_i32_local(function);
        let current_length = schema.reserve_i64_local(function);
        let fixed_length = schema.reserve_i64_local(function);
        let tracking = schema.reserve_i32_local(function);
        let buffer_kind = schema.reserve_i32_local(function);
        schema
            .field(BufferViewSchema::BYTE_OFFSET)
            .read(&view, schema, function)
            .store_i64(offset, function);
        schema
            .field(BufferViewSchema::FIXED_BYTE_LENGTH)
            .read(&view, schema, function)
            .store_i64(fixed_length, function);
        schema
            .field(BufferViewSchema::LENGTH_TRACKING)
            .read(&view, schema, function)
            .store(tracking, function);
        schema
            .field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(target, schema, function)
            .store(kind, function);
        function.instruction(&Instruction::I64Const(0));
        width.store(function);
        for element_kind in TypedArrayElementKind::ALL {
            kind.load(function);
            function.instruction(&Instruction::I32Const(element_kind.encode()));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(
                element_kind.bytes_per_element() as i64
            ));
            width.store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        function.instruction(&Instruction::I32Const(0));
        shared.store(function);
        function.instruction(&Instruction::I32Const(0));
        in_bounds.store(function);
        function.instruction(&Instruction::I64Const(0));
        length.store(function);
        function.instruction(&Instruction::I64Const(0));
        current_length.store(function);
        schema
            .field(BufferOwnerSchema::KIND)
            .read(&buffer, schema, function)
            .store(buffer_kind, function);
        buffer_kind.load(function);
        function.instruction(&Instruction::I32Const(
            BufferOwnerKind::ArrayBuffer.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let array_buffer = schema.reserve_gc_local(function).initialize(
            schema
                .field(BufferOwnerSchema::ARRAY_BUFFER)
                .read(&buffer, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        bytes.replace(
            schema
                .field(ArrayBufferSchema::BYTES)
                .read(&array_buffer, schema, function)
                .reference(),
            function,
        );
        bytes.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .field(ArrayBufferSchema::BYTE_LENGTH)
            .read(&array_buffer, schema, function)
            .store_i64(current_length, function);
        function.instruction(&Instruction::I32Const(1));
        in_bounds.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        array_buffer.clear(function);
        function.instruction(&Instruction::Else);
        // The finalized import plan admits every SAB/Agent producer. If the
        // resource imports are absent, this closed backing kind is unreachable.
        if let Some(length_import) = self
            .functions
            .gc_host_imports()
            .get(GcHostImport::SharedBufferLength)
        {
            let shared_buffer = schema.reserve_gc_local(function).initialize(
                schema
                    .field(BufferOwnerSchema::SHARED_ARRAY_BUFFER)
                    .read(&buffer, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            );
            resource.replace(
                schema
                    .field(SharedArrayBufferSchema::BACKING_RESOURCE)
                    .read(&shared_buffer, schema, function)
                    .reference()
                    .nullable(),
                function,
            );
            let _ = schema
                .field(HostResourceSchema::RESOURCE)
                .read(&resource, schema, function);
            length_import.emit_call_instruction(function);
            current_length.store(function);
            function.instruction(&Instruction::I32Const(1));
            shared.store(function);
            function.instruction(&Instruction::I32Const(1));
            in_bounds.store(function);
            shared_buffer.clear(function);
        } else {
            function.instruction(&Instruction::Unreachable);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        in_bounds.load(function);
        self.open_frame(ControlFrameKind::If, function);
        offset.load(function);
        current_length.load(function);
        function.instruction(&Instruction::I64LeU);
        self.open_frame(ControlFrameKind::If, function);
        tracking.load(function);
        self.open_frame(ControlFrameKind::If, function);
        current_length.load(function);
        offset.load(function);
        function.instruction(&Instruction::I64Sub);
        width.load(function);
        function.instruction(&Instruction::I64DivU);
        length.store(function);
        function.instruction(&Instruction::Else);
        fixed_length.load(function);
        current_length.load(function);
        offset.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64LeU);
        self.open_frame(ControlFrameKind::If, function);
        fixed_length.load(function);
        width.load(function);
        function.instruction(&Instruction::I64DivU);
        length.store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        in_bounds.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        in_bounds.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(buffer_kind, function);
        schema.release_i32_local(tracking, function);
        schema.release_i64_local(fixed_length, function);
        schema.release_i64_local(current_length, function);
        ElementAccess {
            view,
            buffer,
            bytes,
            resource,
            offset,
            length,
            width,
            kind,
            shared,
            in_bounds,
        }
    }

    pub(super) fn emit_typed_array_own_keys_length(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        output: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let access = self.acquire_typed_array_element_access(target, function);
        function.instruction(&Instruction::I64Const(0));
        output.store(function);
        access.in_bounds.load(function);
        self.open_frame(ControlFrameKind::If, function);
        access.length.load(function);
        output.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        access.clear(schema, function);
    }

    pub(crate) fn emit_typed_array_length_snapshot(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        length: I64Local,
        valid: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let access = self.acquire_typed_array_element_access(target, function);
        access.length.load(function);
        length.store(function);
        access.in_bounds.load(function);
        valid.store(function);
        access.clear(schema, function);
    }

    pub(crate) fn emit_validate_typed_array_view(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        length: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let access = self.acquire_typed_array_element_access(target, function);
        result.initialize(function);
        access.length.load(function);
        length.store(function);
        access.in_bounds.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        access.shared.load(function);
        function.instruction(&Instruction::I32Eqz);
        access.bytes.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::TYPEDARRAY_BACKING_BUFFER_IS_DETACHED,
            result,
            function,
        )?;
        function.instruction(&Instruction::Else);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::TYPEDARRAY_BYTELENGTH_OUT_OF_BOUNDS,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        access.clear(schema, function);
        Ok(())
    }

    pub(crate) fn emit_typed_array_valid_integer_index_i32(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        number: I64Local,
        index: I64Local,
        valid: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_integer_index_or_invalid(number, index, function);
        let schema = self.runtime_schema();
        let access = self.acquire_typed_array_element_access(target, function);
        access.in_bounds.load(function);
        index.load(function);
        access.length.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        valid.store(function);
        access.clear(schema, function);
        Ok(())
    }

    fn emit_element_word_read(
        &mut self,
        access: &ElementAccess,
        index: I64Local,
        word: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let address = schema.reserve_i64_local(function);
        let cursor = schema.reserve_i32_local(function);
        let byte_index = schema.reserve_i32_local(function);
        access.offset.load(function);
        index.load(function);
        access.width.load(function);
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        address.store(function);
        access.shared.load(function);
        self.open_frame(ControlFrameKind::If, function);
        if let Some(base_import) = self
            .functions
            .gc_host_imports()
            .get(GcHostImport::SharedBufferBase)
        {
            let _ =
                schema
                    .field(HostResourceSchema::RESOURCE)
                    .read(&access.resource, schema, function);
            base_import.emit_call_instruction(function);
            address.load(function);
            function.instruction(&Instruction::I64Add);
            address.store(function);
            for kind in TypedArrayElementKind::ALL {
                access.kind.load(function);
                function.instruction(&Instruction::I32Const(kind.encode()));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                address.load(function);
                function.instruction(&Instruction::I32WrapI64);
                // Unclamped integer configurations must not tear. Floating
                // and unordered BigInt views retain their ordinary access.
                function.instruction(&match kind {
                    TypedArrayElementKind::Int8
                    | TypedArrayElementKind::Uint8
                    | TypedArrayElementKind::Uint8Clamped => {
                        Instruction::I64AtomicLoad8U(Self::shared_memarg8(0))
                    }
                    TypedArrayElementKind::Int16 | TypedArrayElementKind::Uint16 => {
                        Instruction::I64AtomicLoad16U(Self::shared_memarg16(0))
                    }
                    TypedArrayElementKind::Int32 | TypedArrayElementKind::Uint32 => {
                        Instruction::I64AtomicLoad32U(Self::shared_memarg32(0))
                    }
                    TypedArrayElementKind::Float16 => {
                        Instruction::I64Load16U(Self::shared_memarg16(0))
                    }
                    TypedArrayElementKind::Float32 => {
                        Instruction::I64Load32U(Self::shared_memarg32(0))
                    }
                    TypedArrayElementKind::Float64
                    | TypedArrayElementKind::BigInt64
                    | TypedArrayElementKind::BigUint64 => {
                        Instruction::I64Load(Self::shared_memarg64(0))
                    }
                });
                word.store(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        } else {
            function.instruction(&Instruction::Unreachable);
        }
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        word.store(function);
        function.instruction(&Instruction::I32Const(0));
        cursor.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        cursor.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        access.width.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        address.load(function);
        function.instruction(&Instruction::I32WrapI64);
        cursor.load(function);
        function.instruction(&Instruction::I32Add);
        byte_index.store(function);
        word.load(function);
        schema
            .array_type::<ByteArray>()
            .read(&access.bytes, byte_index, schema, function)
            .store(byte_index, function);
        byte_index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        cursor.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Or);
        word.store(function);
        cursor.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        cursor.store(function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(byte_index, function);
        schema.release_i32_local(cursor, function);
        schema.release_i64_local(address, function);
    }

    fn emit_element_word_write(
        &mut self,
        access: &ElementAccess,
        index: I64Local,
        word: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let address = schema.reserve_i64_local(function);
        let cursor = schema.reserve_i32_local(function);
        let byte_index = schema.reserve_i32_local(function);
        let byte = schema.reserve_i32_local(function);
        access.offset.load(function);
        index.load(function);
        access.width.load(function);
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        address.store(function);
        access.shared.load(function);
        self.open_frame(ControlFrameKind::If, function);
        if let Some(base_import) = self
            .functions
            .gc_host_imports()
            .get(GcHostImport::SharedBufferBase)
        {
            let _ =
                schema
                    .field(HostResourceSchema::RESOURCE)
                    .read(&access.resource, schema, function);
            base_import.emit_call_instruction(function);
            address.load(function);
            function.instruction(&Instruction::I64Add);
            address.store(function);
            for kind in TypedArrayElementKind::ALL {
                access.kind.load(function);
                function.instruction(&Instruction::I32Const(kind.encode()));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                address.load(function);
                function.instruction(&Instruction::I32WrapI64);
                word.load(function);
                function.instruction(&match kind {
                    TypedArrayElementKind::Int8
                    | TypedArrayElementKind::Uint8
                    | TypedArrayElementKind::Uint8Clamped => {
                        Instruction::I64AtomicStore8(Self::shared_memarg8(0))
                    }
                    TypedArrayElementKind::Int16 | TypedArrayElementKind::Uint16 => {
                        Instruction::I64AtomicStore16(Self::shared_memarg16(0))
                    }
                    TypedArrayElementKind::Int32 | TypedArrayElementKind::Uint32 => {
                        Instruction::I64AtomicStore32(Self::shared_memarg32(0))
                    }
                    TypedArrayElementKind::Float16 => {
                        Instruction::I64Store16(Self::shared_memarg16(0))
                    }
                    TypedArrayElementKind::Float32 => {
                        Instruction::I64Store32(Self::shared_memarg32(0))
                    }
                    TypedArrayElementKind::Float64
                    | TypedArrayElementKind::BigInt64
                    | TypedArrayElementKind::BigUint64 => {
                        Instruction::I64Store(Self::shared_memarg64(0))
                    }
                });
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        } else {
            function.instruction(&Instruction::Unreachable);
        }
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        cursor.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        cursor.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        access.width.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        address.load(function);
        function.instruction(&Instruction::I32WrapI64);
        cursor.load(function);
        function.instruction(&Instruction::I32Add);
        byte_index.store(function);
        word.load(function);
        cursor.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I32WrapI64);
        byte.store(function);
        schema.array_type::<ByteArray>().write(
            &access.bytes,
            byte_index,
            GcOperand::i32_local(byte),
            schema,
            function,
        );
        cursor.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        cursor.store(function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(byte, function);
        schema.release_i32_local(byte_index, function);
        schema.release_i32_local(cursor, function);
        schema.release_i64_local(address, function);
    }

    pub(crate) fn emit_typed_array_element_read_from_locals(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        index: I64Local,
        result: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let access = self.acquire_typed_array_element_access(target, function);
        let word = schema.reserve_i64_local(function);
        result.set_undefined(function);
        access.in_bounds.load(function);
        index.load(function);
        access.length.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_element_word_read(&access, index, word, function);
        self.emit_typed_array_element_word_to_value(access.kind, word, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(word, function);
        access.clear(schema, function);
        Ok(())
    }

    /// Decode a word already read from the sole backing owner, including byte-cloned Set input.
    pub(crate) fn emit_typed_array_element_word_to_value(
        &mut self,
        kind_local: I32Local,
        word: I64Local,
        result: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let bits = schema.reserve_i64_local(function);
        result.set_undefined(function);
        for kind in TypedArrayElementKind::ALL {
            kind_local.load(function);
            function.instruction(&Instruction::I32Const(kind.encode()));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            match kind {
                TypedArrayElementKind::Float16 => {
                    self.emit_element_half_to_number(word, bits, function);
                    result.set_number(bits, function);
                }
                TypedArrayElementKind::Float32 => {
                    word.load(function);
                    function.instruction(&Instruction::I32WrapI64);
                    function.instruction(&Instruction::F32ReinterpretI32);
                    function.instruction(&Instruction::F64PromoteF32);
                    function.instruction(&Instruction::I64ReinterpretF64);
                    bits.store(function);
                    result.set_number(bits, function);
                }
                TypedArrayElementKind::Float64 => result.set_number(word, function),
                TypedArrayElementKind::Int8
                | TypedArrayElementKind::Int16
                | TypedArrayElementKind::Int32
                | TypedArrayElementKind::Uint8
                | TypedArrayElementKind::Uint8Clamped
                | TypedArrayElementKind::Uint16
                | TypedArrayElementKind::Uint32 => {
                    word.load(function);
                    let signed = matches!(
                        kind,
                        TypedArrayElementKind::Int8
                            | TypedArrayElementKind::Int16
                            | TypedArrayElementKind::Int32
                    );
                    if signed {
                        function.instruction(&match kind {
                            TypedArrayElementKind::Int8 => Instruction::I64Extend8S,
                            TypedArrayElementKind::Int16 => Instruction::I64Extend16S,
                            TypedArrayElementKind::Int32 => Instruction::I64Extend32S,
                            _ => unreachable!("signed integer element"),
                        });
                    }
                    function.instruction(&if signed {
                        Instruction::F64ConvertI64S
                    } else {
                        Instruction::F64ConvertI64U
                    });
                    function.instruction(&Instruction::I64ReinterpretF64);
                    bits.store(function);
                    result.set_number(bits, function);
                }
                TypedArrayElementKind::BigInt64 | TypedArrayElementKind::BigUint64 => {
                    let length = schema.reserve_i32_local(function);
                    let limb_index = schema.reserve_i32_local(function);
                    let negative = schema.reserve_i32_local(function);
                    function.instruction(&Instruction::I32Const(1));
                    length.store(function);
                    function.instruction(&Instruction::I32Const(0));
                    limb_index.store(function);
                    function.instruction(&Instruction::I32Const(0));
                    negative.store(function);
                    word.load(function);
                    bits.store(function);
                    if kind == TypedArrayElementKind::BigInt64 {
                        word.load(function);
                        function.instruction(&Instruction::I64Const(0));
                        function.instruction(&Instruction::I64LtS);
                        negative.store(function);
                        negative.load(function);
                        self.open_frame(ControlFrameKind::If, function);
                        function.instruction(&Instruction::I64Const(0));
                        word.load(function);
                        function.instruction(&Instruction::I64Sub);
                        bits.store(function);
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                    }
                    let construction = BigIntConstruction::allocate(
                        schema,
                        schema.reserve_gc_local(function),
                        length,
                        function,
                    );
                    construction.write(limb_index, bits, schema, function);
                    let bigint = schema
                        .reserve_gc_local(function)
                        .initialize(construction.publish(negative, schema, function), function);
                    result.set_reference(&bigint, schema, function);
                    bigint.clear(function);
                    schema.release_i32_local(negative, function);
                    schema.release_i32_local(limb_index, function);
                    schema.release_i32_local(length, function);
                }
            }
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        schema.release_i64_local(bits, function);
        Ok(())
    }

    pub(crate) fn emit_to_bigint_u64_word_from_value_locals(
        &mut self,
        input: &ValueLocals,
        word: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_value_to_bigint_locals(
            input,
            BigIntNumberPolicy::RejectNumber,
            result,
            function,
        )?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let schema = self.runtime_schema();
        let bigint = schema.reserve_gc_local(function).initialize(
            result
                .value()
                .cast_reference::<BigIntValue>(schema, function),
            function,
        );
        let limbs = schema.reserve_gc_local(function).initialize(
            schema
                .field(BigIntValueSchema::LIMBS)
                .read(&bigint, schema, function)
                .reference(),
            function,
        );
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I64Const(0));
        word.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        schema
            .array_type::<BigIntLimbArray>()
            .length(&limbs, schema, function);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .array_type::<BigIntLimbArray>()
            .read(&limbs, index, schema, function)
            .store_i64(word, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let negative = schema.reserve_i32_local(function);
        schema
            .field(BigIntValueSchema::NEGATIVE)
            .read(&bigint, schema, function)
            .store(negative, function);
        negative.load(function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        word.load(function);
        function.instruction(&Instruction::I64Sub);
        word.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(negative, function);
        schema.release_i32_local(index, function);
        limbs.clear(function);
        bigint.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_typed_array_element_write_from_locals(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        index: I64Local,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let kind = schema.reserve_i32_local(function);
        let word = schema.reserve_i64_local(function);
        let number = schema.reserve_i64_local(function);
        schema
            .field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(target, schema, function)
            .store(kind, function);
        function.instruction(&Instruction::I32Const(0));
        for kind_value in TypedArrayElementKind::ALL {
            if kind_value.content_type() == TypedArrayContentType::BigInt {
                kind.load(function);
                function.instruction(&Instruction::I32Const(kind_value.encode()));
                function.instruction(&Instruction::I32Eq);
                function.instruction(&Instruction::I32Or);
            }
        }
        self.open_frame(ControlFrameKind::If, function);
        self.emit_to_bigint_u64_word_from_value_locals(input, word, result, function)?;
        function.instruction(&Instruction::Else);
        self.emit_value_to_number_payload(input, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.value().scalar().load(function);
        number.store(function);
        for kind_value in TypedArrayElementKind::ALL {
            if kind_value.content_type() == TypedArrayContentType::BigInt {
                continue;
            }
            kind.load(function);
            function.instruction(&Instruction::I32Const(kind_value.encode()));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            match kind_value {
                TypedArrayElementKind::Float16 => {
                    self.emit_element_number_to_half(number, word, function)
                }
                TypedArrayElementKind::Float32 => {
                    number.load(function);
                    function.instruction(&Instruction::F64ReinterpretI64);
                    function.instruction(&Instruction::F32DemoteF64);
                    function.instruction(&Instruction::I32ReinterpretF32);
                    function.instruction(&Instruction::I64ExtendI32U);
                    word.store(function);
                }
                TypedArrayElementKind::Float64 => {
                    number.load(function);
                    word.store(function);
                }
                TypedArrayElementKind::Uint8Clamped => {
                    number.load(function);
                    function.instruction(&Instruction::F64ReinterpretI64);
                    function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
                    function.instruction(&Instruction::F64Max);
                    function.instruction(&Instruction::F64Const(Ieee64::from(255.0)));
                    function.instruction(&Instruction::F64Min);
                    function.instruction(&Instruction::F64Nearest);
                    function.instruction(&Instruction::I64TruncSatF64U);
                    word.store(function);
                }
                TypedArrayElementKind::Int8
                | TypedArrayElementKind::Int16
                | TypedArrayElementKind::Int32
                | TypedArrayElementKind::Uint8
                | TypedArrayElementKind::Uint16
                | TypedArrayElementKind::Uint32 => {
                    self.emit_to_uint32_i64_from_number_payload(number, word, function)
                }
                TypedArrayElementKind::BigInt64 | TypedArrayElementKind::BigUint64 => {
                    unreachable!("Number content type")
                }
            }
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // Conversion may detach or resize the buffer. This is the first view
        // acquisition and it happens even for a known invalid integer index.
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let access = self.acquire_typed_array_element_access(target, function);
        access.in_bounds.load(function);
        index.load(function);
        access.length.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_element_word_write(&access, index, word, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.set_normal(input, function);
        access.clear(schema, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(number, function);
        schema.release_i64_local(word, function);
        schema.release_i32_local(kind, function);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    fn emit_element_half_to_number(
        &mut self,
        half: I64Local,
        bits: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let sign = schema.reserve_i64_local(function);
        let exponent = schema.reserve_i64_local(function);
        let fraction = schema.reserve_i64_local(function);
        let binary32 = schema.reserve_i64_local(function);
        let normalized_exponent = schema.reserve_i64_local(function);
        self.emit_half_bits_to_f64_payload(
            half,
            sign,
            exponent,
            fraction,
            binary32,
            normalized_exponent,
            function,
        );
        function.instruction(&Instruction::I64ReinterpretF64);
        bits.store(function);
        for local in [normalized_exponent, binary32, fraction, exponent, sign] {
            schema.release_i64_local(local, function);
        }
    }
    fn emit_element_number_to_half(
        &mut self,
        bits: I64Local,
        half: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let sign = schema.reserve_i64_local(function);
        let exponent = schema.reserve_i64_local(function);
        let fraction = schema.reserve_i64_local(function);
        let rounded = schema.reserve_i64_local(function);
        let remainder = schema.reserve_i64_local(function);
        let significand = schema.reserve_i64_local(function);
        self.emit_f64_payload_to_half_bits_local(
            bits,
            half,
            sign,
            exponent,
            fraction,
            rounded,
            remainder,
            significand,
            function,
        );
        for local in [significand, remainder, rounded, fraction, exponent, sign] {
            schema.release_i64_local(local, function);
        }
    }
}
