use super::*;

enum FindViaPredicateKind {
    Find,
    FindIndex,
    FindLast,
    FindLastIndex,
}

enum FindDirection {
    Ascending,
    Descending,
}

enum FindProjection {
    Value,
    Index,
}

impl FindViaPredicateKind {
    const fn direction(&self) -> FindDirection {
        match self {
            Self::Find | Self::FindIndex => FindDirection::Ascending,
            Self::FindLast | Self::FindLastIndex => FindDirection::Descending,
        }
    }

    const fn projection(&self) -> FindProjection {
        match self {
            Self::Find | Self::FindLast => FindProjection::Value,
            Self::FindIndex | Self::FindLastIndex => FindProjection::Index,
        }
    }

    const fn array_predicate_not_callable_message(&self) -> RuntimeErrorMessage {
        match self {
            Self::Find => RuntimeErrorMessage::ARRAY_PROTOTYPE_FIND_PREDICATE_IS_NOT_CALLABLE,
            Self::FindIndex => {
                RuntimeErrorMessage::ARRAY_PROTOTYPE_FINDINDEX_PREDICATE_IS_NOT_CALLABLE
            }
            Self::FindLast => {
                RuntimeErrorMessage::ARRAY_PROTOTYPE_FINDLAST_PREDICATE_IS_NOT_CALLABLE
            }
            Self::FindLastIndex => {
                RuntimeErrorMessage::ARRAY_PROTOTYPE_FINDLASTINDEX_PREDICATE_IS_NOT_CALLABLE
            }
        }
    }

    const fn typed_array_predicate_not_callable_message(&self) -> RuntimeErrorMessage {
        match self {
            Self::Find => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FIND_PREDICATE_IS_NOT_CALLABLE,
            Self::FindIndex => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FINDINDEX_PREDICATE_IS_NOT_CALLABLE
            }
            Self::FindLast => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FINDLAST_PREDICATE_IS_NOT_CALLABLE
            }
            Self::FindLastIndex => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FINDLASTINDEX_PREDICATE_IS_NOT_CALLABLE
            }
        }
    }
}

#[cfg(test)]
mod find_via_predicate_tests {
    use super::*;

    #[test]
    fn four_kinds_fix_direction_and_projection() {
        assert!(matches!(
            FindViaPredicateKind::Find.direction(),
            FindDirection::Ascending
        ));
        assert!(matches!(
            FindViaPredicateKind::FindIndex.direction(),
            FindDirection::Ascending
        ));
        assert!(matches!(
            FindViaPredicateKind::FindLast.direction(),
            FindDirection::Descending
        ));
        assert!(matches!(
            FindViaPredicateKind::FindLastIndex.direction(),
            FindDirection::Descending
        ));
        assert!(matches!(
            FindViaPredicateKind::Find.projection(),
            FindProjection::Value
        ));
        assert!(matches!(
            FindViaPredicateKind::FindLast.projection(),
            FindProjection::Value
        ));
        assert!(matches!(
            FindViaPredicateKind::FindIndex.projection(),
            FindProjection::Index
        ));
        assert!(matches!(
            FindViaPredicateKind::FindLastIndex.projection(),
            FindProjection::Index
        ));
    }
}

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn compile_array_prototype_find_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_find_with_kind(f, FindViaPredicateKind::Find, false)
    }
    pub(in crate::builtins) fn compile_array_prototype_find_index_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_find_with_kind(f, FindViaPredicateKind::FindIndex, false)
    }
    pub(in crate::builtins) fn compile_array_prototype_find_last_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_find_with_kind(f, FindViaPredicateKind::FindLast, false)
    }
    pub(in crate::builtins) fn compile_array_prototype_find_last_index_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_find_with_kind(f, FindViaPredicateKind::FindLastIndex, false)
    }
    pub(in crate::builtins) fn compile_typed_array_prototype_find_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_find_with_kind(f, FindViaPredicateKind::Find, true)
    }
    pub(in crate::builtins) fn compile_typed_array_prototype_find_index_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_find_with_kind(f, FindViaPredicateKind::FindIndex, true)
    }
    pub(in crate::builtins) fn compile_typed_array_prototype_find_last_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_find_with_kind(f, FindViaPredicateKind::FindLast, true)
    }
    pub(in crate::builtins) fn compile_typed_array_prototype_find_last_index_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_find_with_kind(f, FindViaPredicateKind::FindLastIndex, true)
    }
    fn compile_array_find_with_kind(
        &mut self,
        f: &mut Function,
        kind: FindViaPredicateKind,
        typed_array: bool,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let predicate = s.reserve_value_local(f);
        let this_arg = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let index_value = s.reserve_value_local(f);
        let answer = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let array = if typed_array {
            Some(self.emit_array_native_typed_receiver(
                &receiver,
                length,
                RuntimeErrorMessage::TYPEDARRAY_FIND_METHOD_REQUIRES_A_TYPEDARRAY,
                &pending,
                f,
            )?)
        } else {
            self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?;
            None
        };
        self.emit_builtin_arg_to_value(0, &predicate, f);
        self.emit_is_callable_i32(&predicate, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let message = if typed_array {
            kind.typed_array_predicate_not_callable_message()
        } else {
            kind.array_predicate_not_callable_message()
        };
        self.emit_throw_current_function_realm_type_error(message, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_builtin_arg_to_value(1, &this_arg, f);
        match kind.projection() {
            FindProjection::Value => answer.set_undefined(f),
            FindProjection::Index => answer.set_scalar(
                crate::gc_types::ScalarValue::NumberBits((-1.0f64).to_bits() as i64),
                f,
            ),
        }
        match kind.direction() {
            FindDirection::Ascending => f.instruction(&Instruction::I64Const(0)),
            FindDirection::Descending => {
                length.load(f);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Sub)
            }
        };
        index.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        match kind.direction() {
            FindDirection::Ascending => {
                index.load(f);
                length.load(f);
                f.instruction(&Instruction::I64GeU)
            }
            FindDirection::Descending => {
                index.load(f);
                f.instruction(&Instruction::I64Const(0));
                f.instruction(&Instruction::I64LtS)
            }
        };
        self.emit_branch_if_to_target(exit, f);
        // FindViaPredicate performs Get even for holes; no HasProperty shortcut.
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        element.copy_from(pending.value(), f);
        self.emit_array_native_number(index, &index_value, f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&element, &index_value, &receiver], f);
        self.emit_function_or_proxy_call_with_argv(&predicate, &this_arg, &argv, &pending, f)?;
        argv.clear(f);
        self.emit_array_native_propagate(&pending, f);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        match kind.projection() {
            FindProjection::Value => answer.copy_from(&element, f),
            FindProjection::Index => answer.copy_from(&index_value, f),
        };
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        match kind.direction() {
            FindDirection::Ascending => f.instruction(&Instruction::I64Add),
            FindDirection::Descending => f.instruction(&Instruction::I64Sub),
        };
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&answer, f);
        if let Some(array) = array {
            array.clear(f)
        }
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        pending.clear(f);
        answer.clear(f);
        index_value.clear(f);
        element.clear(f);
        this_arg.clear(f);
        predicate.clear(f);
        receiver.clear(f);
        Ok(())
    }
}
