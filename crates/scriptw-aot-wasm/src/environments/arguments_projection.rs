//! Invocation-owned snapshots; the absent-index arm retains a real object.
use super::*;
use crate::arguments_protocol::{ArgumentsProjectionRead, StrictArgumentsProjection};
use crate::gc_types::RuntimeSchema;

pub(crate) struct ProjectedArgumentsLocals {
    proof: StrictArgumentsProjection,
    first: ValueLocals,
    count: I32Local,
}

impl ProjectedArgumentsLocals {
    pub(crate) fn clear(self, schema: &RuntimeSchema, function: &mut Function) {
        self.first.clear(function);
        schema.release_i32_local(self.count, function);
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn initialize_projected_arguments_binding(
        &mut self,
        proof: StrictArgumentsProjection,
        storage: BindingStorage,
        protocol: PresentArgumentsObjectProtocol,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        proof.require_source(
            self.function_id
                .as_ref()
                .expect("projection belongs to source function"),
        );
        assert!(matches!(
            &protocol,
            PresentArgumentsObjectProtocol::Unmapped(_)
        ));
        let schema = self.runtime_schema();
        let first = schema.reserve_value_local(function);
        first.set_undefined(function);
        let count = schema.reserve_i32_local(function);
        schema.array_type::<ValueArray>().length(
            self.body_entry_locals()
                .expect("source invocation owns its argument vector")
                .arguments(),
            schema,
            function,
        );
        count.store(function);
        count.load(function);
        self.open_frame(ControlFrameKind::If, function);
        // Snapshot before formal assignment, callbacks or private argument-list
        // reuse. Unmapped own index zero is an unobservable data property.
        self.read_argument_at_index(0, &first, function);
        let placeholder = schema.reserve_value_local(function);
        placeholder.set_undefined(function);
        self.write_binding_from_locals(storage, &placeholder, function);
        placeholder.clear(function);
        function.instruction(&Instruction::Else);
        // A missing own index can execute an inherited getter with this object
        // as receiver. It can publish or mutate it, including its length.
        self.initialize_arguments_binding(storage, protocol, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.projected_arguments = Some(ProjectedArgumentsLocals {
            proof,
            first,
            count,
        });
        Ok(())
    }

    pub(crate) fn compile_projected_arguments_read(
        &mut self,
        expression: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(projected) = self.projected_arguments.take() else {
            return Ok(false);
        };
        let Some(read) = projected.proof.read(expression) else {
            self.projected_arguments = Some(projected);
            return Ok(false);
        };
        let schema = self.runtime_schema();
        projected.count.load(function);
        self.open_frame(ControlFrameKind::If, function);
        match read {
            ArgumentsProjectionRead::First => output.copy_from(&projected.first, function),
            ArgumentsProjectionRead::Length => {
                let bits = schema.reserve_i64_local(function);
                projected.count.load(function);
                function.instruction(&Instruction::F64ConvertI32U);
                function.instruction(&Instruction::I64ReinterpretF64);
                bits.store(function);
                output.set_number(bits, function);
                schema.release_i64_local(bits, function);
            }
        }
        self.completion().set_normal(output, function);
        function.instruction(&Instruction::Else);
        // With the projection temporarily absent, compile the original source
        // operation, including every later length read, through canonical Get.
        let result = self.compile_expr_to_value(expression, output, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.projected_arguments = Some(projected);
        result?;
        Ok(true)
    }
}
