use super::storage::CollectionCursorLocals;
use super::*;
impl FunctionBuilder<'_> {
    pub(crate) fn emit_map_constructor(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let source = s.reserve_value_local(f);
        let new_target = s.reserve_value_local(f);
        let receiver = s.reserve_value_local(f);
        let adder = s.reserve_value_local(f);
        let entry = s.reserve_value_local(f);
        let key = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        let done = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &source, f);
        self.compile_new_target_to_locals(&new_target, f)?;
        new_target.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::MAP_CONSTRUCTOR_REQUIRES_NEW,
            &pending,
            f,
        )?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_get_prototype_from_constructor(
            &new_target,
            OrdinaryDefaultPrototype::Map,
            &pending,
            f,
        )?;
        self.emit_collection_propagate(&pending, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(pending.value()), f)?,
            f,
        );
        let record = self.emit_collection_alloc_map(&header, f);
        receiver.set_reference(&record, s, f);
        output.set_normal(&receiver, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        source.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        source.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        self.emit_branch_if_to_target(exit, f);
        self.emit_collection_get(&receiver, "set", &pending, f)?;
        self.emit_collection_propagate(&pending, f);
        adder.copy_from(pending.value(), f);
        self.emit_collection_assert_callable(
            &adder,
            RuntimeErrorMessage::MAP_CONSTRUCTOR_SET_METHOD_IS_NOT_CALLABLE,
            f,
        )?;
        let iterator =
            self.emit_get_sync_iterator(&source, SyncIteratorConsumer::MapConstructor, f)?;
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_sync_iterator_step_value(&iterator, done, &entry, f)?;
        done.load(f);
        self.emit_branch_if_to_target(exit, f);
        self.emit_is_heap_object_like_tag_i32(entry.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::MAP_CONSTRUCTOR_ITERATOR_VALUE_MUST_BE_AN_OBJECT,
            &pending,
            f,
        )?;
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_collection_get(&entry, "0", &pending, f)?;
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        key.copy_from(pending.value(), f);
        self.emit_collection_get(&entry, "1", &pending, f)?;
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        value.copy_from(pending.value(), f);
        let args = self.emit_pre_evaluated_arg_vector(&[&key, &value], f);
        self.emit_function_or_proxy_call_with_argv(&adder, &receiver, &args, &pending, f)?;
        args.clear(f);
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        entry.set_undefined(f);
        key.set_undefined(f);
        value.set_undefined(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        iterator.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        record.clear(f);
        header.clear(f);
        s.release_i32_local(done, f);
        output.clear(f);
        pending.clear(f);
        value.clear(f);
        key.clear(f);
        entry.clear(f);
        adder.clear(f);
        receiver.clear(f);
        new_target.clear(f);
        source.clear(f);
        Ok(())
    }
    pub(crate) fn emit_set_constructor(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let source = s.reserve_value_local(f);
        let new_target = s.reserve_value_local(f);
        let receiver = s.reserve_value_local(f);
        let adder = s.reserve_value_local(f);
        let entry = s.reserve_value_local(f);
        let key = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        let done = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &source, f);
        self.compile_new_target_to_locals(&new_target, f)?;
        new_target.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::SET_CONSTRUCTOR_REQUIRES_NEW,
            &pending,
            f,
        )?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_get_prototype_from_constructor(
            &new_target,
            OrdinaryDefaultPrototype::Set,
            &pending,
            f,
        )?;
        self.emit_collection_propagate(&pending, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(pending.value()), f)?,
            f,
        );
        let record = self.emit_collection_alloc_set(&header, f);
        receiver.set_reference(&record, s, f);
        output.set_normal(&receiver, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        source.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        source.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        self.emit_branch_if_to_target(exit, f);
        self.emit_collection_get(&receiver, "add", &pending, f)?;
        self.emit_collection_propagate(&pending, f);
        adder.copy_from(pending.value(), f);
        self.emit_collection_assert_callable(
            &adder,
            RuntimeErrorMessage::SET_CONSTRUCTOR_ADD_METHOD_IS_NOT_CALLABLE,
            f,
        )?;
        let iterator =
            self.emit_get_sync_iterator(&source, SyncIteratorConsumer::SetConstructor, f)?;
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_sync_iterator_step_value(&iterator, done, &entry, f)?;
        done.load(f);
        self.emit_branch_if_to_target(exit, f);
        let args = self.emit_pre_evaluated_arg_vector(&[&entry], f);
        self.emit_function_or_proxy_call_with_argv(&adder, &receiver, &args, &pending, f)?;
        args.clear(f);
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        entry.set_undefined(f);
        key.set_undefined(f);
        value.set_undefined(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        iterator.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        record.clear(f);
        header.clear(f);
        s.release_i32_local(done, f);
        output.clear(f);
        pending.clear(f);
        value.clear(f);
        key.clear(f);
        entry.clear(f);
        adder.clear(f);
        receiver.clear(f);
        new_target.clear(f);
        source.clear(f);
        Ok(())
    }
    pub(crate) fn emit_object_from_entries(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let source = s.reserve_value_local(f);
        let object = s.reserve_value_local(f);
        let entry = s.reserve_value_local(f);
        let key_value = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        let done = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &source, f);
        let header = self.emit_collection_header(NonArrayRealmIntrinsicSlot::ObjectPrototype, f)?;
        object.set_reference(&header, s, f);
        output.set_normal(&object, f);
        let iterator =
            self.emit_get_sync_iterator(&source, SyncIteratorConsumer::ObjectFromEntries, f)?;
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_sync_iterator_step_value(&iterator, done, &entry, f)?;
        done.load(f);
        self.emit_branch_if_to_target(exit, f);
        self.emit_is_heap_object_like_tag_i32(entry.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::OBJECT_FROMENTRIES_ITERATOR_VALUE_MUST_BE_AN_OBJECT,
            &pending,
            f,
        )?;
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_collection_get(&entry, "0", &pending, f)?;
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        key_value.copy_from(pending.value(), f);
        self.emit_collection_get(&entry, "1", &pending, f)?;
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        value.copy_from(pending.value(), f);
        self.emit_value_to_property_key_completion(&key_value, &pending, f)?;
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        // The completed value is already String/Symbol: this admission performs
        // no second observable conversion and cannot leave the Close owner.
        let key = self.emit_value_to_property_key_locals(pending.value(), f)?;
        self.emit_create_data_property_or_throw(&object, &key, &value, &pending, f)?;
        key.clear(f);
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        entry.set_undefined(f);
        key_value.set_undefined(f);
        value.set_undefined(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        iterator.clear(f);
        header.clear(f);
        s.release_i32_local(done, f);
        output.clear(f);
        pending.clear(f);
        value.clear(f);
        key_value.clear(f);
        entry.clear(f);
        object.clear(f);
        source.clear(f);
        Ok(())
    }
    pub(crate) fn emit_map_group_by(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_collection_group_by(GroupByResult::Map, f)
    }
    pub(crate) fn emit_object_group_by(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_collection_group_by(GroupByResult::Object, f)
    }
    fn emit_collection_group_by(
        &mut self,
        mode: GroupByResult,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let source = s.reserve_value_local(f);
        let callback = s.reserve_value_local(f);
        let key = s.reserve_value_local(f);
        let item = s.reserve_value_local(f);
        let list_value = s.reserve_value_local(f);
        let number = s.reserve_value_local(f);
        let undefined = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        let position = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let done = s.reserve_i32_local(f);
        let found = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &source, f);
        self.emit_builtin_arg_to_value(1, &callback, f);
        undefined.set_undefined(f);
        output.initialize(f);
        source.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        source.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            match mode {
                GroupByResult::Map => {
                    RuntimeErrorMessage::MAP_GROUPBY_ITEMS_CANNOT_BE_NULL_OR_UNDEFINED
                }
                GroupByResult::Object => {
                    RuntimeErrorMessage::OBJECT_GROUPBY_ITEMS_CANNOT_BE_NULL_OR_UNDEFINED
                }
            },
            &pending,
            f,
        )?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_collection_assert_callable(
            &callback,
            match mode {
                GroupByResult::Map => RuntimeErrorMessage::MAP_GROUPBY_CALLBACK_MUST_BE_CALLABLE,
                GroupByResult::Object => {
                    RuntimeErrorMessage::OBJECT_GROUPBY_CALLBACK_MUST_BE_CALLABLE
                }
            },
            f,
        )?;
        let header = self.emit_collection_header(NonArrayRealmIntrinsicSlot::MapPrototype, f)?;
        let groups = self.emit_collection_alloc_map(&header, f);
        let iterator = self.emit_get_sync_iterator(
            &source,
            match mode {
                GroupByResult::Map => SyncIteratorConsumer::MapGroupBy,
                GroupByResult::Object => SyncIteratorConsumer::ObjectGroupBy,
            },
            f,
        )?;
        f.instruction(&Instruction::I64Const(0));
        position.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        position.load(f);
        f.instruction(&Instruction::I64Const(9_007_199_254_740_991));
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            match mode {
                GroupByResult::Map => {
                    RuntimeErrorMessage::MAP_GROUPBY_ITERATOR_PRODUCED_TOO_MANY_VALUES
                }
                GroupByResult::Object => {
                    RuntimeErrorMessage::OBJECT_GROUPBY_ITERATOR_PRODUCED_TOO_MANY_VALUES
                }
            },
            &pending,
            f,
        )?;
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_sync_iterator_step_value(&iterator, done, &item, f)?;
        done.load(f);
        self.emit_branch_if_to_target(finished, f);
        position.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.scalar().store(f);
        number.set_number(number.scalar(), f);
        let args = self.emit_pre_evaluated_arg_vector(&[&item, &number], f);
        self.emit_function_or_proxy_call_with_argv(&callback, &undefined, &args, &pending, f)?;
        args.clear(f);
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        key.copy_from(pending.value(), f);
        match mode {
            GroupByResult::Map => self.emit_collection_normalize_zero(&key, f),
            GroupByResult::Object => {
                self.emit_value_to_property_key_completion(&key, &pending, f)?;
                self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
                key.copy_from(pending.value(), f);
            }
        }
        let prepared = self.emit_collection_prepare_key(&key, f);
        self.emit_collection_find_prepared_map(&groups, &prepared, index, found, f)?;
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_collection_read_map(&groups, index, &key, &list_value, f);
        let list = s
            .reserve_gc_local(f)
            .initialize(list_value.cast_reference::<ArrayObject>(s, f), f);
        let length = s.reserve_i64_local(f);
        s.struct_type::<ArrayObject>()
            .field(ArrayObjectSchema::LENGTH)
            .read(&list, s, f)
            .store_i64(length, f);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.scalar().store(f);
        number.set_number(number.scalar(), f);
        self.emit_value_to_string_payload(&number, &pending, f)?;
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        let text = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        let property = PropertyKeyLocals::from_string(s, &text, f);
        self.emit_create_data_property_or_throw(&list_value, &property, &item, &pending, f)?;
        property.clear(f);
        text.clear(f);
        s.release_i64_local(length, f);
        list.clear(f);
        self.emit_collection_close_abrupt(&iterator, &pending, &output, exit, f)?;
        f.instruction(&Instruction::Else);
        let args = self.emit_pre_evaluated_arg_vector(&[&item], f);
        let list = self.emit_array_from_argument_list(&args, f)?;
        list_value.set_reference(&list, s, f);
        self.emit_collection_put_prepared_map(&groups, &prepared, &list_value, f)?;
        list.clear(f);
        args.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        prepared.release(self, f);
        self.emit_increment_local(position, 1, f);
        item.set_undefined(f);
        key.set_undefined(f);
        list_value.set_undefined(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        match mode {
            GroupByResult::Map => {
                key.set_reference(&groups, s, f);
                output.set_normal(&key, f);
            }
            GroupByResult::Object => {
                let object = s
                    .reserve_gc_local(f)
                    .initialize(self.emit_alloc_plain_object_with_prototype(None, f)?, f);
                let object_value = s.reserve_value_local(f);
                object_value.set_reference(&object, s, f);
                let cursor = CollectionCursorLocals::reserve(s, f);
                let complete = self.open_frame(ControlFrameKind::Block, f);
                let again = self.open_frame(ControlFrameKind::Loop, f);
                self.emit_collection_next_map(&groups, &cursor, found, f)?;
                found.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.emit_branch_if_to_target(complete, f);
                self.emit_collection_read_map(&groups, cursor.index, &key, &list_value, f);
                self.emit_increment_local(cursor.index, 1, f);
                let property = self.emit_value_to_property_key_locals(&key, f)?;
                self.emit_create_data_property_or_throw(
                    &object_value,
                    &property,
                    &list_value,
                    &pending,
                    f,
                )?;
                property.clear(f);
                self.emit_collection_abrupt_exit(&pending, &output, exit, f);
                self.emit_branch_to_target(again, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                output.set_normal(&object_value, f);
                object_value.clear(f);
                cursor.release(s, f);
                object.clear(f);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        iterator.clear(f);
        groups.clear(f);
        header.clear(f);
        s.release_i32_local(found, f);
        s.release_i32_local(done, f);
        s.release_i64_local(index, f);
        s.release_i64_local(position, f);
        output.clear(f);
        pending.clear(f);
        undefined.clear(f);
        number.clear(f);
        list_value.clear(f);
        item.clear(f);
        key.clear(f);
        callback.clear(f);
        source.clear(f);
        Ok(())
    }
}
