use super::*;
impl FunctionBuilder<'_> {
    pub(crate) fn emit_weak_map_constructor(&mut self, f: &mut Function) -> Result<(), EmitError> {
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
            RuntimeErrorMessage::WEAKMAP_CONSTRUCTOR_REQUIRES_NEW,
            &pending,
            f,
        )?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_get_prototype_from_constructor(
            &new_target,
            OrdinaryDefaultPrototype::WeakMap,
            &pending,
            f,
        )?;
        self.emit_collection_propagate(&pending, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(pending.value()), f)?,
            f,
        );
        let record = s.reserve_gc_local(f).initialize(
            s.struct_type::<WeakMapObject>()
                .construct((GcOperand::reference(&header, s), GcOperand::null(s)), f),
            f,
        );
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
            RuntimeErrorMessage::WEAKMAP_CONSTRUCTOR_SET_METHOD_IS_NOT_CALLABLE,
            f,
        )?;
        let iterator =
            self.emit_get_sync_iterator(&source, SyncIteratorConsumer::WeakMapConstructor, f)?;
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_sync_iterator_step_value(&iterator, done, &entry, f)?;
        done.load(f);
        self.emit_branch_if_to_target(exit, f);
        self.emit_is_heap_object_like_tag_i32(entry.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::WEAKMAP_CONSTRUCTOR_ITERATOR_VALUE_MUST_BE_AN_OBJECT,
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
}
