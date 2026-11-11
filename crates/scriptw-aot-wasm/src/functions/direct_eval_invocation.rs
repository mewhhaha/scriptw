//! A retained direct-eval invocation never enters the JavaScript value domain.

use super::*;
use crate::gc_types::{
    BindingCell, DirectEvalDerivedBindings, DirectEvalDerivedBindingsSchema,
    DirectEvalExecutionContext, DirectEvalExecutionContextSchema, Environment, EnvironmentSchema,
    FunctionContext, FunctionContextSchema, GcLocal, GcOperand, NonNullable, Nullable, StoredValue,
    ValueLocals,
};

#[must_use = "direct-eval invocation roots must reach the prepared entry and be cleared"]
pub(crate) struct DirectEvalInvocationLocals {
    this_value: ValueLocals,
    new_target: ValueLocals,
    context: GcLocal<DirectEvalExecutionContext, Nullable>,
}

impl DirectEvalInvocationLocals {
    pub(super) fn this_value(&self) -> &ValueLocals {
        &self.this_value
    }
    pub(super) fn new_target(&self) -> &ValueLocals {
        &self.new_target
    }
    pub(super) fn context(&self) -> &GcLocal<DirectEvalExecutionContext, Nullable> {
        &self.context
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn initialize_direct_eval_execution_context(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let Some(cached) = self.direct_eval_execution_context_local() else {
            return Ok(());
        };
        let schema = self.runtime_schema();
        let context = schema
            .reserve_gc_local(function)
            .initialize(cached.load(schema, function), function);
        // Entry initialization acquires this edge before a resumed frame restores
        // its retained lexical depth. Capture hops must not be interpreted again
        // from that deeper Environment. Prepared entries supply the same typed edge.
        // Never store the context as a JavaScript value.
        let required = schema
            .reserve_gc_local::<DirectEvalExecutionContext, NonNullable>(function)
            .initialize(
                context.load(schema, function).require_non_null(function),
                function,
            );
        schema
            .struct_type::<Environment>()
            .field(EnvironmentSchema::DIRECT_EVAL_CONTEXT)
            .write(
                self.current_environment(),
                GcOperand::nullable_reference(&required, schema),
                schema,
                function,
            );
        required.clear(function);
        context.clear(function);
        Ok(())
    }

    fn emit_direct_eval_binding_cell(
        &mut self,
        storage: BindingStorage,
        function: &mut Function,
    ) -> Result<GcLocal<BindingCell>, EmitError> {
        let BindingStorage::EnvSlot { slot, hops } = storage else {
            return Err(EmitError::unsupported(
                "derived direct-eval state requires validated environment cells",
            ));
        };
        let environment = self.resolve_env_handle_local(hops, function);
        let cell = self.emit_environment_cell_local(&environment, slot, function);
        environment.clear(function);
        Ok(cell)
    }

    pub(crate) fn emit_capture_direct_eval_invocation(
        &mut self,
        function: &mut Function,
    ) -> Result<DirectEvalInvocationLocals, EmitError> {
        let schema = self.runtime_schema();
        let this_value = schema.reserve_value_local(function);
        this_value.set_undefined(function);
        let new_target = schema.reserve_value_local(function);
        new_target.set_undefined(function);
        let context = schema
            .reserve_gc_local::<DirectEvalExecutionContext, Nullable>(function)
            .initialize_null(schema, function);
        if let Some(existing) = self.direct_eval_execution_context_local() {
            context.replace(
                existing
                    .load(schema, function)
                    .require_non_null(function)
                    .nullable(),
                function,
            );
        } else {
            let class_context = schema
                .reserve_gc_local::<FunctionContext, Nullable>(function)
                .initialize_null(schema, function);
            if let Some(callable) = self
                .body_entry_locals()
                .and_then(|entry| entry.function_context())
            {
                class_context.replace(callable.load(schema, function).nullable(), function);
            }
            let home = schema.reserve_value_local(function);
            home.set_undefined(function);
            if self.function_flavor == FunctionFlavor::Arrow {
                if let Some(storage) = self.lookup_binding(LEXICAL_HOME_OBJECT_NAME) {
                    self.read_binding_to_locals(storage, &home, function)?;
                }
            } else if self
                .current_function_meta()
                .is_some_and(WasmFunctionMeta::has_home_object_execution_context)
            {
                let stored = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<FunctionContext>()
                        .field(FunctionContextSchema::HOME_OBJECT)
                        .read(&class_context, schema, function)
                        .reference(),
                    function,
                );
                schema
                    .struct_type::<StoredValue>()
                    .read_into(&stored, &home, schema, function);
                stored.clear(function);
            }
            let derived = schema
                .reserve_gc_local::<DirectEvalDerivedBindings, Nullable>(function)
                .initialize_null(schema, function);
            if let Some(activation) = self.lexical_derived_activation.cloned() {
                let this_storage = self.derived_activation_storage(&activation.this_binding)?;
                let status_storage =
                    self.derived_activation_storage(&activation.this_status_binding)?;
                let target_storage =
                    self.derived_activation_storage(&activation.new_target_binding)?;
                let active_storage =
                    self.derived_activation_storage(&activation.active_function_binding)?;
                let this_cell = self.emit_direct_eval_binding_cell(this_storage, function)?;
                let status_cell = self.emit_direct_eval_binding_cell(status_storage, function)?;
                let target_cell = self.emit_direct_eval_binding_cell(target_storage, function)?;
                let active_cell = self.emit_direct_eval_binding_cell(active_storage, function)?;
                derived.replace(
                    schema
                        .struct_type::<DirectEvalDerivedBindings>()
                        .construct(
                            (
                                GcOperand::reference(&this_cell, schema),
                                GcOperand::reference(&status_cell, schema),
                                GcOperand::reference(&target_cell, schema),
                                GcOperand::reference(&active_cell, schema),
                            ),
                            function,
                        )
                        .nullable(),
                    function,
                );
                active_cell.clear(function);
                target_cell.clear(function);
                status_cell.clear(function);
                this_cell.clear(function);
            }
            let stored_home = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&home, function),
                function,
            );
            let undefined = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&this_value, function),
                function,
            );
            context.replace(
                schema
                    .struct_type::<DirectEvalExecutionContext>()
                    .construct(
                        (
                            GcOperand::reference(&class_context, schema),
                            GcOperand::reference(&stored_home, schema),
                            GcOperand::reference(&derived, schema),
                            GcOperand::reference(&undefined, schema),
                            GcOperand::reference(&undefined, schema),
                        ),
                        function,
                    )
                    .nullable(),
                function,
            );
            undefined.clear(function);
            stored_home.clear(function);
            derived.clear(function);
            home.clear(function);
            class_context.clear(function);
        }
        let derived = schema
            .reserve_gc_local::<DirectEvalDerivedBindings, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<DirectEvalExecutionContext>()
                    .field(DirectEvalExecutionContextSchema::DERIVED_BINDINGS)
                    .read(&context, schema, function)
                    .reference(),
                function,
            );
        derived.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        self.compile_this_to_locals(&this_value, function)?;
        self.compile_new_target_to_locals(&new_target, function)?;
        function.instruction(&Instruction::Else);
        // Capture the cell value without GetThisBinding's uninitialized-this
        // check: eval("super()") is legal before the constructor binds this.
        for (field, output) in [
            (DirectEvalDerivedBindingsSchema::THIS_CELL, &this_value),
            (
                DirectEvalDerivedBindingsSchema::NEW_TARGET_CELL,
                &new_target,
            ),
        ] {
            let cell = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<DirectEvalDerivedBindings>()
                    .field(field)
                    .read(&derived, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<BindingCell>()
                .read_value(&cell, output, schema, function);
            cell.clear(function);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for (field, value) in [
            (DirectEvalExecutionContextSchema::THIS_SNAPSHOT, &this_value),
            (
                DirectEvalExecutionContextSchema::NEW_TARGET_SNAPSHOT,
                &new_target,
            ),
        ] {
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(value, function),
                function,
            );
            schema
                .struct_type::<DirectEvalExecutionContext>()
                .field(field)
                .write(
                    &context,
                    GcOperand::reference(&stored, schema),
                    schema,
                    function,
                );
            stored.clear(function);
        }
        derived.clear(function);
        Ok(DirectEvalInvocationLocals {
            this_value,
            new_target,
            context,
        })
    }

    pub(crate) fn release_direct_eval_invocation(
        &mut self,
        invocation: DirectEvalInvocationLocals,
        function: &mut Function,
    ) {
        invocation.context.clear(function);
        invocation.new_target.clear(function);
        invocation.this_value.clear(function);
    }
}
