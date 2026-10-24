use super::storage::CollectionCursorLocals;
use super::*;
impl FunctionBuilder<'_> {
    fn emit_collection_map_iterator(
        &mut self,
        kind: MapIterationKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_collection_map_receiver(f)?;
        let header =
            self.emit_collection_header(NonArrayRealmIntrinsicSlot::MapIteratorPrototype, f)?;
        let iterator = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapIteratorObject>().construct(
                (
                    GcOperand::reference(&header, s),
                    GcOperand::nullable_reference(&record, s),
                    GcOperand::i64(0),
                    GcOperand::boolean(false),
                    GcOperand::constant(kind),
                    GcOperand::i64(0),
                    GcOperand::i64(-1),
                ),
                f,
            ),
            f,
        );
        let value = s.reserve_value_local(f);
        value.set_reference(&iterator, s, f);
        self.completion().set_normal(&value, f);
        value.clear(f);
        iterator.clear(f);
        header.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_map_prototype_keys(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_collection_map_iterator(MapIterationKind::Key, f)
    }
    pub(crate) fn emit_map_prototype_values(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_collection_map_iterator(MapIterationKind::Value, f)
    }
    pub(crate) fn emit_map_prototype_entries(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_collection_map_iterator(MapIterationKind::KeyAndValue, f)
    }
    pub(crate) fn emit_map_iterator_next(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let key = s.reserve_value_local(f);
        let result = s.reserve_completion(f);
        let cursor = CollectionCursorLocals::reserve(s, f);
        let found = s.reserve_i32_local(f);
        let kind = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        result.initialize(f);
        value.set_undefined(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<MapIteratorObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_collection_receiver_type_error(
            &receiver,
            StrongCollectionReceiverKind::MapIterator,
            &result,
            f,
        )?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let iterator = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<MapIteratorObject>(s, f), f);
        let maybe = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapIteratorObject>()
                .field(MapIteratorObjectSchema::MAP)
                .read(&iterator, s, f)
                .reference(),
            f,
        );
        maybe.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_iterator_result_object_from_locals(&value, true, &result, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let record = s
            .reserve_gc_local(f)
            .initialize(maybe.load(s, f).require_non_null(f), f);
        s.struct_type::<MapIteratorObject>()
            .field(MapIteratorObjectSchema::NEXT_INDEX)
            .read(&iterator, s, f)
            .store_i64(cursor.index, f);
        s.field(MapIteratorObjectSchema::NEXT_ORDINAL)
            .read(&iterator, s, f)
            .store_i64(cursor.next_ordinal, f);
        s.field(MapIteratorObjectSchema::VERSION)
            .read(&iterator, s, f)
            .store_i64(cursor.version, f);
        self.emit_collection_next_map(&record, &cursor, found, f)?;
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_collection_read_map(&record, cursor.index, &key, &value, f);
        self.emit_increment_local(cursor.index, 1, f);
        s.struct_type::<MapIteratorObject>()
            .field(MapIteratorObjectSchema::NEXT_INDEX)
            .write(&iterator, GcOperand::i64_local(cursor.index), s, f);
        s.field(MapIteratorObjectSchema::NEXT_ORDINAL).write(
            &iterator,
            GcOperand::i64_local(cursor.next_ordinal),
            s,
            f,
        );
        s.field(MapIteratorObjectSchema::VERSION).write(
            &iterator,
            GcOperand::i64_local(cursor.version),
            s,
            f,
        );
        s.struct_type::<MapIteratorObject>()
            .field(MapIteratorObjectSchema::KIND)
            .read(&iterator, s, f)
            .store(kind, f);
        kind.load(f);
        f.instruction(&Instruction::I32Const(GcI32Constant::encode(
            MapIterationKind::Key,
        )));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        value.copy_from(&key, f);
        self.emit_iterator_result_object_from_locals(&value, false, &result, f)?;
        f.instruction(&Instruction::Else);
        kind.load(f);
        f.instruction(&Instruction::I32Const(GcI32Constant::encode(
            MapIterationKind::Value,
        )));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_iterator_result_object_from_locals(&value, false, &result, f)?;
        f.instruction(&Instruction::Else);
        kind.load(f);
        f.instruction(&Instruction::I32Const(GcI32Constant::encode(
            MapIterationKind::KeyAndValue,
        )));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let arguments = self.emit_pre_evaluated_arg_vector(&[&key, &value], f);
        let pair = self.emit_array_from_argument_list(&arguments, f)?;
        value.set_reference(&pair, s, f);
        pair.clear(f);
        arguments.clear(f);
        self.emit_iterator_result_object_from_locals(&value, false, &result, f)?;
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        s.struct_type::<MapIteratorObject>()
            .field(MapIteratorObjectSchema::DONE)
            .write(&iterator, GcOperand::boolean(true), s, f);
        s.struct_type::<MapIteratorObject>()
            .field(MapIteratorObjectSchema::MAP)
            .write(&iterator, GcOperand::null(s), s, f);
        self.emit_iterator_result_object_from_locals(&value, true, &result, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        record.clear(f);
        maybe.clear(f);
        iterator.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&result, f);
        s.release_i32_local(kind, f);
        s.release_i32_local(found, f);
        cursor.release(s, f);
        result.clear(f);
        key.clear(f);
        value.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(crate) fn emit_map_prototype_for_each(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_collection_map_receiver(f)?;
        let callback = s.reserve_value_local(f);
        let this = s.reserve_value_local(f);
        let receiver = s.reserve_value_local(f);
        let key = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let cursor = CollectionCursorLocals::reserve(s, f);
        let found = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &callback, f);
        self.emit_builtin_arg_to_value(1, &this, f);
        receiver.set_reference(&record, s, f);
        self.emit_collection_assert_callable(
            &callback,
            RuntimeErrorMessage::MAP_PROTOTYPE_FOREACH_CALLBACK_MUST_BE_CALLABLE,
            f,
        )?;
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_collection_next_map(&record, &cursor, found, f)?;
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        self.emit_collection_read_map(&record, cursor.index, &key, &value, f);
        self.emit_increment_local(cursor.index, 1, f);
        let args = self.emit_pre_evaluated_arg_vector(&[&value, &key, &receiver], f);
        self.emit_function_or_proxy_call_with_argv(&callback, &this, &args, &pending, f)?;
        args.clear(f);
        self.emit_collection_propagate(&pending, f);
        key.set_undefined(f);
        value.set_undefined(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.set_undefined(f);
        self.completion().set_normal(&value, f);
        s.release_i32_local(found, f);
        cursor.release(s, f);
        pending.clear(f);
        value.clear(f);
        key.clear(f);
        receiver.clear(f);
        this.clear(f);
        callback.clear(f);
        record.clear(f);
        Ok(())
    }
    fn emit_collection_set_iterator(
        &mut self,
        kind: SetIterationKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_collection_set_receiver(f)?;
        let header =
            self.emit_collection_header(NonArrayRealmIntrinsicSlot::SetIteratorPrototype, f)?;
        let iterator = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetIteratorObject>().construct(
                (
                    GcOperand::reference(&header, s),
                    GcOperand::nullable_reference(&record, s),
                    GcOperand::i64(0),
                    GcOperand::boolean(false),
                    GcOperand::constant(kind),
                    GcOperand::i64(0),
                    GcOperand::i64(-1),
                ),
                f,
            ),
            f,
        );
        let value = s.reserve_value_local(f);
        value.set_reference(&iterator, s, f);
        self.completion().set_normal(&value, f);
        value.clear(f);
        iterator.clear(f);
        header.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_set_prototype_values(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_collection_set_iterator(SetIterationKind::Value, f)
    }
    pub(crate) fn emit_set_prototype_entries(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_collection_set_iterator(SetIterationKind::KeyAndValue, f)
    }
    pub(crate) fn emit_set_iterator_next(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let key = s.reserve_value_local(f);
        let result = s.reserve_completion(f);
        let cursor = CollectionCursorLocals::reserve(s, f);
        let found = s.reserve_i32_local(f);
        let kind = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        result.initialize(f);
        value.set_undefined(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<SetIteratorObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_collection_receiver_type_error(
            &receiver,
            StrongCollectionReceiverKind::SetIterator,
            &result,
            f,
        )?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let iterator = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<SetIteratorObject>(s, f), f);
        let maybe = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetIteratorObject>()
                .field(SetIteratorObjectSchema::SET)
                .read(&iterator, s, f)
                .reference(),
            f,
        );
        maybe.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_iterator_result_object_from_locals(&value, true, &result, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let record = s
            .reserve_gc_local(f)
            .initialize(maybe.load(s, f).require_non_null(f), f);
        s.struct_type::<SetIteratorObject>()
            .field(SetIteratorObjectSchema::NEXT_INDEX)
            .read(&iterator, s, f)
            .store_i64(cursor.index, f);
        s.field(SetIteratorObjectSchema::NEXT_ORDINAL)
            .read(&iterator, s, f)
            .store_i64(cursor.next_ordinal, f);
        s.field(SetIteratorObjectSchema::VERSION)
            .read(&iterator, s, f)
            .store_i64(cursor.version, f);
        self.emit_collection_next_set(&record, &cursor, found, f)?;
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_collection_read_set(&record, cursor.index, &value, f);
        key.copy_from(&value, f);
        self.emit_increment_local(cursor.index, 1, f);
        s.struct_type::<SetIteratorObject>()
            .field(SetIteratorObjectSchema::NEXT_INDEX)
            .write(&iterator, GcOperand::i64_local(cursor.index), s, f);
        s.field(SetIteratorObjectSchema::NEXT_ORDINAL).write(
            &iterator,
            GcOperand::i64_local(cursor.next_ordinal),
            s,
            f,
        );
        s.field(SetIteratorObjectSchema::VERSION).write(
            &iterator,
            GcOperand::i64_local(cursor.version),
            s,
            f,
        );
        s.struct_type::<SetIteratorObject>()
            .field(SetIteratorObjectSchema::KIND)
            .read(&iterator, s, f)
            .store(kind, f);
        kind.load(f);
        f.instruction(&Instruction::I32Const(GcI32Constant::encode(
            SetIterationKind::Value,
        )));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_iterator_result_object_from_locals(&value, false, &result, f)?;
        f.instruction(&Instruction::Else);
        kind.load(f);
        f.instruction(&Instruction::I32Const(GcI32Constant::encode(
            SetIterationKind::KeyAndValue,
        )));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let arguments = self.emit_pre_evaluated_arg_vector(&[&key, &value], f);
        let pair = self.emit_array_from_argument_list(&arguments, f)?;
        value.set_reference(&pair, s, f);
        pair.clear(f);
        arguments.clear(f);
        self.emit_iterator_result_object_from_locals(&value, false, &result, f)?;
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        s.struct_type::<SetIteratorObject>()
            .field(SetIteratorObjectSchema::DONE)
            .write(&iterator, GcOperand::boolean(true), s, f);
        s.struct_type::<SetIteratorObject>()
            .field(SetIteratorObjectSchema::SET)
            .write(&iterator, GcOperand::null(s), s, f);
        self.emit_iterator_result_object_from_locals(&value, true, &result, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        record.clear(f);
        maybe.clear(f);
        iterator.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&result, f);
        s.release_i32_local(kind, f);
        s.release_i32_local(found, f);
        cursor.release(s, f);
        result.clear(f);
        key.clear(f);
        value.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(crate) fn emit_set_prototype_for_each(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_collection_set_receiver(f)?;
        let callback = s.reserve_value_local(f);
        let this = s.reserve_value_local(f);
        let receiver = s.reserve_value_local(f);
        let key = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let cursor = CollectionCursorLocals::reserve(s, f);
        let found = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &callback, f);
        self.emit_builtin_arg_to_value(1, &this, f);
        receiver.set_reference(&record, s, f);
        self.emit_collection_assert_callable(
            &callback,
            RuntimeErrorMessage::SET_PROTOTYPE_FOREACH_CALLBACK_MUST_BE_CALLABLE,
            f,
        )?;
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_collection_next_set(&record, &cursor, found, f)?;
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        self.emit_collection_read_set(&record, cursor.index, &value, f);
        key.copy_from(&value, f);
        self.emit_increment_local(cursor.index, 1, f);
        let args = self.emit_pre_evaluated_arg_vector(&[&value, &key, &receiver], f);
        self.emit_function_or_proxy_call_with_argv(&callback, &this, &args, &pending, f)?;
        args.clear(f);
        self.emit_collection_propagate(&pending, f);
        key.set_undefined(f);
        value.set_undefined(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.set_undefined(f);
        self.completion().set_normal(&value, f);
        s.release_i32_local(found, f);
        cursor.release(s, f);
        pending.clear(f);
        value.clear(f);
        key.clear(f);
        receiver.clear(f);
        this.clear(f);
        callback.clear(f);
        record.clear(f);
        Ok(())
    }
}
