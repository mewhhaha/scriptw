//! Static targets select candidates; this reusable region proves the saved
//! callable's actual immutable native code and its execution Realm at runtime.
use super::*;
use crate::gc_types::{
    ExecutableCodeKind, ExecutableCodeSchema, FunctionContextSchema, FunctionObject,
    FunctionObjectSchema, GcI32Constant, GcNullability, ValueLocals,
};

impl FunctionBuilder<'_> {
    pub(super) fn emit_with_current_native_builtin(
        &mut self,
        callee: &ValueLocals,
        builtin: StandardBuiltinId,
        function: &mut Function,
        consume: impl FnOnce(&mut Self, &mut Function) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        callee.tag().load(function);
        function.instruction(&Instruction::I32Const(ValueKind::Function.tag() as i32));
        function.instruction(&Instruction::I32Eq);
        callee.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let callable = s.reserve_gc_local(function).initialize(
            callee.cast_reference::<FunctionObject>(s, function),
            function,
        );
        let code = s.reserve_gc_local(function).initialize(
            s.field(FunctionObjectSchema::CODE)
                .read(&callable, s, function)
                .reference(),
            function,
        );
        let context = s.reserve_gc_local(function).initialize(
            s.field(FunctionObjectSchema::CONTEXT)
                .read(&callable, s, function)
                .reference(),
            function,
        );
        let realm = s.reserve_gc_local(function).initialize(
            s.field(FunctionContextSchema::REALM)
                .read(&context, s, function)
                .reference(),
            function,
        );
        let kind = s.reserve_i32_local(function);
        let actual_builtin = s.reserve_i32_local(function);
        s.field(ExecutableCodeSchema::KIND)
            .read(&code, s, function)
            .store(kind, function);
        s.field(ExecutableCodeSchema::STANDARD_BUILTIN)
            .read(&code, s, function)
            .store(actual_builtin, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(
            ExecutableCodeKind::StandardBuiltin.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        actual_builtin.load(function);
        function.instruction(&Instruction::I32Const(builtin.encode()));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        let current_realm = self.load_current_realm(function);
        realm.load(s, function);
        current_realm.load(s, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        consume(self, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current_realm.clear(function);
        s.release_i32_local(actual_builtin, function);
        s.release_i32_local(kind, function);
        realm.clear(function);
        context.clear(function);
        code.clear(function);
        callable.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }
}
