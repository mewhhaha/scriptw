//! Fixed native entries borrow the same packed append admission/publication
//! authority as ordinary Array.push, with rooted values in place of an argv.
use super::*;
use crate::gc_types::GcNullability;
use crate::runtime_helpers::{
    ArrayPackedPush1Parameters, ArrayPackedPush2Parameters, ArrayPackedPush3Parameters,
};

impl FunctionBuilder<'_> {
    fn emit_fixed_packed_array_push(
        &mut self,
        receiver: &ValueLocals,
        arguments: &[&ValueLocals],
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // The closed result protocol is Normal(Undefined) on a pure decline,
        // Normal(Number) on publication, and Throw for any future abrupt route.
        result.initialize(f);
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<ArrayObject>(s, f), f);
        let captured_length = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::LENGTH)
            .read(&array, s, f)
            .store_i64(captured_length, f);
        let count = s.reserve_i32_local(f);
        count.set_constant(arguments.len() as i32, f);
        let length = s.reserve_i64_local(f);
        captured_length.load(f);
        f.instruction(&Instruction::I64Const(arguments.len() as i64));
        f.instruction(&Instruction::I64Add);
        length.store(f);
        let handled = self.emit_try_packed_array_append(
            receiver,
            captured_length,
            count,
            length,
            f,
            |builder, packed, captured_length, f| {
                let index = s.reserve_i64_local(f);
                captured_length.load(f);
                index.store(f);
                for value in arguments {
                    builder.emit_array_indexed_publish_default_data(
                        &packed.array,
                        index,
                        value,
                        f,
                    )?;
                    builder.emit_increment_local(index, 1, f);
                    s.field(ArrayObjectSchema::LENGTH).write(
                        &packed.array,
                        GcOperand::i64_local(index),
                        s,
                        f,
                    );
                }
                s.release_i64_local(index, f);
                Ok(())
            },
        )?;
        handled.load(f);
        self.open_frame(ControlFrameKind::If, f);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        length.store(f);
        result.value().set_number(length, f);
        result.set_normal(result.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(handled, f);
        s.release_i64_local(length, f);
        s.release_i32_local(count, f);
        s.release_i64_local(captured_length, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_array_packed_push1_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::ArrayPackedPush1);
        let p = self.helper_parameters::<ArrayPackedPush1Parameters>(&mut f);
        let result = self.runtime_schema().reserve_completion(&mut f);
        self.emit_fixed_packed_array_push(&p.receiver, &[&p.first], &result, &mut f)?;
        result.emit(&mut f);
        result.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    pub(crate) fn compile_array_packed_push2_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::ArrayPackedPush2);
        let p = self.helper_parameters::<ArrayPackedPush2Parameters>(&mut f);
        let result = self.runtime_schema().reserve_completion(&mut f);
        self.emit_fixed_packed_array_push(&p.receiver, &[&p.first, &p.second], &result, &mut f)?;
        result.emit(&mut f);
        result.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    pub(crate) fn compile_array_packed_push3_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::ArrayPackedPush3);
        let p = self.helper_parameters::<ArrayPackedPush3Parameters>(&mut f);
        let result = self.runtime_schema().reserve_completion(&mut f);
        self.emit_fixed_packed_array_push(
            &p.receiver,
            &[&p.first, &p.second, &p.third],
            &result,
            &mut f,
        )?;
        result.emit(&mut f);
        result.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
