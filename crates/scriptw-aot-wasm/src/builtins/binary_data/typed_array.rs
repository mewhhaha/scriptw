use super::*;
use crate::control_flow::SyncIteratorConsumer;

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_typed_array_subarray_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let out = s.reserve_completion(f);
        out.initialize(f);
        let pending = s.reserve_completion(f);
        let receiver = s.reserve_value_local(f);
        receiver.copy_from(self.body_entry_locals().expect("subarray").this_value(), f);
        let start_arg = s.reserve_value_local(f);
        let end_arg = s.reserve_value_local(f);
        let buffer = s.reserve_value_local(f);
        let ctor = s.reserve_value_local(f);
        let offset_arg = s.reserve_value_local(f);
        let count_arg = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &start_arg, f);
        self.emit_builtin_arg_to_value(1, &end_arg, f);
        let length = s.reserve_i64_local(f);
        let start = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        let offset = s.reserve_i64_local(f);
        let width = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let valid = s.reserve_i32_local(f);
        let kind = s.reserve_i32_local(f);
        let target_kind = s.reserve_i32_local(f);
        let tracking = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let source = self.emit_binary_require_ref::<TypedArrayObject>(
            &receiver,
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SUBARRAY_REQUIRES_TYPEDARRAY,
            &out,
            exit,
            f,
        )?;
        self.emit_typed_array_length_snapshot(&source, length, valid, f);
        self.emit_binary_relative_index(&start_arg, length, false, start, &pending, &out, exit, f)?;
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&source, s, f)
            .store(kind, f);
        f.instruction(&Instruction::I64Const(0));
        width.store(f);
        for element in TypedArrayElementKind::ALL {
            kind.load(f);
            f.instruction(&Instruction::I32Const(element.encode()));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I64Const(element.bytes_per_element() as i64));
            width.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        let view = s.reserve_gc_local(f).initialize(
            s.field(TypedArrayObjectSchema::VIEW)
                .read(&source, s, f)
                .reference(),
            f,
        );
        let owner = s.reserve_gc_local(f).initialize(
            s.field(BufferViewSchema::BUFFER)
                .read(&view, s, f)
                .reference(),
            f,
        );
        self.emit_binary_buffer_value(&owner, &buffer, f);
        s.field(BufferViewSchema::BYTE_OFFSET)
            .read(&view, s, f)
            .store_i64(offset, f);
        s.field(BufferViewSchema::LENGTH_TRACKING)
            .read(&view, s, f)
            .store(tracking, f);
        offset.load(f);
        start.load(f);
        width.load(f);
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64Add);
        offset.store(f);
        offset.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        offset_arg.scalar().store(f);
        offset_arg.set_number(offset_arg.scalar(), f);
        tracking.load(f);
        end_arg.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        tracking.store(f);
        tracking.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_relative_index(&end_arg, length, true, end, &pending, &out, exit, f)?;
        end.load(f);
        start.load(f);
        f.instruction(&Instruction::I64GtU);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        end.load(f);
        start.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::End);
        count.store(f);
        count.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        count_arg.scalar().store(f);
        count_arg.set_number(count_arg.scalar(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let default = self.emit_current_function_realm_typed_array_constructor(kind, f)?;
        ctor.copy_from(default.value(), f);
        let key = self.emit_binary_string_key("constructor", f)?;
        self.emit_object_read(&receiver, &receiver, &key, &pending, f)?;
        key.clear(f);
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        pending.value().tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_heap_object_like_tag_i32(pending.value().tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SUBARRAY_CONSTRUCTOR_PROPERTY_IS_NOT_AN_OBJECT,&out,exit,f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let constructor = s.reserve_value_local(f);
        constructor.copy_from(pending.value(), f);
        let symbol = s.reserve_gc_local(f).initialize(
            self.emit_well_known_symbol_reference(WellKnownSymbol::Species, f)?,
            f,
        );
        let key = PropertyKeyLocals::from_symbol(s, &symbol, f);
        self.emit_object_read(&constructor, &constructor, &key, &pending, f)?;
        key.clear(f);
        symbol.clear(f);
        constructor.clear(f);
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        pending.value().tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        pending.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        ctor.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_is_constructor_i32(&ctor, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SUBARRAY_SPECIES_IS_NOT_A_CONSTRUCTOR,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        tracking.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&buffer, &offset_arg], f);
        self.emit_binary_construct_typed_array(
            &ctor,
            &argv,
            TypedArrayCreateAccess::Read,
            None,
            &out,
            f,
        )?;
        argv.clear(f);
        f.instruction(&Instruction::Else);
        let argv = self.emit_pre_evaluated_arg_vector(&[&buffer, &offset_arg, &count_arg], f);
        self.emit_binary_construct_typed_array(
            &ctor,
            &argv,
            TypedArrayCreateAccess::Read,
            None,
            &out,
            f,
        )?;
        argv.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_binary_abrupt_exit(&out, &out, exit, f);
        let target = s
            .reserve_gc_local(f)
            .initialize(out.value().cast_reference::<TypedArrayObject>(s, f), f);
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&target, s, f)
            .store(target_kind, f);
        f.instruction(&Instruction::I32Const(0));
        valid.store(f);
        for a in TypedArrayElementKind::ALL {
            for b in TypedArrayElementKind::ALL {
                if a.content_type() == b.content_type() {
                    kind.load(f);
                    f.instruction(&Instruction::I32Const(a.encode()));
                    f.instruction(&Instruction::I32Eq);
                    target_kind.load(f);
                    f.instruction(&Instruction::I32Const(b.encode()));
                    f.instruction(&Instruction::I32Eq);
                    f.instruction(&Instruction::I32And);
                    valid.load(f);
                    f.instruction(&Instruction::I32Or);
                    valid.store(f);
                }
            }
        }
        valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SUBARRAY_SPECIES_CONTENT_TYPE_DIFFERS,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        target.clear(f);
        default.clear(f);
        owner.clear(f);
        view.clear(f);
        source.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&out, f);
        s.release_i32_local(tracking, f);
        s.release_i32_local(target_kind, f);
        s.release_i32_local(kind, f);
        s.release_i32_local(valid, f);
        s.release_i64_local(count, f);
        s.release_i64_local(width, f);
        s.release_i64_local(offset, f);
        s.release_i64_local(end, f);
        s.release_i64_local(start, f);
        s.release_i64_local(length, f);
        count_arg.clear(f);
        offset_arg.clear(f);
        ctor.clear(f);
        buffer.clear(f);
        end_arg.clear(f);
        start_arg.clear(f);
        receiver.clear(f);
        pending.clear(f);
        out.clear(f);
        Ok(())
    }
    pub(in crate::builtins) fn emit_typed_array_abstract_constructor_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let out = s.reserve_completion(f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::TYPEDARRAY_CANNOT_BE_CALLED_OR_CONSTRUCTED_DIRECTLY,
            &out,
            f,
        )?;
        self.completion().copy_from(&out, f);
        out.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_typed_array_accessor_builtin(
        &mut self,
        kind: TypedArrayAccessorKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let out = s.reserve_completion(f);
        out.initialize(f);
        let receiver = s.reserve_value_local(f);
        receiver.copy_from(
            self.body_entry_locals()
                .expect("TypedArray accessor")
                .this_value(),
            f,
        );
        let length = s.reserve_i64_local(f);
        let valid = s.reserve_i32_local(f);
        let number = s.reserve_i64_local(f);
        let element_kind = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let array = self.emit_binary_require_ref::<TypedArrayObject>(
            &receiver,
            RuntimeErrorMessage::TYPEDARRAY_ACCESSOR_REQUIRES_TYPEDARRAY,
            &out,
            exit,
            f,
        )?;
        self.emit_typed_array_length_snapshot(&array, length, valid, f);
        f.instruction(&Instruction::I64Const(0));
        number.store(f);
        valid.load(f);
        self.open_frame(ControlFrameKind::If, f);
        match kind {
            TypedArrayAccessorKind::Length => {
                length.load(f);
                number.store(f);
            }
            TypedArrayAccessorKind::ByteLength => {
                s.field(TypedArrayObjectSchema::ELEMENT_KIND)
                    .read(&array, s, f)
                    .store(element_kind, f);
                for element in TypedArrayElementKind::ALL {
                    element_kind.load(f);
                    f.instruction(&Instruction::I32Const(element.encode()));
                    f.instruction(&Instruction::I32Eq);
                    self.open_frame(ControlFrameKind::If, f);
                    length.load(f);
                    f.instruction(&Instruction::I64Const(element.bytes_per_element() as i64));
                    f.instruction(&Instruction::I64Mul);
                    number.store(f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                }
            }
            TypedArrayAccessorKind::ByteOffset => {
                let view = s.reserve_gc_local(f).initialize(
                    s.field(TypedArrayObjectSchema::VIEW)
                        .read(&array, s, f)
                        .reference(),
                    f,
                );
                s.field(BufferViewSchema::BYTE_OFFSET)
                    .read(&view, s, f)
                    .store_i64(number, f);
                view.clear(f);
            }
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        number.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.store(f);
        out.value().set_number(number, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&out, f);
        s.release_i32_local(element_kind, f);
        s.release_i64_local(number, f);
        s.release_i32_local(valid, f);
        s.release_i64_local(length, f);
        receiver.clear(f);
        out.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_typed_array_buffer_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let out = s.reserve_completion(f);
        out.initialize(f);
        let receiver = s.reserve_value_local(f);
        receiver.copy_from(
            self.body_entry_locals()
                .expect("TypedArray buffer")
                .this_value(),
            f,
        );
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let array = self.emit_binary_require_ref::<TypedArrayObject>(
            &receiver,
            RuntimeErrorMessage::TYPEDARRAY_ACCESSOR_REQUIRES_TYPEDARRAY,
            &out,
            exit,
            f,
        )?;
        let view = s.reserve_gc_local(f).initialize(
            s.field(TypedArrayObjectSchema::VIEW)
                .read(&array, s, f)
                .reference(),
            f,
        );
        let owner = s.reserve_gc_local(f).initialize(
            s.field(BufferViewSchema::BUFFER)
                .read(&view, s, f)
                .reference(),
            f,
        );
        self.emit_binary_buffer_value(&owner, out.value(), f);
        owner.clear(f);
        view.clear(f);
        array.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&out, f);
        receiver.clear(f);
        out.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_typed_array_tag_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let out = s.reserve_value_local(f);
        out.set_undefined(f);
        let receiver = self
            .body_entry_locals()
            .expect("TypedArray tag")
            .this_value();
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<TypedArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let array = s.reserve_gc_local(f).initialize(
            self.body_entry_locals()
                .expect("TypedArray tag")
                .this_value()
                .cast_reference::<TypedArrayObject>(s, f),
            f,
        );
        let kind = s.reserve_i32_local(f);
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&array, s, f)
            .store(kind, f);
        for element in TypedArrayElementKind::ALL {
            kind.load(f);
            f.instruction(&Instruction::I32Const(element.encode()));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            let name = match element {
                TypedArrayElementKind::Int8 => "Int8Array",
                TypedArrayElementKind::Uint8 => "Uint8Array",
                TypedArrayElementKind::Uint8Clamped => "Uint8ClampedArray",
                TypedArrayElementKind::Int16 => "Int16Array",
                TypedArrayElementKind::Uint16 => "Uint16Array",
                TypedArrayElementKind::Int32 => "Int32Array",
                TypedArrayElementKind::Uint32 => "Uint32Array",
                TypedArrayElementKind::Float16 => "Float16Array",
                TypedArrayElementKind::Float32 => "Float32Array",
                TypedArrayElementKind::Float64 => "Float64Array",
                TypedArrayElementKind::BigInt64 => "BigInt64Array",
                TypedArrayElementKind::BigUint64 => "BigUint64Array",
            };
            let string = s
                .reserve_gc_local(f)
                .initialize(self.emit_interned_string_reference(name, f)?, f);
            out.set_reference(&string, s, f);
            string.clear(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        s.release_i32_local(kind, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&out, f);
        out.clear(f);
        Ok(())
    }

    /// Actual Construct followed by concrete brand, `access` admission and live
    /// minimum length. Even an empty custom `ReadWrite` result must pass write
    /// admission; a `Read` result only needs a live in-bounds view.
    pub(in crate::builtins) fn emit_binary_construct_typed_array(
        &mut self,
        constructor: &ValueLocals,
        argv: &GcLocal<ValueArray>,
        access: TypedArrayCreateAccess,
        minimum: Option<I64Local>,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_function_or_proxy_construct_with_argv(
            constructor,
            constructor,
            argv,
            &pending,
            f,
        )?;
        result.copy_from(&pending, f);
        self.emit_binary_abrupt_exit(&pending, result, exit, f);
        let array = self.emit_binary_require_ref::<TypedArrayObject>(
            pending.value(),
            RuntimeErrorMessage::CONSTRUCTED_TARGET_IS_NOT_A_TYPED_ARRAY,
            result,
            exit,
            f,
        )?;
        match access {
            TypedArrayCreateAccess::Read => {
                self.emit_validate_typed_array_view(&array, length, &pending, f)?;
            }
            TypedArrayCreateAccess::ReadWrite => {
                self.emit_validate_typed_array_write_view(&array, length, &pending, f)?;
            }
        }
        self.emit_binary_abrupt_exit(&pending, result, exit, f);
        if let Some(minimum) = minimum {
            length.load(f);
            minimum.load(f);
            f.instruction(&Instruction::I64LtU);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_binary_type_error(
                RuntimeErrorMessage::TYPEDARRAY_LENGTH_OUT_OF_RANGE,
                result,
                exit,
                f,
            )?;
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        array.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(length, f);
        pending.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_typed_array_from_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let out = s.reserve_completion(f);
        out.initialize(f);
        let pending = s.reserve_completion(f);
        let ctor = s.reserve_value_local(f);
        ctor.copy_from(
            self.body_entry_locals()
                .expect("TypedArray.from")
                .this_value(),
            f,
        );
        let source = s.reserve_value_local(f);
        let mapper = s.reserve_value_local(f);
        let this_arg = s.reserve_value_local(f);
        let method = s.reserve_value_local(f);
        let boxed = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let target = s.reserve_value_local(f);
        let number = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &source, f);
        self.emit_builtin_arg_to_value(1, &mapper, f);
        self.emit_builtin_arg_to_value(2, &this_arg, f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let mapping = s.reserve_i32_local(f);
        let present = s.reserve_i32_local(f);
        let slot = s.reserve_i32_local(f);
        let list = s
            .reserve_gc_local::<ValueArray, Nullable>(f)
            .initialize_null(s, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_is_constructor_i32(&ctor, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::TYPEDARRAY_FROM_RECEIVER_IS_NOT_A_CONSTRUCTOR,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        mapper.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        mapping.store(f);
        mapping.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_callable_i32(&mapper, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::TYPEDARRAY_FROM_MAPPER_IS_NOT_CALLABLE,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_binary_optional_iterator(&source, &method, present, &pending, &out, exit, f)?;
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let collected =
            self.emit_binary_iterator_to_list(&source, &method, length, &pending, &out, exit, f)?;
        list.replace(collected.load(s, f).nullable(), f);
        collected.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_value_to_object_locals(&source, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        boxed.copy_from(pending.value(), f);
        let key = self.emit_binary_string_key("length", f)?;
        self.emit_object_read(&boxed, &boxed, &key, &pending, f)?;
        key.clear(f);
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        value.copy_from(pending.value(), f);
        self.emit_to_length_i64_from_value_locals(&value, length, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.scalar().store(f);
        number.set_number(number.scalar(), f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&number], f);
        self.emit_binary_construct_typed_array(
            &ctor,
            &argv,
            TypedArrayCreateAccess::ReadWrite,
            Some(length),
            &pending,
            f,
        )?;
        argv.clear(f);
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        target.copy_from(pending.value(), f);
        let array = s
            .reserve_gc_local(f)
            .initialize(target.cast_reference::<TypedArrayObject>(s, f), f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        let row = s.reserve_gc_local(f).initialize(
            s.array_type::<ValueArray>()
                .read(&list, slot, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>().read_into(&row, &value, s, f);
        row.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_typed_array_or_object_index_read_from_locals(&boxed, index, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        value.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        mapping.load(f);
        self.open_frame(ControlFrameKind::If, f);
        index.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.scalar().store(f);
        number.set_number(number.scalar(), f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&value, &number], f);
        self.emit_function_or_proxy_call_with_argv(&mapper, &this_arg, &argv, &pending, f)?;
        argv.clear(f);
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        value.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_typed_array_element_write_from_locals(&array, index, &value, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        out.set_normal(&target, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&out, f);
        list.clear(f);
        s.release_i32_local(slot, f);
        s.release_i32_local(present, f);
        s.release_i32_local(mapping, f);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        number.clear(f);
        target.clear(f);
        value.clear(f);
        boxed.clear(f);
        method.clear(f);
        this_arg.clear(f);
        mapper.clear(f);
        source.clear(f);
        ctor.clear(f);
        pending.clear(f);
        out.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_typed_array_of_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let out = s.reserve_completion(f);
        out.initialize(f);
        let pending = s.reserve_completion(f);
        let ctor = s.reserve_value_local(f);
        let number = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i32_local(f);
        let count = s.reserve_i32_local(f);
        let args = s.reserve_gc_local(f).initialize(
            self.body_entry_locals()
                .expect("TypedArray.of")
                .arguments()
                .load(s, f),
            f,
        );
        ctor.copy_from(
            self.body_entry_locals()
                .expect("TypedArray.of")
                .this_value(),
            f,
        );
        s.array_type::<ValueArray>().length(&args, s, f);
        count.store(f);
        count.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_is_constructor_i32(&ctor, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::TYPEDARRAY_OF_RECEIVER_IS_NOT_A_CONSTRUCTOR,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.scalar().store(f);
        number.set_number(number.scalar(), f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&number], f);
        self.emit_binary_construct_typed_array(
            &ctor,
            &argv,
            TypedArrayCreateAccess::ReadWrite,
            Some(length),
            &out,
            f,
        )?;
        argv.clear(f);
        self.emit_binary_abrupt_exit(&out, &out, exit, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(out.value().cast_reference::<TypedArrayObject>(s, f), f);
        f.instruction(&Instruction::I32Const(0));
        index.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, f);
        self.emit_argument_vector_entry_to_value(&args, index, &value, f);
        index.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        self.emit_typed_array_element_write_from_locals(&array, length, &value, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        array.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&out, f);
        s.release_i32_local(count, f);
        s.release_i32_local(index, f);
        s.release_i64_local(length, f);
        value.clear(f);
        number.clear(f);
        ctor.clear(f);
        args.clear(f);
        pending.clear(f);
        out.clear(f);
        Ok(())
    }
    fn emit_binary_optional_iterator(
        &mut self,
        input: &ValueLocals,
        method: &ValueLocals,
        present: I32Local,
        pending: &CompletionLocals,
        out: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.emit_value_to_object_locals(input, pending, f)?;
        self.emit_binary_abrupt_exit(pending, out, exit, f);
        let boxed = s.reserve_value_local(f);
        boxed.copy_from(pending.value(), f);
        let symbol = s.reserve_gc_local(f).initialize(
            self.emit_well_known_symbol_reference(WellKnownSymbol::Iterator, f)?,
            f,
        );
        let key = PropertyKeyLocals::from_symbol(s, &symbol, f);
        self.emit_object_read(&boxed, input, &key, pending, f)?;
        key.clear(f);
        symbol.clear(f);
        boxed.clear(f);
        self.emit_binary_abrupt_exit(pending, out, exit, f);
        method.copy_from(pending.value(), f);
        method.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        method.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        present.store(f);
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_callable_i32(method, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::TYPEDARRAY_FROM_ITERATOR_NEXT_MUST_BE_CALLABLE,
            out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    /// IteratorToList keeps values as rooted StoredValue rows. It performs no
    /// numeric conversion and introduces no IteratorClose on protocol failure.
    fn emit_binary_iterator_to_list(
        &mut self,
        input: &ValueLocals,
        method: &ValueLocals,
        length: I64Local,
        pending: &CompletionLocals,
        out: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<ValueArray>, EmitError> {
        let s = self.runtime_schema();
        let argv = self.emit_pre_evaluated_arg_vector(&[], f);
        self.emit_function_or_proxy_call_with_argv(method, input, &argv, pending, f)?;
        argv.clear(f);
        self.emit_binary_abrupt_exit(pending, out, exit, f);
        let iterator = self.emit_get_sync_iterator_direct(
            pending.value(),
            SyncIteratorConsumer::ArrayAccumulation,
            f,
        )?;
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        let filler = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&undefined, f), f);
        let size = s.reserve_i32_local(f);
        let count = s.reserve_i32_local(f);
        let cursor = s.reserve_i32_local(f);
        let done = s.reserve_i32_local(f);
        let value = s.reserve_value_local(f);
        f.instruction(&Instruction::I32Const(8));
        size.store(f);
        f.instruction(&Instruction::I32Const(0));
        count.store(f);
        let list = s.reserve_gc_local::<ValueArray, Nullable>(f).initialize(
            s.array_type::<ValueArray>()
                .filled(GcOperand::reference(&filler, s), size, f)
                .nullable(),
            f,
        );
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_sync_iterator_step_value(&iterator, done, &value, f)?;
        done.load(f);
        self.emit_branch_if_to_target(finished, f);
        count.load(f);
        f.instruction(&Instruction::I32Const(i32::MAX));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::TYPEDARRAY_LENGTH_OUT_OF_RANGE,
            out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        count.load(f);
        size.load(f);
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        size.load(f);
        f.instruction(&Instruction::I32Const(i32::MAX / 2));
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(i32::MAX));
        f.instruction(&Instruction::Else);
        size.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::End);
        size.store(f);
        let grown = s.reserve_gc_local(f).initialize(
            s.array_type::<ValueArray>()
                .filled(GcOperand::reference(&filler, s), size, f),
            f,
        );
        f.instruction(&Instruction::I32Const(0));
        cursor.store(f);
        let copied = self.open_frame(ControlFrameKind::Block, f);
        let copy = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(copied, f);
        let row = s.reserve_gc_local(f).initialize(
            s.array_type::<ValueArray>()
                .read(&list, cursor, s, f)
                .reference(),
            f,
        );
        s.array_type::<ValueArray>()
            .write(&grown, cursor, GcOperand::reference(&row, s), s, f);
        row.clear(f);
        cursor.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        cursor.store(f);
        self.emit_branch_to_target(copy, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        list.replace(grown.load(s, f).nullable(), f);
        grown.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let row = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&value, f), f);
        s.array_type::<ValueArray>()
            .write(&list, count, GcOperand::reference(&row, s), s, f);
        row.clear(f);
        count.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        count.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        count.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        let completed = s
            .reserve_gc_local(f)
            .initialize(list.load(s, f).require_non_null(f), f);
        list.clear(f);
        value.clear(f);
        s.release_i32_local(done, f);
        s.release_i32_local(cursor, f);
        s.release_i32_local(count, f);
        s.release_i32_local(size, f);
        filler.clear(f);
        undefined.clear(f);
        iterator.clear(f);
        Ok(completed)
    }

    pub(in crate::builtins) fn emit_binary_allocate_typed_array(
        &mut self,
        kind: TypedArrayElementKind,
        prototype: &ValueLocals,
        owner: &GcLocal<BufferOwner>,
        offset: I64Local,
        bytes: I64Local,
        tracking: I32Local,
        f: &mut Function,
    ) -> Result<GcLocal<TypedArrayObject>, EmitError> {
        let s = self.runtime_schema();
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(prototype), f)?,
            f,
        );
        let view = s.reserve_gc_local(f).initialize(
            s.struct_type::<BufferView>().construct(
                (
                    GcOperand::reference(owner, s),
                    GcOperand::i64_local(offset),
                    GcOperand::i64_local(bytes),
                    GcOperand::boolean_local(tracking),
                ),
                f,
            ),
            f,
        );
        let array = s.reserve_gc_local(f).initialize(
            s.struct_type::<TypedArrayObject>().construct(
                (
                    GcOperand::reference(&header, s),
                    GcOperand::reference(&view, s),
                    GcOperand::constant(kind),
                ),
                f,
            ),
            f,
        );
        view.clear(f);
        header.clear(f);
        Ok(array)
    }

    fn emit_binary_fresh_typed_array(
        &mut self,
        kind: TypedArrayElementKind,
        prototype: &ValueLocals,
        length: I64Local,
        out: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<TypedArrayObject>, EmitError> {
        let s = self.runtime_schema();
        let bytes = s.reserve_i64_local(f);
        let zero = s.reserve_i64_local(f);
        let fixed = s.reserve_i32_local(f);
        let buffer_prototype = s.reserve_value_local(f);
        length.load(f);
        f.instruction(&Instruction::I64Const(
            (i64::from(u32::MAX) - 7) / (kind.bytes_per_element() as i64),
        ));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::TYPEDARRAY_LENGTH_OUT_OF_RANGE,
            out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length.load(f);
        f.instruction(&Instruction::I64Const((kind.bytes_per_element() as i64)));
        f.instruction(&Instruction::I64Mul);
        bytes.store(f);
        f.instruction(&Instruction::I64Const(0));
        zero.store(f);
        f.instruction(&Instruction::I32Const(0));
        fixed.store(f);
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::ArrayBufferPrototype,
            &buffer_prototype,
            f,
        );
        realm.clear(f);
        let buffer = self.emit_binary_allocate_array_buffer(
            &buffer_prototype,
            bytes,
            bytes,
            fixed,
            false,
            out,
            exit,
            f,
        )?;
        let value = s.reserve_value_local(f);
        value.set_reference(&buffer, s, f);
        let owner = self.emit_binary_buffer_owner(&value, out, exit, f)?;
        let array =
            self.emit_binary_allocate_typed_array(kind, prototype, &owner, zero, bytes, fixed, f)?;
        owner.clear(f);
        value.clear(f);
        buffer.clear(f);
        buffer_prototype.clear(f);
        s.release_i32_local(fixed, f);
        s.release_i64_local(zero, f);
        s.release_i64_local(bytes, f);
        Ok(array)
    }

    pub(in crate::builtins) fn emit_typed_array_constructor_builtin(
        &mut self,
        kind: TypedArrayElementKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let out = s.reserve_completion(f);
        out.initialize(f);
        let pending = s.reserve_completion(f);
        let prototype = s.reserve_value_local(f);
        let source = s.reserve_value_local(f);
        let offset_arg = s.reserve_value_local(f);
        let length_arg = s.reserve_value_local(f);
        let method = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let new_target = s.reserve_value_local(f);
        new_target.copy_from(
            self.body_entry_locals()
                .expect("TypedArray constructor")
                .new_target(),
            f,
        );
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let offset = s.reserve_i64_local(f);
        let bytes = s.reserve_i64_local(f);
        let word = s.reserve_i64_local(f);
        let present = s.reserve_i32_local(f);
        let tracking = s.reserve_i32_local(f);
        let valid = s.reserve_i32_local(f);
        let source_kind = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_binary_require_new_target(
            &new_target,
            RuntimeErrorMessage::TYPEDARRAY_CONSTRUCTOR_REQUIRES_NEW,
            &out,
            exit,
            f,
        )?;
        self.emit_builtin_arg_to_value(0, &source, f);
        self.emit_builtin_arg_to_value(1, &offset_arg, f);
        self.emit_builtin_arg_to_value(2, &length_arg, f);
        self.emit_is_heap_object_like_tag_i32(source.tag(), f);
        self.open_frame(ControlFrameKind::If, f);
        // Object initialization allocates its ordinary header before acquiring the source.
        self.emit_get_prototype_from_constructor(
            &new_target,
            OrdinaryDefaultPrototype::TypedArray(kind),
            &pending,
            f,
        )?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        prototype.copy_from(pending.value(), f);
        source.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<TypedArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let from = s
            .reserve_gc_local(f)
            .initialize(source.cast_reference::<TypedArrayObject>(s, f), f);
        self.emit_validate_typed_array_view(&from, length, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        let array = self.emit_binary_fresh_typed_array(kind, &prototype, length, &out, exit, f)?;
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&from, s, f)
            .store(source_kind, f);
        source_kind.load(f);
        f.instruction(&Instruction::I32Const(kind.encode()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let copied = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(copied, f);
        self.emit_typed_array_element_bits_read(&from, index, word, valid, f);
        self.emit_typed_array_element_bits_write(&array, index, word, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I32Const(0));
        valid.store(f);
        for source_type in TypedArrayElementKind::ALL {
            if source_type.content_type() == kind.content_type() {
                source_kind.load(f);
                f.instruction(&Instruction::I32Const(source_type.encode()));
                f.instruction(&Instruction::I32Eq);
                valid.load(f);
                f.instruction(&Instruction::I32Or);
                valid.store(f);
            }
        }
        valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::TYPEDARRAY_CONSTRUCTOR_SOURCE_AND_TARGET_CONTENT_TYPES_DIFFER,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let copied = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(copied, f);
        self.emit_typed_array_element_read_from_locals(&from, index, &value, f)?;
        self.emit_typed_array_element_write_from_locals(&array, index, &value, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        out.value().set_reference(&array, s, f);
        array.clear(f);
        from.clear(f);
        f.instruction(&Instruction::Else);
        source.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayBuffer>(GcNullability::NonNullable)
                .heap_type,
        ));
        source.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<SharedArrayBuffer>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        let owner = self.emit_binary_buffer_owner(&source, &out, exit, f)?;
        self.emit_to_index_i64_from_value_locals(
            &offset_arg,
            offset,
            RuntimeErrorMessage::TYPEDARRAY_BYTEOFFSET_OUT_OF_RANGE,
            &pending,
            f,
        )?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        offset.load(f);
        f.instruction(&Instruction::I64Const((kind.bytes_per_element() as i64)));
        f.instruction(&Instruction::I64RemU);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::TYPEDARRAY_BYTEOFFSET_MUST_BE_ALIGNED,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_binary_buffer_resizable_i32(&owner, f);
        tracking.store(f);
        length_arg.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I32Const(0));
        tracking.store(f);
        self.emit_to_index_i64_from_value_locals(
            &length_arg,
            length,
            RuntimeErrorMessage::TYPEDARRAY_LENGTH_OUT_OF_RANGE,
            &pending,
            f,
        )?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        length.load(f);
        f.instruction(&Instruction::I64Const(
            9_007_199_254_740_991 / (kind.bytes_per_element() as i64),
        ));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::TYPEDARRAY_LENGTH_OUT_OF_RANGE,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length.load(f);
        f.instruction(&Instruction::I64Const((kind.bytes_per_element() as i64)));
        f.instruction(&Instruction::I64Mul);
        bytes.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let access = self.emit_binary_buffer_access(&owner, f);
        access.valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::TYPEDARRAY_BACKING_BUFFER_IS_DETACHED,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        offset.load(f);
        access.length.load(f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::TYPEDARRAY_BYTEOFFSET_OUT_OF_RANGE,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length_arg.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        tracking.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        access.length.load(f);
        f.instruction(&Instruction::I64Const((kind.bytes_per_element() as i64)));
        f.instruction(&Instruction::I64RemU);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::TYPEDARRAY_BYTELENGTH_MUST_BE_ALIGNED,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        access.length.load(f);
        offset.load(f);
        f.instruction(&Instruction::I64Sub);
        bytes.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        bytes.load(f);
        access.length.load(f);
        offset.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::TYPEDARRAY_BYTELENGTH_OUT_OF_RANGE,
            &out,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let array = self.emit_binary_allocate_typed_array(
            kind, &prototype, &owner, offset, bytes, tracking, f,
        )?;
        out.value().set_reference(&array, s, f);
        array.clear(f);
        access.clear(s, f);
        owner.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_binary_optional_iterator(&source, &method, present, &pending, &out, exit, f)?;
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let list =
            self.emit_binary_iterator_to_list(&source, &method, length, &pending, &out, exit, f)?;
        let array = self.emit_binary_fresh_typed_array(kind, &prototype, length, &out, exit, f)?;
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        let slot = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        let row = s.reserve_gc_local(f).initialize(
            s.array_type::<ValueArray>()
                .read(&list, slot, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>().read_into(&row, &value, s, f);
        row.clear(f);
        s.release_i32_local(slot, f);
        self.emit_typed_array_element_write_from_locals(&array, index, &value, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        out.value().set_reference(&array, s, f);
        array.clear(f);
        list.clear(f);
        f.instruction(&Instruction::Else);
        let key = self.emit_binary_string_key("length", f)?;
        self.emit_object_read(&source, &source, &key, &pending, f)?;
        key.clear(f);
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        value.copy_from(pending.value(), f);
        self.emit_to_length_i64_from_value_locals(&value, length, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        let array = self.emit_binary_fresh_typed_array(kind, &prototype, length, &out, exit, f)?;
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        self.emit_typed_array_or_object_index_read_from_locals(&source, index, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        value.copy_from(pending.value(), f);
        self.emit_typed_array_element_write_from_locals(&array, index, &value, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        out.value().set_reference(&array, s, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        // Scalar length conversion precedes GetPrototypeFromConstructor.
        self.emit_to_index_i64_from_value_locals(
            &source,
            length,
            RuntimeErrorMessage::TYPEDARRAY_LENGTH_OUT_OF_RANGE,
            &pending,
            f,
        )?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        self.emit_get_prototype_from_constructor(
            &new_target,
            OrdinaryDefaultPrototype::TypedArray(kind),
            &pending,
            f,
        )?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        prototype.copy_from(pending.value(), f);
        let array = self.emit_binary_fresh_typed_array(kind, &prototype, length, &out, exit, f)?;
        out.value().set_reference(&array, s, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&out, f);
        s.release_i32_local(source_kind, f);
        s.release_i32_local(valid, f);
        s.release_i32_local(tracking, f);
        s.release_i32_local(present, f);
        s.release_i64_local(word, f);
        s.release_i64_local(bytes, f);
        s.release_i64_local(offset, f);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        new_target.clear(f);
        value.clear(f);
        method.clear(f);
        length_arg.clear(f);
        offset_arg.clear(f);
        source.clear(f);
        prototype.clear(f);
        pending.clear(f);
        out.clear(f);
        Ok(())
    }
}
