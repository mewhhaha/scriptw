//! The observable Array map/filter/every/some loop. The length is captured
//! once; each subsequent HasProperty and Get uses the live receiver.
use super::*;
#[derive(Clone, Copy)]
pub(super) enum ArrayCallbackIterationKind {
    Map,
    Filter,
    Every,
    Some,
}
impl ArrayCallbackIterationKind {
    const fn callback_error(self) -> RuntimeErrorMessage {
        match self {
            Self::Map => RuntimeErrorMessage::ARRAY_PROTOTYPE_MAP_MAPPER_IS_NOT_CALLABLE,
            Self::Filter => RuntimeErrorMessage::ARRAY_PROTOTYPE_FILTER_CALLBACK_IS_NOT_CALLABLE,
            Self::Every => RuntimeErrorMessage::ARRAY_PROTOTYPE_EVERY_CALLBACK_IS_NOT_CALLABLE,
            Self::Some => RuntimeErrorMessage::ARRAY_PROTOTYPE_SOME_CALLBACK_IS_NOT_CALLABLE,
        }
    }
}
impl FunctionBuilder<'_> {
    pub(super) fn compile_array_callback_iteration(
        &mut self,
        f: &mut Function,
        kind: ArrayCallbackIterationKind,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let callback = s.reserve_value_local(f);
        let this_arg = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let index_value = s.reserve_value_local(f);
        let target = s.reserve_value_local(f);
        let answer = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let target_index = s.reserve_i64_local(f);
        let present = s.reserve_i32_local(f);
        let selected = s.reserve_i32_local(f);
        let default_species = s.reserve_i32_local(f);
        default_species.set_constant(0, f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?;
        self.emit_builtin_arg_to_value(0, &callback, f);
        self.emit_is_callable_i32(&callback, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(kind.callback_error(), &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_builtin_arg_to_value(1, &this_arg, f);
        f.instruction(&Instruction::I64Const(0));
        target_index.store(f);
        match kind {
            ArrayCallbackIterationKind::Map => self.emit_array_species_create_with_default_flag(
                &receiver,
                length,
                &target,
                Some(default_species),
                f,
            )?,
            ArrayCallbackIterationKind::Filter => self.emit_array_species_create_with_default_flag(
                &receiver,
                target_index,
                &target,
                Some(default_species),
                f,
            )?,
            ArrayCallbackIterationKind::Every | ArrayCallbackIterationKind::Some => {}
        }
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let exhausted = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exhausted, f);
        self.emit_array_native_has_and_get_index(&receiver, index, &element, present, &pending, f)?;
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_array_native_number(index, &index_value, f);
        self.emit_array_callback_call_from_values(
            &callback,
            &this_arg,
            &[&element, &index_value, &receiver],
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        match kind {
            ArrayCallbackIterationKind::Map => {
                element.copy_from(pending.value(), f);
                default_species.load(f);
                self.open_frame(ControlFrameKind::If, f);
                let fresh = s
                    .reserve_gc_local(f)
                    .initialize(target.cast_reference::<ArrayObject>(s, f), f);
                self.emit_array_indexed_publish_default_data(&fresh, index, &element, f)?;
                fresh.clear(f);
                f.instruction(&Instruction::Else);
                let key = self.emit_array_native_index_key(index, f)?;
                self.emit_create_data_property_or_throw(&target, &key, &element, &pending, f)?;
                key.clear(f);
                self.emit_array_native_propagate(&pending, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            ArrayCallbackIterationKind::Filter => {
                self.compile_truthy_tagged_i32(pending.value(), f)?;
                self.open_frame(ControlFrameKind::If, f);
                default_species.load(f);
                self.open_frame(ControlFrameKind::If, f);
                let fresh = s
                    .reserve_gc_local(f)
                    .initialize(target.cast_reference::<ArrayObject>(s, f), f);
                self.emit_array_indexed_publish_default_data(&fresh, target_index, &element, f)?;
                target_index.load(f);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Add);
                target_index.store(f);
                s.field(ArrayObjectSchema::LENGTH).write(
                    &fresh,
                    GcOperand::i64_local(target_index),
                    s,
                    f,
                );
                fresh.clear(f);
                f.instruction(&Instruction::Else);
                let key = self.emit_array_native_index_key(target_index, f)?;
                self.emit_create_data_property_or_throw(&target, &key, &element, &pending, f)?;
                key.clear(f);
                self.emit_array_native_propagate(&pending, f);
                target_index.load(f);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Add);
                target_index.store(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            ArrayCallbackIterationKind::Every | ArrayCallbackIterationKind::Some => {
                self.compile_truthy_tagged_i32(pending.value(), f)?;
                selected.store(f);
                selected.load(f);
                if matches!(kind, ArrayCallbackIterationKind::Every) {
                    f.instruction(&Instruction::I32Eqz);
                }
                self.open_frame(ControlFrameKind::If, f);
                answer.set_boolean(selected, f);
                self.completion().set_normal(&answer, f);
                self.emit_branch_to_target(exit, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        match kind {
            ArrayCallbackIterationKind::Map | ArrayCallbackIterationKind::Filter => {
                self.completion().set_normal(&target, f)
            }
            ArrayCallbackIterationKind::Every => {
                answer.set_scalar(crate::gc_types::ScalarValue::Boolean(true), f);
                self.completion().set_normal(&answer, f);
            }
            ArrayCallbackIterationKind::Some => {
                answer.set_scalar(crate::gc_types::ScalarValue::Boolean(false), f);
                self.completion().set_normal(&answer, f);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(default_species, f);
        s.release_i32_local(selected, f);
        s.release_i32_local(present, f);
        s.release_i64_local(target_index, f);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        pending.clear(f);
        answer.clear(f);
        target.clear(f);
        index_value.clear(f);
        element.clear(f);
        this_arg.clear(f);
        callback.clear(f);
        receiver.clear(f);
        Ok(())
    }
}
