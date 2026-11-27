//! Invocation-owned snapshots; the absent-index arm retains a real object.
use super::*;
use crate::arguments_protocol::{ArgumentsProjectionRead, StrictArgumentsProjection};
use crate::gc_types::RuntimeSchema;

pub(crate) struct ProjectedArgumentsLocals {
    proof: StrictArgumentsProjection,
    storage: BindingStorage,
    inputs: ProjectedArgumentsInputs,
    /// The real object once a read needed it: Undefined until then. Every
    /// read after it goes to the object, which an inherited getter may have
    /// changed.
    materialized: ValueLocals,
}

enum ProjectedArgumentsInputs {
    Vector { first: ValueLocals, count: I32Local },
    Local { count: I32Local },
}

enum ProjectedStaticRead {
    Length,
    Index(u32),
}

impl ProjectedArgumentsLocals {
    pub(crate) fn clear(self, schema: &RuntimeSchema, function: &mut Function) {
        self.materialized.clear(function);
        if let ProjectedArgumentsInputs::Vector { first, count } = self.inputs {
            first.clear(function);
            schema.release_i32_local(count, function);
        }
    }

    fn count(&self) -> I32Local {
        match self.inputs {
            ProjectedArgumentsInputs::Vector { count, .. }
            | ProjectedArgumentsInputs::Local { count } => count,
        }
    }

    fn emit_is_unmaterialized(&self, function: &mut Function) {
        self.materialized.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
    }
}

impl FunctionBuilder<'_> {
    /// A strict invocation whose own arguments object only `.length` and
    /// computed reads observe: nothing is built at entry. An unmapped object's
    /// own index below the count is a writable data property holding the
    /// passed value and `length` holds the count; nothing can write them
    /// before the object exists. The original vector or rooted local lanes
    /// answer those reads until an operation needs the real object.
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
        let entry = self
            .body_entry_locals()
            .expect("projection belongs to a callable invocation");
        let inputs = if let Some(arguments) = entry.counted_local_arguments() {
            ProjectedArgumentsInputs::Local {
                count: arguments.count(),
            }
        } else {
            let first = schema.reserve_value_local(function);
            first.set_undefined(function);
            let count = schema.reserve_i32_local(function);
            schema
                .array_type::<ValueArray>()
                .length(entry.arguments(), schema, function);
            count.store(function);
            count.load(function);
            self.open_frame(ControlFrameKind::If, function);
            // Snapshot before formal assignment or callbacks. Unmapped own
            // index zero is an unobservable data property.
            self.read_argument_at_index(0, &first, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            ProjectedArgumentsInputs::Vector { first, count }
        };
        let placeholder = schema.reserve_value_local(function);
        placeholder.set_undefined(function);
        self.write_binding_from_locals(storage, &placeholder, function);
        placeholder.clear(function);
        let materialized = schema.reserve_value_local(function);
        materialized.set_undefined(function);
        self.projected_arguments = Some(ProjectedArgumentsLocals {
            proof,
            storage,
            inputs,
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
        projected.emit_is_unmaterialized(function);
        self.open_frame(ControlFrameKind::If, function);
        self.initialize_arguments_binding(
            projected.storage,
            projected.proof.unmapped_protocol(),
            function,
        )?;
        self.read_binding_to_locals(projected.storage, &projected.materialized, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// Whether `expression` is a read the projection answers. A fast path that
    /// would evaluate its base on its own leaves it whole instead, so the read
    /// keeps the answer from the invocation's original arguments.
    pub(crate) fn is_projected_arguments_read(&self, expression: &TypedExpr) -> bool {
        self.projected_arguments
            .as_ref()
            .is_some_and(|projected| projected.proof.read(expression).is_some())
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
        if projected.proof.own_binding(expression) {
            // The proof admits the binding only as the base of a read, so this
            // is a site that took such a read apart and evaluates the base on
            // its own. The binding holds a placeholder until a read needs the
            // object: the base is the real object, built now if no read has.
            let result = self.emit_materialize_projected_arguments(&projected, function);
            if result.is_ok() {
                output.copy_from(&projected.materialized, function);
                self.completion().set_normal(output, function);
            }
            self.projected_arguments = Some(projected);
            result?;
            return Ok(true);
        }
        let Some(read) = projected.proof.read(expression) else {
            self.projected_arguments = Some(projected);
            return Ok(false);
        };
        let static_read = match read {
            ArgumentsProjectionRead::Length => Some(ProjectedStaticRead::Length),
            ArgumentsProjectionRead::First => Some(ProjectedStaticRead::Index(0)),
            ArgumentsProjectionRead::Other | ArgumentsProjectionRead::Index => self
                .body_entry_locals()
                .and_then(|entry| entry.counted_local_arguments())
                .and_then(|arguments| {
                    let index = projected.proof.literal_index(expression)?;
                    (index < arguments.lanes().len() as u32)
                        .then_some(ProjectedStaticRead::Index(index))
                }),
        };
        let result = match (static_read, read) {
            (Some(read), _) => {
                self.compile_projected_static_read(&projected, read, expression, output, function)
            }
            (None, ArgumentsProjectionRead::Index) => {
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
                    self.emit_propagate_current_throw_if_needed(function);
                    self.compile_projected_index_read(&projected, &key, output, function)
                });
                key.clear(function);
                self.projected_arguments = Some(projected);
                result?;
                return Ok(true);
            }
            (None, _) => self
                .emit_materialize_projected_arguments(&projected, function)
                .and_then(|()| self.compile_expr_to_value(expression, output, function)),
        };
        self.projected_arguments = Some(projected);
        result?;
        Ok(true)
    }

    /// `.length` or a statically selected local lane.
    fn compile_projected_static_read(
        &mut self,
        projected: &ProjectedArgumentsLocals,
        read: ProjectedStaticRead,
        expression: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        projected.emit_is_unmaterialized(function);
        if let ProjectedStaticRead::Index(index) = read {
            projected.count().load(function);
            function.instruction(&Instruction::I32Const(index as i32));
            function.instruction(&Instruction::I32GtU);
            function.instruction(&Instruction::I32And);
        }
        self.open_frame(ControlFrameKind::If, function);
        match read {
            ProjectedStaticRead::Index(index) => match &projected.inputs {
                ProjectedArgumentsInputs::Vector { first, .. } => {
                    assert_eq!(index, 0, "only index zero has a vector snapshot");
                    output.copy_from(first, function);
                }
                ProjectedArgumentsInputs::Local { .. } => {
                    let entry = self
                        .body_entry_locals()
                        .expect("local projection belongs to a callable entry");
                    assert!(entry.copy_local_argument(index as usize, output, function));
                }
            },
            ProjectedStaticRead::Length => {
                let bits = schema.reserve_i64_local(function);
                projected.count().load(function);
                function.instruction(&Instruction::F64ConvertI32U);
                function.instruction(&Instruction::I64ReinterpretF64);
                bits.store(function);
                output.set_number(bits, function);
                schema.release_i64_local(bits, function);
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
        projected.emit_is_unmaterialized(function);
        function.instruction(&Instruction::I32Eqz);
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
        projected.count().load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::I32Or);
        self.emit_branch_if_to_target(object, function);
        self.emit_projected_argument_at_index(index, output, function);
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
