use super::*;
use crate::environments::environment_reference::EnvironmentIdentifierRead;
use crate::functions::CallContinuation;
use scriptw_ir::{
    EnvironmentCompoundOperationIr, EnvironmentIdentifierIr, EnvironmentIdentifierOperationIr,
    EnvironmentIdentifierResolutionStart,
};

impl FunctionBuilder<'_> {
    pub(crate) fn compile_global_identifier_assignment_to_value(
        &mut self,
        name: &str,
        rhs: &TypedExpr,
        strictness: Strictness,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let key = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
        let reference = self.emit_resolve_global_identifier(&key, strictness, function)?;
        let value = schema.reserve_value_local(function);
        self.compile_expr_to_value(rhs, &value, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_environment_identifier_put(&reference, &value, function)?;
        output.copy_from(&value, function);
        self.release_environment_identifier_reference(reference, function);
        value.clear(function);
        key.clear(function);
        Ok(())
    }

    /// An expression operand lives in the real local-binding arena. The scope
    /// keeps only its copied ID; releasing that ID clears the owned reference.
    pub(crate) fn retain_expression_operand(
        &mut self,
        name: &str,
        value: &ValueLocals,
        function: &mut Function,
    ) -> (TypedExpr, crate::emit::LocalBindingId) {
        let storage = self.allocate_local_binding(BindingMode::Let, function);
        let BindingStorage::Local(id) = storage else {
            unreachable!("local binding allocation returns its owned arena identity")
        };
        self.write_binding_from_locals(storage, value, function);
        self.binding_scopes
            .last_mut()
            .expect("operand scope exists")
            .insert(name.to_string(), storage);
        (
            TypedExpr {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: scriptw_ir::FunctionTargetKnowledge::unknown(),
                expr: ExprIr::Identifier(name.to_string()),
            },
            id,
        )
    }

    pub(crate) fn compile_environment_identifier_to_value(
        &mut self,
        identifier: &EnvironmentIdentifierIr,
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        use EnvironmentIdentifierOperationIr as Operation;
        let resolution = identifier.resolution_start();
        if matches!(identifier.operation, Operation::Read | Operation::Typeof) {
            if let EnvironmentIdentifierResolutionStart::Cell(proof) = &resolution {
                // Resolving this identity cannot run source code. Read its
                // existing cell without a pooled key or generic Reference;
                // initialization and abrupt completion retain the normal path.
                let value = schema.reserve_value_local(function);
                self.emit_read_module_cell_identifier(proof, &identifier.name, &value, function)?;
                if matches!(identifier.operation, Operation::Typeof) {
                    self.emit_typeof_value(&value, output, function)?;
                } else {
                    output.copy_from(&value, function);
                }
                value.clear(function);
                return Ok(());
            }
        }
        let key = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.emit_interned_string_reference(&identifier.name, function)?,
                function,
            );
        let value = schema.reserve_value_local(function);
        match &identifier.operation {
            Operation::AssignWithGlobalFallback {
                selection,
                value: rhs,
            } => {
                let selected = schema.reserve_value_local(function);
                self.compile_expr_to_value(selection, &selected, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                let reference = self.emit_resolve_selected_with_global_fallback_identifier(
                    &key,
                    &selected,
                    identifier.strictness,
                    function,
                )?;
                selected.clear(function);
                self.compile_expr_to_value(rhs, &value, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.emit_environment_identifier_put(&reference, &value, function)?;
                output.copy_from(&value, function);
                self.release_environment_identifier_reference(reference, function);
                value.clear(function);
                key.clear(function);
                return Ok(());
            }
            Operation::CaptureAssignmentReference { capture } => {
                self.emit_capture_identifier_reference(
                    capture,
                    &key,
                    identifier.strictness,
                    &value,
                    function,
                )?;
                output.copy_from(&value, function);
                value.clear(function);
                key.clear(function);
                return Ok(());
            }
            Operation::PutCapturedReference {
                reference: captured,
                value: rhs,
            } => {
                let reference = self.emit_take_captured_identifier_reference(
                    captured,
                    identifier.strictness,
                    function,
                )?;
                self.compile_expr_to_value(rhs, &value, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.emit_environment_identifier_put(&reference, &value, function)?;
                output.copy_from(&value, function);
                self.release_environment_identifier_reference(reference, function);
                value.clear(function);
                key.clear(function);
                return Ok(());
            }
            Operation::ReleaseCapturedReference { reference } => {
                self.emit_release_captured_identifier_reference(reference, function)?;
                output.set_undefined(function);
                value.clear(function);
                key.clear(function);
                return Ok(());
            }
            Operation::Read | Operation::Typeof => {
                let read = if matches!(identifier.operation, Operation::Typeof) {
                    EnvironmentIdentifierRead::Typeof
                } else {
                    EnvironmentIdentifierRead::Value
                };
                // No source evaluation separates ResolveBinding and GetValue.
                // Dynamic names keep both steps inside the shared runtime.
                let record = match identifier.resolution_start() {
                    EnvironmentIdentifierResolutionStart::CurrentEnvironment => {
                        Some(self.resolve_env_handle_local(0, function))
                    }
                    EnvironmentIdentifierResolutionStart::GlobalEnvironment => {
                        let global = self.emit_source_global_environment_to_local(function);
                        let record = schema
                            .reserve_gc_local(function)
                            .initialize(global.load(schema, function).nullable(), function);
                        global.clear(function);
                        Some(record)
                    }
                    EnvironmentIdentifierResolutionStart::Cell(proof) => {
                        return Err(EmitError::unsupported(format!(
                            "compiler invariant: proven cell read for {} missed its direct path",
                            proof.storage_name()
                        )));
                    }
                };
                if let Some(record) = record {
                    self.emit_environment_identifier_read_from_record(
                        &key,
                        identifier.strictness,
                        read,
                        record,
                        &value,
                        function,
                    )?;
                }
                if matches!(identifier.operation, Operation::Typeof) {
                    self.emit_typeof_value(&value, output, function)?;
                } else {
                    output.copy_from(&value, function);
                }
                value.clear(function);
                key.clear(function);
                return Ok(());
            }
            Operation::CaptureCallReference { .. }
            | Operation::Assign { .. }
            | Operation::Delete
            | Operation::Update { .. }
            | Operation::EagerCompound { .. }
            | Operation::LogicalCompound { .. }
            | Operation::Call { .. } => {}
        }
        let reference = match identifier.resolution_start() {
            EnvironmentIdentifierResolutionStart::CurrentEnvironment => {
                self.emit_resolve_environment_identifier(&key, identifier.strictness, function)?
            }
            EnvironmentIdentifierResolutionStart::GlobalEnvironment => {
                self.emit_resolve_global_identifier(&key, identifier.strictness, function)?
            }
            EnvironmentIdentifierResolutionStart::Cell(proof) => self
                .emit_resolve_module_cell_identifier(
                    &proof,
                    &identifier.name,
                    &key,
                    identifier.strictness,
                    function,
                )?,
        };
        match &identifier.operation {
            Operation::Assign { value: rhs } => {
                self.compile_expr_to_value(rhs, &value, function)?;
                self.emit_environment_identifier_put(&reference, &value, function)?;
            }
            Operation::Delete => {
                let deleted = schema.reserve_i32_local(function);
                self.emit_environment_identifier_delete(&reference, deleted, function)?;
                value.set_boolean(deleted, function);
                schema.release_i32_local(deleted, function);
            }
            operation => {
                let read = if matches!(operation, Operation::Typeof) {
                    EnvironmentIdentifierRead::Typeof
                } else {
                    EnvironmentIdentifierRead::Value
                };
                self.emit_environment_identifier_get(&reference, read, &value, function)?;
                match operation {
                    Operation::Read => {}
                    Operation::CaptureCallReference { receiver: captured } => {
                        let receiver = schema.reserve_value_local(function);
                        self.emit_environment_identifier_call_base(&reference, &receiver, function);
                        let storage = self
                            .lookup_binding(captured.storage_name())
                            .expect("call Reference owns its activation receiver");
                        self.write_binding_from_locals(storage, &receiver, function);
                        receiver.clear(function);
                    }
                    Operation::Typeof => {
                        self.emit_typeof_value(&value, output, function)?;
                        value.copy_from(output, function);
                    }
                    Operation::Update {
                        operation,
                        return_mode,
                    } => {
                        let pending = schema.reserve_completion(function);
                        pending.initialize(function);
                        self.emit_value_to_numeric_locals(&value, &pending, function)?;
                        self.completion().copy_from(&pending, function);
                        self.emit_propagate_current_throw_if_needed(function);
                        value.copy_from(pending.value(), function);
                        pending.clear(function);
                        let old = schema.reserve_value_local(function);
                        old.copy_from(&value, function);
                        self.emit_numeric_update_to_locals(
                            *operation,
                            NumericUpdateValueKind::Dynamic,
                            &old,
                            &value,
                            function,
                        )?;
                        self.emit_environment_identifier_put(&reference, &value, function)?;
                        if *return_mode == UpdateReturnMode::Postfix {
                            value.copy_from(&old, function);
                        }
                        old.clear(function);
                    }
                    Operation::EagerCompound { operation, rhs } => {
                        self.push_scope();
                        let (lhs, id) =
                            self.retain_expression_operand("\0environment.old", &value, function);
                        let result = match operation {
                            EnvironmentCompoundOperationIr::Add => {
                                self.compile_coercive_add_to_locals(&lhs, rhs, &value, function)
                            }
                            EnvironmentCompoundOperationIr::Arithmetic(op) => self
                                .compile_coercive_binary_number_to_locals(
                                    *op, &lhs, rhs, &value, function,
                                ),
                            EnvironmentCompoundOperationIr::Bitwise(op) => self
                                .compile_bitwise_numeric_to_locals(
                                    *op, &lhs, rhs, &value, function,
                                ),
                        };
                        self.pop_scope();
                        self.release_local_binding(id, function);
                        result?;
                        self.emit_environment_identifier_put(&reference, &value, function)?;
                    }
                    Operation::LogicalCompound { operation, rhs } => {
                        match operation {
                            LogicalBinaryOp::And | LogicalBinaryOp::Or => {
                                self.compile_truthy_tagged_i32(&value, function)?;
                                if *operation == LogicalBinaryOp::Or {
                                    function.instruction(&Instruction::I32Eqz);
                                }
                            }
                            LogicalBinaryOp::Coalesce => {
                                self.compile_nullish_tagged_i32(value.tag(), function)?
                            }
                        }
                        self.open_frame(ControlFrameKind::If, function);
                        self.compile_expr_to_value(rhs, &value, function)?;
                        self.emit_environment_identifier_put(&reference, &value, function)?;
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                    }
                    Operation::Call { args, direct_eval } => {
                        let receiver = schema.reserve_value_local(function);
                        self.emit_environment_identifier_call_base(&reference, &receiver, function);
                        self.push_scope();
                        let (callee, callee_id) = self.retain_expression_operand(
                            "\0environment.callee",
                            &value,
                            function,
                        );
                        let (this, this_id) = self.retain_expression_operand(
                            "\0environment.receiver",
                            &receiver,
                            function,
                        );
                        let result = (|| {
                            self.emit_indirect_call(
                                &callee,
                                Some(&this),
                                args,
                                None,
                                direct_eval.as_ref(),
                                continuation,
                                &value,
                                function,
                            )
                        })();
                        self.pop_scope();
                        self.release_local_binding(this_id, function);
                        self.release_local_binding(callee_id, function);
                        result?;
                        receiver.clear(function);
                    }
                    Operation::Assign { .. }
                    | Operation::AssignWithGlobalFallback { .. }
                    | Operation::Delete
                    | Operation::CaptureAssignmentReference { .. }
                    | Operation::PutCapturedReference { .. }
                    | Operation::ReleaseCapturedReference { .. } => {
                        unreachable!("captured and write-only operations precede generic GetValue")
                    }
                }
            }
        }
        output.copy_from(&value, function);
        self.release_environment_identifier_reference(reference, function);
        value.clear(function);
        key.clear(function);
        Ok(())
    }
}
