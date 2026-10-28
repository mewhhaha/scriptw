//! Generic mutation uses the sole object operations after one cached length.
use super::*;
use crate::objects::PropertyKeyLocals;
use scriptw_ir::NativeErrorKind;

#[derive(Clone, Copy)]
pub(super) enum ArrayMutationKind {
    Pop,
    Push,
    Shift,
    Unshift,
}
#[derive(Clone, Copy)]
enum RemoveKind {
    Pop,
    Shift,
}
#[derive(Clone, Copy)]
enum GrowKind {
    Push,
    Unshift,
}

#[must_use = "the prepared Object and cached length must be consumed"]
struct MutationReceiver {
    object: ValueLocals,
    length: I64Local,
}
impl MutationReceiver {
    fn prepare(builder: &mut FunctionBuilder<'_>, f: &mut Function) -> Result<Self, EmitError> {
        let s = builder.runtime_schema();
        let object = s.reserve_value_local(f);
        builder.compile_this_to_locals(&object, f)?;
        let length = s.reserve_i64_local(f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        builder.emit_array_like_length_snapshot(&object, length, &pending, f)?;
        pending.clear(f);
        Ok(Self { object, length })
    }
    fn finish(self, s: &RuntimeSchema, f: &mut Function) {
        self.object.clear(f);
        s.release_i64_local(self.length, f);
    }
}
impl FunctionBuilder<'_> {
    pub(super) fn emit_native_array_mutation(
        &mut self,
        kind: ArrayMutationKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        match kind {
            ArrayMutationKind::Pop => self.emit_array_mutation_remove(RemoveKind::Pop, f),
            ArrayMutationKind::Shift => self.emit_array_mutation_remove(RemoveKind::Shift, f),
            ArrayMutationKind::Push => self.emit_array_mutation_grow(GrowKind::Push, f),
            ArrayMutationKind::Unshift => self.emit_array_mutation_grow(GrowKind::Unshift, f),
        }
    }
    fn emit_mutation_throw_exit(
        &mut self,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) {
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        output.copy_from(pending, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }
    fn emit_mutation_number(&self, integer: I64Local, value: &ValueLocals, f: &mut Function) {
        integer.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        value.scalar().store(f);
        value.set_number(value.scalar(), f);
    }
    fn emit_mutation_length_key(
        &mut self,
        f: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let s = self.runtime_schema();
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("length", f)?, f);
        let key = PropertyKeyLocals::from_string(s, &text, f);
        text.clear(f);
        Ok(key)
    }
    fn emit_mutation_get(
        &mut self,
        receiver: &MutationReceiver,
        index: I64Local,
        value: &ValueLocals,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_object_read(&receiver.object, &receiver.object, &key, pending, f)?;
        key.clear(f);
        self.emit_mutation_throw_exit(pending, output, exit, f);
        value.copy_from(pending.value(), f);
        Ok(())
    }
    fn emit_mutation_set(
        &mut self,
        receiver: &MutationReceiver,
        index: I64Local,
        value: &ValueLocals,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_object_write_strict(&receiver.object, &key, value, pending, f)?;
        key.clear(f);
        self.emit_mutation_throw_exit(pending, output, exit, f);
        Ok(())
    }
    fn emit_mutation_delete(
        &mut self,
        receiver: &MutationReceiver,
        index: I64Local,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_object_delete(&receiver.object, &key, pending, f)?;
        key.clear(f);
        self.emit_mutation_throw_exit(pending, output, exit, f);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_DELETE_PROPERTY,
            output,
            f,
        )?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_mutation_set_length(
        &mut self,
        receiver: &MutationReceiver,
        length: I64Local,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let number = s.reserve_value_local(f);
        self.emit_mutation_number(length, &number, f);
        let key = self.emit_mutation_length_key(f)?;
        self.emit_object_write_strict(&receiver.object, &key, &number, pending, f)?;
        key.clear(f);
        number.clear(f);
        self.emit_mutation_throw_exit(pending, output, exit, f);
        Ok(())
    }
    fn emit_mutation_move(
        &mut self,
        receiver: &MutationReceiver,
        from: I64Local,
        to: I64Local,
        value: &ValueLocals,
        present: I32Local,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let key = self.emit_array_native_index_key(from, f)?;
        self.emit_object_has_property_i32(&receiver.object, &key, present, f)?;
        key.clear(f);
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_mutation_get(receiver, from, value, pending, output, exit, f)?;
        self.emit_mutation_set(receiver, to, value, pending, output, exit, f)?;
        f.instruction(&Instruction::Else);
        self.emit_mutation_delete(receiver, to, pending, output, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_array_mutation_remove(
        &mut self,
        kind: RemoveKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = MutationReceiver::prepare(self, f)?;
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let first = s.reserve_value_local(f);
        first.set_undefined(f);
        let value = s.reserve_value_local(f);
        let from = s.reserve_i64_local(f);
        let to = s.reserve_i64_local(f);
        let length = s.reserve_i64_local(f);
        let present = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        f.instruction(&Instruction::I64Const(0));
        length.store(f);
        receiver.length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        receiver.length.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        length.store(f);
        match kind {
            RemoveKind::Pop => {
                length.load(f);
                from.store(f);
                self.emit_mutation_get(&receiver, from, &first, &pending, &output, exit, f)?;
            }
            RemoveKind::Shift => {
                f.instruction(&Instruction::I64Const(0));
                from.store(f);
                self.emit_mutation_get(&receiver, from, &first, &pending, &output, exit, f)?;
                f.instruction(&Instruction::I64Const(1));
                from.store(f);
                let finished = self.open_frame(ControlFrameKind::Block, f);
                let next = self.open_frame(ControlFrameKind::Loop, f);
                from.load(f);
                receiver.length.load(f);
                f.instruction(&Instruction::I64GeU);
                self.emit_branch_if_to_target(finished, f);
                from.load(f);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Sub);
                to.store(f);
                self.emit_mutation_move(
                    &receiver, from, to, &value, present, &pending, &output, exit, f,
                )?;
                self.emit_increment_local(from, 1, f);
                self.emit_branch_to_target(next, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
            }
        }
        self.emit_mutation_delete(&receiver, length, &pending, &output, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_mutation_set_length(&receiver, length, &pending, &output, exit, f)?;
        output.set_normal(&first, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        receiver.finish(s, f);
        output.clear(f);
        pending.clear(f);
        first.clear(f);
        value.clear(f);
        s.release_i64_local(from, f);
        s.release_i64_local(to, f);
        s.release_i64_local(length, f);
        s.release_i32_local(present, f);
        Ok(())
    }
    fn emit_array_mutation_grow(
        &mut self,
        kind: GrowKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = MutationReceiver::prepare(self, f)?;
        let arguments = s.reserve_gc_local(f).initialize(
            self.body_entry_locals()
                .map(|entry| entry.arguments())
                .ok_or_else(|| EmitError::unsupported("missing actual mutation argument List"))?
                .load(s, f),
            f,
        );
        let count = s.reserve_i32_local(f);
        s.array_type::<ValueArray>().length(&arguments, s, f);
        count.store(f);
        let position = s.reserve_i32_local(f);
        let present = s.reserve_i32_local(f);
        let from = s.reserve_i64_local(f);
        let to = s.reserve_i64_local(f);
        let length = s.reserve_i64_local(f);
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let value = s.reserve_value_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        receiver.length.load(f);
        count.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Add);
        length.store(f);
        length.load(f);
        f.instruction(&Instruction::I64Const(9007199254740991));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        let message = match kind {
            GrowKind::Push => RuntimeErrorMessage::ARRAY_PROTOTYPE_PUSH_LENGTH_EXCEEDS_SAFE_INTEGER,
            GrowKind::Unshift => {
                RuntimeErrorMessage::ARRAY_PROTOTYPE_UNSHIFT_LENGTH_EXCEEDS_SAFE_INTEGER
            }
        };
        self.emit_throw_current_function_realm_type_error(message, &output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        if matches!(kind, GrowKind::Push) {
            let handled = self.emit_try_packed_array_push(
                &receiver.object,
                receiver.length,
                &arguments,
                count,
                length,
                f,
            )?;
            handled.load(f);
            s.release_i32_local(handled, f);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_mutation_number(length, &value, f);
            output.set_normal(&value, f);
            self.emit_branch_to_target(exit, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        match kind {
            GrowKind::Push => {
                receiver.length.load(f);
                to.store(f);
            }
            GrowKind::Unshift => {
                count.load(f);
                self.open_frame(ControlFrameKind::If, f);
                receiver.length.load(f);
                from.store(f);
                let finished = self.open_frame(ControlFrameKind::Block, f);
                let next = self.open_frame(ControlFrameKind::Loop, f);
                from.load(f);
                f.instruction(&Instruction::I64Eqz);
                self.emit_branch_if_to_target(finished, f);
                from.load(f);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Sub);
                from.store(f);
                from.load(f);
                count.load(f);
                f.instruction(&Instruction::I64ExtendI32U);
                f.instruction(&Instruction::I64Add);
                to.store(f);
                self.emit_mutation_move(
                    &receiver, from, to, &value, present, &pending, &output, exit, f,
                )?;
                self.emit_branch_to_target(next, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                f.instruction(&Instruction::I64Const(0));
                to.store(f);
            }
        }
        f.instruction(&Instruction::I32Const(0));
        position.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        position.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(finished, f);
        self.emit_argument_vector_entry_to_value(&arguments, position, &value, f);
        self.emit_mutation_set(&receiver, to, &value, &pending, &output, exit, f)?;
        position.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        position.store(f);
        self.emit_increment_local(to, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_mutation_set_length(&receiver, length, &pending, &output, exit, f)?;
        self.emit_mutation_number(length, &value, f);
        output.set_normal(&value, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        receiver.finish(s, f);
        arguments.clear(f);
        value.clear(f);
        output.clear(f);
        pending.clear(f);
        s.release_i32_local(count, f);
        s.release_i32_local(position, f);
        s.release_i32_local(present, f);
        s.release_i64_local(from, f);
        s.release_i64_local(to, f);
        s.release_i64_local(length, f);
        Ok(())
    }
}
