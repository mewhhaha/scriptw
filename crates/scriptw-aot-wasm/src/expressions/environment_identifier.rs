use super::*;
use crate::environments::environment_reference::EnvironmentIdentifierRead;
use crate::functions::CallContinuation;
use crate::gc_types::Environment;
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

    /// An already-evaluated operand owns initialized compiler storage, not a
    /// source declaration or TDZ. The scope retains its arena ID until release.
    pub(crate) fn retain_expression_operand(
        &mut self,
        name: &str,
        value: &ValueLocals,
        function: &mut Function,
    ) -> (TypedExpr, crate::emit::LocalBindingId) {
        let storage = self.allocate_compiler_temporary_binding(value, function);
        let BindingStorage::Local(id) = storage else {
            unreachable!("local binding allocation returns its owned arena identity")
        };
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

    /// ResolveBinding and GetValue of a dynamic identifier read. A read that
    /// starts at the Global Environment Record first tries the global
    /// object's remembered entry; the shared runtime owns everything else.
    fn emit_environment_identifier_read_value(
        &mut self,
        identifier: &EnvironmentIdentifierIr,
        key: &GcLocal<StringValue>,
        read: EnvironmentIdentifierRead,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let record = self.environment_identifier_read_record(identifier, function)?;
        if !matches!(
            identifier.resolution_start(),
            EnvironmentIdentifierResolutionStart::GlobalEnvironment
        ) {
            return self.emit_environment_identifier_read_from_record(
                key,
                identifier.strictness,
                read,
                record,
                value,
                function,
            );
        }
        let schema = self.runtime_schema();
        let found = self.emit_global_object_hint_read_i32(&record, key, value, function)?;
        found.load(function);
        function.instruction(&Instruction::I32Eqz);
        schema.release_i32_local(found, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_environment_identifier_read_from_record(
            key,
            identifier.strictness,
            read,
            record,
            value,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// The record where a dynamic read's ResolveBinding starts.
    fn environment_identifier_read_record(
        &mut self,
        identifier: &EnvironmentIdentifierIr,
        function: &mut Function,
    ) -> Result<GcLocal<Environment, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        match identifier.resolution_start() {
            EnvironmentIdentifierResolutionStart::CurrentEnvironment => {
                Ok(self.resolve_env_handle_local(0, function))
            }
            EnvironmentIdentifierResolutionStart::GlobalEnvironment => {
                let global = self.emit_source_global_environment_to_local(function);
                let record = schema
                    .reserve_gc_local(function)
                    .initialize(global.load(schema, function).nullable(), function);
                global.clear(function);
                Ok(record)
            }
            EnvironmentIdentifierResolutionStart::Cell(proof) => {
                Err(EmitError::unsupported(format!(
                    "compiler invariant: proven cell read for {} missed its direct path",
                    proof.storage_name()
                )))
            }
        }
    }

    /// The value that `typeof` inspects for an identifier Reference: the
    /// identifier's own typeof read, where an unresolvable name is undefined.
    /// A literal comparison then needs no typeof String.
    pub(crate) fn compile_typeof_reference_value(
        &mut self,
        reference: &TypedExpr,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match &reference.expr {
            ExprIr::TypeOfUnresolvedIdentifier { name } => self.emit_global_identifier_read(
                name,
                EnvironmentIdentifierRead::Typeof,
                value,
                function,
            ),
            ExprIr::EnvironmentIdentifier(identifier)
                if matches!(
                    identifier.operation,
                    EnvironmentIdentifierOperationIr::Typeof
                ) =>
            {
                if let EnvironmentIdentifierResolutionStart::Cell(proof) =
                    &identifier.resolution_start()
                {
                    return self.emit_read_module_cell_identifier(
                        proof,
                        &identifier.name,
                        value,
                        function,
                    );
                }
                let schema = self.runtime_schema();
                let key = schema
                    .reserve_gc_local::<StringValue, NonNullable>(function)
                    .initialize(
                        self.emit_interned_string_reference(&identifier.name, function)?,
                        function,
                    );
                self.emit_environment_identifier_read_value(
                    identifier,
                    &key,
                    EnvironmentIdentifierRead::Typeof,
                    value,
                    function,
                )?;
                key.clear(function);
                Ok(())
            }
            _ => Err(EmitError::unsupported(
                "compiler invariant: a typeof Reference operand is an identifier",
            )),
        }
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
        if let EnvironmentIdentifierResolutionStart::Cell(proof) = &resolution {
            if matches!(
                identifier.operation,
                Operation::Assign { .. }
                    | Operation::Update { .. }
                    | Operation::EagerCompound { .. }
                    | Operation::LogicalCompound { .. }
            ) {
                // Resolving this identity cannot run source code, and the
                // cell's own INITIALIZED, MUTABLE and IMMUTABLE_STRICT words
                // decide GetBindingValue and SetMutableBinding: no pooled key,
                // generic Reference or by-name runtime call.
                let storage = self.module_cell_storage(proof, &identifier.name)?;
                let value = schema.reserve_value_local(function);
                if let Operation::Assign { value: rhs } = &identifier.operation {
                    self.compile_expr_to_value(rhs, &value, function)?;
                    self.write_binding_from_locals_checked(storage, &value, function)?;
                } else {
                    self.read_binding_to_locals(storage, &value, function)?;
                    self.emit_identifier_mutation(
                        &identifier.operation,
                        IdentifierPut::Cell(storage),
                        &value,
                        function,
                    )?;
                }
                output.copy_from(&value, function);
                value.clear(function);
                return Ok(());
            }
        }
        if matches!(identifier.strictness, Strictness::Strict) {
            if let (
                EnvironmentIdentifierResolutionStart::Cell(proof),
                Operation::Call {
                    args,
                    direct_eval: None,
                    callee_targets,
                },
            ) = (&resolution, &identifier.operation)
            {
                let callee = schema.reserve_value_local(function);
                self.emit_read_module_cell_identifier(proof, &identifier.name, &callee, function)?;
                let receiver = schema.reserve_value_local(function);
                receiver.set_undefined(function);
                self.emit_resolved_identifier_call(
                    &callee,
                    &receiver,
                    args,
                    callee_targets,
                    continuation,
                    output,
                    function,
                )?;
                receiver.clear(function);
                callee.clear(function);
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
                self.emit_environment_identifier_read_value(
                    identifier, &key, read, &value, function,
                )?;
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
                    Operation::Update { .. }
                    | Operation::EagerCompound { .. }
                    | Operation::LogicalCompound { .. } => self.emit_identifier_mutation(
                        operation,
                        IdentifierPut::Reference(&reference),
                        &value,
                        function,
                    )?,
                    Operation::Call {
                        args,
                        direct_eval,
                        callee_targets,
                    } => {
                        let receiver = schema.reserve_value_local(function);
                        self.emit_environment_identifier_call_base(&reference, &receiver, function);
                        if direct_eval.is_none() {
                            self.emit_resolved_identifier_call(
                                &value,
                                &receiver,
                                args,
                                callee_targets,
                                continuation,
                                &value,
                                function,
                            )?;
                        } else {
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
                        }
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

    /// Both paths dispatch the held callee; a target hint never replaces its value.
    fn emit_resolved_identifier_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        args: &[TypedExpr],
        callee_targets: &scriptw_ir::FunctionTargetKnowledge,
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let targets = callee_targets.known_targets();
        let known_target = if targets.len() == 1 {
            targets
                .iter()
                .next()
                .and_then(|id| self.functions.get(id))
                .cloned()
        } else {
            None
        };
        let handled = match known_target.as_ref() {
            Some(meta) => self.try_emit_known_leaf_call_from_locals(
                meta,
                callee,
                receiver,
                args,
                continuation,
                output,
                function,
            )?,
            None => false,
        };
        if !handled {
            self.emit_indirect_call_from_locals_with_known_target(
                callee,
                Some(receiver),
                args,
                known_target.as_ref(),
                continuation,
                output,
                function,
            )?;
        }
        Ok(())
    }
}

/// Where an identifier mutation stores its result.
enum IdentifierPut<'a> {
    /// A resolved Reference, through PutValue.
    Reference(&'a crate::environments::environment_reference::EnvironmentIdentifierReference),
    /// A statically proven cell: SetMutableBinding's initialization and
    /// mutability checks run inline on the cell's own words.
    Cell(BindingStorage),
}

impl FunctionBuilder<'_> {
    fn emit_identifier_put(
        &mut self,
        target: &IdentifierPut<'_>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match target {
            IdentifierPut::Reference(reference) => {
                self.emit_environment_identifier_put(reference, value, function)
            }
            IdentifierPut::Cell(storage) => {
                self.write_binding_from_locals_checked(*storage, value, function)
            }
        }
    }

    /// An update, compound or logical assignment after GetValue left the
    /// old value in `value`: computes the result into `value` and stores it
    /// to `target`; a postfix update then leaves the old numeric value.
    fn emit_identifier_mutation(
        &mut self,
        operation: &EnvironmentIdentifierOperationIr,
        target: IdentifierPut<'_>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use EnvironmentIdentifierOperationIr as Operation;
        let schema = self.runtime_schema();
        match operation {
            Operation::Update {
                operation,
                return_mode,
            } => {
                let pending = schema.reserve_completion(function);
                pending.initialize(function);
                self.emit_value_to_numeric_locals(value, &pending, function)?;
                self.completion().copy_from(&pending, function);
                self.emit_propagate_current_throw_if_needed(function);
                value.copy_from(pending.value(), function);
                pending.clear(function);
                let old = schema.reserve_value_local(function);
                old.copy_from(value, function);
                self.emit_numeric_update_to_locals(
                    *operation,
                    NumericUpdateValueKind::Dynamic,
                    &old,
                    value,
                    function,
                )?;
                self.emit_identifier_put(&target, value, function)?;
                if *return_mode == UpdateReturnMode::Postfix {
                    value.copy_from(&old, function);
                }
                old.clear(function);
            }
            Operation::EagerCompound { operation, rhs } => {
                self.push_scope();
                let (lhs, id) =
                    self.retain_expression_operand("\0environment.old", value, function);
                let result = match operation {
                    EnvironmentCompoundOperationIr::Add => {
                        self.compile_coercive_add_to_locals(&lhs, rhs, value, function)
                    }
                    EnvironmentCompoundOperationIr::Arithmetic(op) => self
                        .compile_coercive_binary_number_to_locals(
                            *op, &lhs, rhs, value, function,
                        ),
                    EnvironmentCompoundOperationIr::Bitwise(op) => self
                        .compile_bitwise_numeric_to_locals(
                            *op, &lhs, rhs, value, function,
                        ),
                };
                self.pop_scope();
                self.release_local_binding(id, function);
                result?;
                self.emit_identifier_put(&target, value, function)?;
            }
            Operation::LogicalCompound { operation, rhs } => {
                match operation {
                    LogicalBinaryOp::And | LogicalBinaryOp::Or => {
                        self.compile_truthy_tagged_i32(value, function)?;
                        if *operation == LogicalBinaryOp::Or {
                            function.instruction(&Instruction::I32Eqz);
                        }
                    }
                    LogicalBinaryOp::Coalesce => {
                        self.compile_nullish_tagged_i32(value.tag(), function)?
                    }
                }
                self.open_frame(ControlFrameKind::If, function);
                self.compile_expr_to_value(rhs, value, function)?;
                self.emit_identifier_put(&target, value, function)?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            _ => {
                return Err(EmitError::unsupported(
                    "compiler invariant: only update, compound and logical assignments mutate after GetValue",
                ))
            }
        }
        Ok(())
    }
}
