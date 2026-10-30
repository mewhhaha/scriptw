//! Resolve a pure scalar callback once for a private numeric sorting phase.
use super::*;

impl FunctionBuilder<'_> {
    /// The published funcref is immutable and retains its defining module.
    /// No source code runs during admission. A null entry keeps ordinary sort.
    pub(crate) fn emit_binary_numeric_sort_entry(
        &mut self,
        callee: &ValueLocals,
        f: &mut Function,
    ) -> Result<crate::gc_types::FunctionLocal<BinaryNumericCallback, Nullable>, EmitError> {
        let s = self.runtime_schema();
        let result = s
            .reserve_function_local::<BinaryNumericCallback, Nullable>(f)
            .initialize_null(f);
        callee.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
        f.instruction(&Instruction::I32Eq);
        callee.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let callable = s
            .reserve_gc_local(f)
            .initialize(callee.cast_reference::<FunctionObject>(s, f), f);
        let code = s.reserve_gc_local(f).initialize(
            s.field(FunctionObjectSchema::CODE)
                .read(&callable, s, f)
                .reference(),
            f,
        );
        let kind = s.reserve_i32_local(f);
        s.field(ExecutableCodeSchema::KIND)
            .read(&code, s, f)
            .store(kind, f);
        kind.load(f);
        f.instruction(&Instruction::I32Const(
            ExecutableCodeKind::JavaScript.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        result.replace(
            s.field(ExecutableCodeSchema::BINARY_NUMERIC_ENTRY)
                .read(&code, s, f)
                .callable(),
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(kind, f);
        code.clear(f);
        callable.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(result)
    }
}
