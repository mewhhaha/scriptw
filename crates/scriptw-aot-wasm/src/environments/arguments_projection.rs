//! Invocation-owned snapshots; the absent-index arm retains a real object.
use super::*;
use crate::arguments_protocol::{ArgumentsProjectionRead, StrictArgumentsProjection};
use crate::gc_types::RuntimeSchema;

pub(crate) struct ProjectedArgumentsLocals {
    proof: StrictArgumentsProjection,
    storage: BindingStorage,
    first: ValueLocals,
    count: I32Local,
    /// The real object once a read needed it: Undefined until then. Every
    /// read after it goes to the object, which an inherited getter may have
    /// changed.
    materialized: ValueLocals,
}

impl ProjectedArgumentsLocals {
    pub(crate) fn clear(self, schema: &RuntimeSchema, function: &mut Function) {
        self.first.clear(function);
        self.materialized.clear(function);
        schema.release_i32_local(self.count, function);
    }
}

impl FunctionBuilder<'_> {
    /// A strict invocation whose own arguments object only `.length` and
    /// computed reads observe: nothing is built at entry. An unmapped object's
    /// own index below the count is a writable data property holding the
    /// passed value and `length` holds the count; nothing can write them
    /// before the object exists, and building it later from the unchanged
    /// argument vector makes the same object, so reads answer from the
    /// vector until one needs the object itself.
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
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let placeholder = schema.reserve_value_local(function);
        placeholder.set_undefined(function);
        self.write_binding_from_locals(storage, &placeholder, function);
        placeholder.clear(function);
        let materialized = schema.reserve_value_local(function);
        materialized.set_undefined(function);
        self.projected_arguments = Some(ProjectedArgumentsLocals {
            proof,
            storage,
            first,
            count,
            materialized,
        });
        Ok(())
    }

    /// Builds the real object the first time a read needs it and publishes it
    /// as the binding; later reads find it in `materialized`.
    fn emit_materialize_projected_arguments(
        &mut self,
        projected: &ProjectedArgumentsLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        projected.materialized.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.initialize_arguments_binding(
            projected.storage,
            projected.proof.unmapped_protocol(),
            function,
        )?;
        self.read_binding_to_locals(projected.storage, &projected.materialized, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
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
        let result = match read {
            ArgumentsProjectionRead::Index => {
                // The key is evaluated first, with the projection in place:
                // it may read the binding itself.
                let key_expression = projected
                    .proof
                    .read_key(expression)
                    .expect("an index read has a key expression");
                self.projected_arguments = Some(projected);
                let key = self.runtime_schema().reserve_value_local(function);
                let compiled = self.compile_expr_to_value(key_expression, &key, function);
                let projected = self
                    .projected_arguments
                    .take()
                    .expect("the key's reads restore the projection");
                let result = compiled.and_then(|()| {
                    self.compile_projected_index_read(&projected, &key, output, function)
                });
                key.clear(function);
                self.projected_arguments = Some(projected);
                result?;
                return Ok(true);
            }
            read => {
                self.compile_projected_static_read(&projected, read, expression, output, function)
            }
        };
        self.projected_arguments = Some(projected);
        result?;
        Ok(true)
    }

    /// `.length`, index zero and other static keys.
    fn compile_projected_static_read(
        &mut self,
        projected: &ProjectedArgumentsLocals,
        read: ArgumentsProjectionRead,
        expression: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        // Answered from the vector while no object exists, and for index
        // zero only when it is an own index.
        projected.materialized.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        match read {
            ArgumentsProjectionRead::First => {
                projected.count.load(function);
                function.instruction(&Instruction::I32Const(0));
                function.instruction(&Instruction::I32Ne);
                function.instruction(&Instruction::I32And);
            }
            ArgumentsProjectionRead::Length => {}
            ArgumentsProjectionRead::Other | ArgumentsProjectionRead::Index => {
                function.instruction(&Instruction::Drop);
                function.instruction(&Instruction::I32Const(0));
            }
        }
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
            ArgumentsProjectionRead::Other | ArgumentsProjectionRead::Index => {
                output.set_undefined(function);
            }
        }
        self.completion().set_normal(output, function);
        function.instruction(&Instruction::Else);
        self.emit_materialize_projected_arguments(projected, function)?;
        // With the projection absent, compile the original source operation
        // through canonical Get on the binding, which now holds the object.
        let result = self.compile_expr_to_value(expression, output, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result
    }

    /// `arguments[key]`: the key is evaluated once. A Number that is an
    /// integer index below the count reads the passed value while no object
    /// exists; every other key reads the real object, after ToPropertyKey,
    /// exactly as the source Get does.
    fn compile_projected_index_read(
        &mut self,
        projected: &ProjectedArgumentsLocals,
        key: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let index = schema.reserve_i32_local(function);
        let number = schema.reserve_f64_local(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let object = self.open_frame(ControlFrameKind::Block, function);
        projected.materialized.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(object, function);
        key.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(object, function);
        key.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        number.store(function);
        number.load(function);
        function.instruction(&Instruction::I32TruncSatF64U);
        index.store(function);
        // An integer index: the value converts back exactly (-0 is index 0,
        // NaN and fractions are not), and it is below the count.
        index.load(function);
        function.instruction(&Instruction::F64ConvertI32U);
        number.load(function);
        function.instruction(&Instruction::F64Ne);
        index.load(function);
        projected.count.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::I32Or);
        self.emit_branch_if_to_target(object, function);
        let entry = self
            .body_entry_locals()
            .expect("source invocation owns its argument vector");
        self.emit_argument_vector_entry_to_value(entry.arguments(), index, output, function);
        self.completion().set_normal(output, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_materialize_projected_arguments(projected, function)?;
        let property_key = self.emit_value_to_property_key_locals(key, function)?;
        schema
            .call_helper(
                crate::runtime_helpers::ObjectReadArguments::new(
                    &projected.materialized,
                    &projected.materialized,
                    &property_key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(self.completion(), function);
        property_key.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        output.copy_from(self.completion().value(), function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_f64_local(number, function);
        schema.release_i32_local(index, function);
        Ok(())
    }
}
