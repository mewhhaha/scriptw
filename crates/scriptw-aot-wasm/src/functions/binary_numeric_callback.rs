//! Invoke only the actual callable's published scalar Number callback entry.
mod sort_entry;

use super::*;
use crate::gc_types::{
    BinaryNumericCallback, ExecutableCodeKind, ExecutableCodeSchema, F64Local, FunctionObject,
    FunctionObjectSchema, GcI32Constant, GcNullability, I32Local, Nullable, ValueLocals,
};

impl FunctionBuilder<'_> {
    /// Operands stay rooted in their caller's saved values. A declined attempt
    /// neither converts nor calls them; the caller owns its original fallback.
    pub(crate) fn emit_try_binary_numeric_callback(
        &mut self,
        callee: &ValueLocals,
        left: &ValueLocals,
        right: &ValueLocals,
        output: F64Local,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(function);
        handled.set_constant(0, function);
        callee.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
        function.instruction(&Instruction::I32Eq);
        callee.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32And);
        for operand in [left, right] {
            operand.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32And);
        }
        self.open_frame(ControlFrameKind::If, function);
        let callable = schema.reserve_gc_local(function).initialize(
            callee.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CODE)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        let kind = schema.reserve_i32_local(function);
        schema
            .field(ExecutableCodeSchema::KIND)
            .read(&code, schema, function)
            .store(kind, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(
            ExecutableCodeKind::JavaScript.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let optional = schema
            .reserve_function_local::<BinaryNumericCallback, Nullable>(function)
            .initialize(
                schema
                    .field(ExecutableCodeSchema::BINARY_NUMERIC_ENTRY)
                    .read(&code, schema, function)
                    .callable(),
                function,
            );
        optional.load(function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let entry = schema
            .reserve_function_local::<BinaryNumericCallback, _>(function)
            .initialize(optional.load(function).require_non_null(function), function);
        // The actual P thunk checks shared source depth before its arithmetic.
        // Its typed funcref retains the defining module independently of this R
        // caller. No P-local source index or current-program match is involved.
        schema.call_binary_numeric_reference(&entry, left, right, handled, output, function);
        entry.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        optional.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(kind, function);
        code.clear(function);
        callable.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(handled)
    }
}
