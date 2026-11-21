//! Closed module-state reads consume only rooted GC ModuleRecord owners.
use super::*;

impl FunctionBuilder<'_> {
    pub(super) fn emit_load_module_state_strict(
        &self,
        record: &GcLocal<ModuleRecord>,
        out: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .struct_type::<ModuleRecord>()
            .field(ModuleRecordSchema::STATE)
            .read(record, schema, function)
            .store(out, function);
        crate::gc_types::emit_i32_in(
            out,
            ModuleEvaluationState::ALL
                .iter()
                .copied()
                .map(|state| GcI32Constant::encode(state)),
            function,
        );
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
    }

    pub(super) fn emit_load_module_completion_strict(
        &self,
        record: &GcLocal<ModuleRecord>,
        out: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .struct_type::<ModuleRecord>()
            .field(ModuleRecordSchema::COMPLETION)
            .read(record, schema, function)
            .store(out, function);
        crate::gc_types::emit_i32_in(
            out,
            ModuleEvaluationCompletion::ALL
                .iter()
                .copied()
                .map(|state| GcI32Constant::encode(state)),
            function,
        );
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
    }

    pub(super) fn emit_load_module_body_state_strict(
        &self,
        record: &GcLocal<ModuleRecord>,
        out: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .struct_type::<ModuleRecord>()
            .field(ModuleRecordSchema::BODY_STATE)
            .read(record, schema, function)
            .store(out, function);
        crate::gc_types::emit_i32_in(
            out,
            ModuleBodyState::ALL
                .iter()
                .copied()
                .map(|state| GcI32Constant::encode(state)),
            function,
        );
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
    }

    pub(super) fn emit_load_module_activation_kind_strict(
        &self,
        record: &GcLocal<ModuleRecord>,
        out: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .struct_type::<ModuleRecord>()
            .field(ModuleRecordSchema::ACTIVATION_KIND)
            .read(record, schema, function)
            .store(out, function);
        crate::gc_types::emit_i32_in(
            out,
            ModuleActivationKind::ALL
                .iter()
                .copied()
                .map(|state| GcI32Constant::encode(state)),
            function,
        );
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
    }
}
