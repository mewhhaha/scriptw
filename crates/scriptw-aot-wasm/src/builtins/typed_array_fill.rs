//! Native Fill retains one converted primitive through both bounds witnesses.
use super::super::*;
use crate::gc_types::{GcI32Constant, TypedArrayObjectSchema};
use crate::operations::BigIntNumberPolicy;

impl FunctionBuilder<'_> {
    pub(super) fn compile_typed_array_prototype_fill_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let initial = s.reserve_i64_local(f);
        let current = s.reserve_i64_local(f);
        let relative = s.reserve_i64_local(f);
        let start = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let array = self.emit_array_native_typed_brand(
            &receiver,
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FILL_REQUIRES_TYPEDARRAY,
            &pending,
            f,
        )?;
        self.emit_validate_typed_array_write_view(&array, initial, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        let kind = s.reserve_i32_local(f);
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&array, s, f)
            .store(kind, f);
        self.emit_builtin_arg_to_value(0, &value, f);
        crate::gc_types::emit_i32_in(
            kind,
            TypedArrayElementKind::ALL
                .iter()
                .copied()
                .filter(|k| k.content_type() == TypedArrayContentType::BigInt)
                .map(|k| k.encode()),
            f,
        );
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_bigint_locals(&value, BigIntNumberPolicy::RejectNumber, &pending, f)?;
        f.instruction(&Instruction::Else);
        self.emit_value_to_number_payload(&value, &pending, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_array_native_propagate(&pending, f);
        value.copy_from(pending.value(), f);
        self.emit_builtin_arg_to_value(1, &argument, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        self.emit_array_slice_clamped_index(relative, initial, start, f);
        initial.load(f);
        end.store(f);
        self.emit_builtin_arg_to_value(2, &argument, f);
        argument.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        self.emit_array_slice_clamped_index(relative, initial, end, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Coercions may detach or shrink even when no writes will follow.
        self.emit_validate_typed_array_view(&array, current, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        current.load(f);
        end.load(f);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        current.load(f);
        end.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        start.load(f);
        index.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        end.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, f);
        // Only the retained primitive reaches the shared element writer. No
        // source object is converted again during this loop.
        self.emit_typed_array_element_write_from_locals(&array, index, &value, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&receiver, f);
        s.release_i32_local(kind, f);
        array.clear(f);
        for local in [index, end, start, relative, current, initial] {
            s.release_i64_local(local, f);
        }
        pending.clear(f);
        argument.clear(f);
        value.clear(f);
        receiver.clear(f);
        Ok(())
    }
}
