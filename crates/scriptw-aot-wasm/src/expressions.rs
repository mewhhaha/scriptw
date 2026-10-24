use super::*;
use crate::gc_types::{
    CompletionLocals, GcLocal, GcOperand, I32Local, NonNullable, Nullable, ScalarValue,
    StringValue, ValueLocals,
};
use crate::operations::PropertyKeyLocals;
use scriptw_ir::{NativeErrorKind, StaticRegExpCompilation};

mod environment_identifier;
mod foreign_call;
mod optional_chain;
mod regexp_program;
mod super_property_mutation;

#[must_use = "raw Reference operands must enter their checked transition"]
struct EvaluatedRawOrdinaryPropertyReferenceLocals {
    base_and_receiver: ValueLocals,
    referenced_name: ValueLocals,
}

#[must_use = "assignment operands must enter PutValue"]
struct EvaluatedRawOrdinaryPropertyAssignmentLocals {
    reference: EvaluatedRawOrdinaryPropertyReferenceLocals,
    rhs: ValueLocals,
}

#[must_use = "a canonical Reference must be consumed and released"]
struct CanonicalOrdinaryPropertyReferenceLocals {
    base_and_receiver: ValueLocals,
    target_object: ValueLocals,
    property_key: PropertyKeyLocals,
}
impl CanonicalOrdinaryPropertyReferenceLocals {
    fn clear(self, function: &mut Function) {
        self.property_key.clear(function);
        self.target_object.clear(function);
        self.base_and_receiver.clear(function);
    }
}

#[must_use = "a canonical assignment must publish only after normal Set"]
struct ReadyToWriteOrdinaryPropertyAssignmentLocals {
    reference: CanonicalOrdinaryPropertyReferenceLocals,
    rhs: ValueLocals,
}

#[must_use = "a read Reference retains its old value across RHS evaluation"]
struct ReadOrdinaryPropertyReferenceLocals {
    reference: CanonicalOrdinaryPropertyReferenceLocals,
    old_value: ValueLocals,
}

#[must_use = "an eager result must enter PutValue before publication"]
struct ReadyToWriteOrdinaryPropertyReferenceLocals {
    reference: CanonicalOrdinaryPropertyReferenceLocals,
    old_value: ValueLocals,
    result: ValueLocals,
}

#[must_use = "a numeric Reference must advance before it can be written"]
struct ReadOrdinaryPropertyNumericUpdateLocals {
    reference: CanonicalOrdinaryPropertyReferenceLocals,
    old_value: ValueLocals,
}

#[must_use = "a numeric update must enter PutValue before publication"]
struct ReadyToWriteOrdinaryPropertyNumericUpdateLocals {
    reference: CanonicalOrdinaryPropertyReferenceLocals,
    old_value: ValueLocals,
    new_value: ValueLocals,
}

/// The sealed input required by the shared ordinary Reference evaluator.
///
/// The fused IR carriers and retained Get capture own the same base/raw-key
/// pair. Keeping this trait private makes that pair the only reusable part of their
/// distinct plain-assignment, logical-assignment, compound-assignment and
/// numeric-update lifecycles.
trait OrdinaryPropertyReferenceSource {
    fn base_and_receiver(&self) -> &TypedExpr;
    fn referenced_name(&self) -> &PropertyKeyIr;
}

impl OrdinaryPropertyReferenceSource for OrdinaryPropertyAssignmentIr {
    fn base_and_receiver(&self) -> &TypedExpr {
        self.base_and_receiver()
    }

    fn referenced_name(&self) -> &PropertyKeyIr {
        self.referenced_name()
    }
}

impl OrdinaryPropertyReferenceSource for OrdinaryPropertyLogicalAssignmentIr {
    fn base_and_receiver(&self) -> &TypedExpr {
        self.base_and_receiver()
    }

    fn referenced_name(&self) -> &PropertyKeyIr {
        self.referenced_name()
    }
}

impl OrdinaryPropertyReferenceSource for OrdinaryPropertyGetCaptureIr {
    fn base_and_receiver(&self) -> &TypedExpr {
        self.base_and_receiver()
    }
    fn referenced_name(&self) -> &PropertyKeyIr {
        self.referenced_name()
    }
}

impl OrdinaryPropertyReferenceSource for OrdinaryPropertyEagerCompoundAssignmentIr {
    fn base_and_receiver(&self) -> &TypedExpr {
        self.base_and_receiver()
    }

    fn referenced_name(&self) -> &PropertyKeyIr {
        self.referenced_name()
    }
}

impl OrdinaryPropertyReferenceSource for OrdinaryPropertyNumericUpdateIr {
    fn base_and_receiver(&self) -> &TypedExpr {
        self.base_and_receiver()
    }

    fn referenced_name(&self) -> &PropertyKeyIr {
        self.referenced_name()
    }
}

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn with_reference_strictness(
        &mut self,
        strictness: Strictness,
        function: &mut Function,
        body: impl FnOnce(&mut Self, &mut Function) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let strict = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(i32::from(
            strictness.throws_on_failed_set(),
        )));
        strict.store(function);
        let previous = self.object_write_strict_flag_local.replace(strict);
        let result = body(self, function);
        self.object_write_strict_flag_local = previous;
        schema.release_i32_local(strict, function);
        result
    }

    fn emit_expression_native_error(
        &mut self,
        kind: NativeErrorKind,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let pending = self.runtime_schema().reserve_completion(function);
        pending.initialize(function);
        self.emit_throw_runtime_error_to_active_handler(kind, message, &pending, function)?;
        pending.clear(function);
        Ok(())
    }

    fn compile_raw_property_key_expression_to_value(
        &mut self,
        key: &PropertyKeyIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match key {
            PropertyKeyIr::StaticString(value) => {
                let schema = self.runtime_schema();
                let text = schema
                    .reserve_gc_local::<StringValue, NonNullable>(function)
                    .initialize(
                        self.emit_interned_string_reference(value, function)?,
                        function,
                    );
                output.set_reference(&text, schema, function);
                text.clear(function);
            }
            PropertyKeyIr::StringExpr(expr) | PropertyKeyIr::ArrayIndex(expr) => {
                self.compile_expr_to_value(expr, output, function)?
            }
            PropertyKeyIr::ArrayLength => {
                let schema = self.runtime_schema();
                let text = schema
                    .reserve_gc_local::<StringValue, NonNullable>(function)
                    .initialize(
                        self.emit_interned_string_reference("length", function)?,
                        function,
                    );
                output.set_reference(&text, schema, function);
                text.clear(function);
            }
        }
        Ok(())
    }

    fn evaluate_raw_ordinary_property_reference(
        &mut self,
        reference: &impl OrdinaryPropertyReferenceSource,
        function: &mut Function,
    ) -> Result<EvaluatedRawOrdinaryPropertyReferenceLocals, EmitError> {
        let schema = self.runtime_schema();
        let base_and_receiver = schema.reserve_value_local(function);
        let referenced_name = schema.reserve_value_local(function);
        self.compile_expr_to_value(reference.base_and_receiver(), &base_and_receiver, function)?;
        self.compile_raw_property_key_expression_to_value(
            reference.referenced_name(),
            &referenced_name,
            function,
        )?;
        Ok(EvaluatedRawOrdinaryPropertyReferenceLocals {
            base_and_receiver,
            referenced_name,
        })
    }

    fn canonicalize_ordinary_property_reference(
        &mut self,
        reference: EvaluatedRawOrdinaryPropertyReferenceLocals,
        nullish_message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<CanonicalOrdinaryPropertyReferenceLocals, EmitError> {
        let EvaluatedRawOrdinaryPropertyReferenceLocals {
            base_and_receiver,
            referenced_name,
        } = reference;
        let schema = self.runtime_schema();
        self.compile_nullish_tagged_i32(base_and_receiver.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_expression_native_error(NativeErrorKind::TypeError, nullish_message, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // The boxed target and original primitive receiver remain distinct.
        // Nullish rejection precedes observable ToPropertyKey in this transition.
        let target_object = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        self.emit_value_to_object_locals(&base_and_receiver, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        target_object.copy_from(pending.value(), function);
        pending.clear(function);
        let property_key = self.emit_value_to_property_key_locals(&referenced_name, function)?;
        referenced_name.clear(function);
        Ok(CanonicalOrdinaryPropertyReferenceLocals {
            base_and_receiver,
            target_object,
            property_key,
        })
    }

    fn emit_get_value_from_raw_ordinary_property_reference(
        &mut self,
        reference: EvaluatedRawOrdinaryPropertyReferenceLocals,
        function: &mut Function,
    ) -> Result<ReadOrdinaryPropertyReferenceLocals, EmitError> {
        let schema = self.runtime_schema();
        let old_value = schema.reserve_value_local(function);
        let reference = self.canonicalize_ordinary_property_reference(
            reference,
            RuntimeErrorMessage::CANNOT_READ_PROPERTIES_OF_NULL_OR_UNDEFINED,
            function,
        )?;
        schema
            .call_helper(
                crate::runtime_helpers::ObjectReadArguments::new(
                    &reference.target_object,
                    &reference.base_and_receiver,
                    &reference.property_key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(self.completion(), function);
        self.emit_propagate_current_throw_if_needed(function);
        old_value.copy_from(self.completion().value(), function);
        Ok(ReadOrdinaryPropertyReferenceLocals {
            reference,
            old_value,
        })
    }

    fn evaluate_rhs_for_raw_ordinary_property_assignment(
        &mut self,
        reference: EvaluatedRawOrdinaryPropertyReferenceLocals,
        assignment: &OrdinaryPropertyAssignmentIr,
        function: &mut Function,
    ) -> Result<EvaluatedRawOrdinaryPropertyAssignmentLocals, EmitError> {
        let rhs = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(assignment.rhs(), &rhs, function)?;
        Ok(EvaluatedRawOrdinaryPropertyAssignmentLocals { reference, rhs })
    }

    fn canonicalize_raw_ordinary_property_assignment(
        &mut self,
        assignment: EvaluatedRawOrdinaryPropertyAssignmentLocals,
        function: &mut Function,
    ) -> Result<ReadyToWriteOrdinaryPropertyAssignmentLocals, EmitError> {
        let EvaluatedRawOrdinaryPropertyAssignmentLocals { reference, rhs } = assignment;
        // Plain assignment evaluates the complete RHS before ToObject/key coercion.
        let reference = self.canonicalize_ordinary_property_reference(
            reference,
            RuntimeErrorMessage::CANNOT_CONVERT_UNDEFINED_OR_NULL_TO_OBJECT,
            function,
        )?;
        Ok(ReadyToWriteOrdinaryPropertyAssignmentLocals { reference, rhs })
    }

    fn emit_ordinary_reference_set(
        &mut self,
        reference: &CanonicalOrdinaryPropertyReferenceLocals,
        rhs: &ValueLocals,
        strictness: Strictness,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        schema
            .call_helper(
                crate::runtime_helpers::OrdinarySetArguments::new(
                    &reference.target_object,
                    &reference.base_and_receiver,
                    &reference.property_key,
                    rhs,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(self.completion(), function);
        self.emit_propagate_current_throw_if_needed(function);
        if strictness.throws_on_failed_set() {
            self.completion().value().scalar().load(function);
            function.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_expression_native_error(NativeErrorKind::TypeError, message, function)?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        Ok(())
    }

    fn emit_put_value_from_ready_ordinary_property_assignment(
        &mut self,
        assignment: ReadyToWriteOrdinaryPropertyAssignmentLocals,
        strictness: Strictness,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let ReadyToWriteOrdinaryPropertyAssignmentLocals { reference, rhs } = assignment;
        self.emit_ordinary_reference_set(
            &reference,
            &rhs,
            strictness,
            RuntimeErrorMessage::CANNOT_ASSIGN_TO_PROPERTY,
            function,
        )?;
        output.copy_from(&rhs, function);
        reference.clear(function);
        rhs.clear(function);
        Ok(())
    }

    fn compile_ordinary_property_assignment_to_value(
        &mut self,
        assignment: &OrdinaryPropertyAssignmentIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let raw = self.evaluate_raw_ordinary_property_reference(assignment, function)?;
        let evaluated =
            self.evaluate_rhs_for_raw_ordinary_property_assignment(raw, assignment, function)?;
        let ready = self.canonicalize_raw_ordinary_property_assignment(evaluated, function)?;
        self.emit_put_value_from_ready_ordinary_property_assignment(
            ready,
            assignment.strictness(),
            output,
            function,
        )
    }

    fn compile_ordinary_property_logical_assignment_to_value(
        &mut self,
        assignment: &OrdinaryPropertyLogicalAssignmentIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let raw = self.evaluate_raw_ordinary_property_reference(assignment, function)?;
        let ReadOrdinaryPropertyReferenceLocals {
            reference,
            old_value,
        } = self.emit_get_value_from_raw_ordinary_property_reference(raw, function)?;
        let rhs = schema.reserve_value_local(function);
        match assignment.op() {
            LogicalBinaryOp::And | LogicalBinaryOp::Or => {
                self.compile_truthy_tagged_i32(&old_value, function)?;
                if assignment.op() == LogicalBinaryOp::Or {
                    function.instruction(&Instruction::I32Eqz);
                }
            }
            LogicalBinaryOp::Coalesce => {
                self.compile_nullish_tagged_i32(old_value.tag(), function)?
            }
        }
        self.open_frame(ControlFrameKind::If, function);
        self.compile_expr_to_value(assignment.rhs(), &rhs, function)?;
        self.emit_ordinary_reference_set(
            &reference,
            &rhs,
            assignment.strictness(),
            RuntimeErrorMessage::CANNOT_ASSIGN_TO_PROPERTY,
            function,
        )?;
        output.copy_from(&rhs, function);
        function.instruction(&Instruction::Else);
        output.copy_from(&old_value, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        rhs.clear(function);
        reference.clear(function);
        old_value.clear(function);
        Ok(())
    }

    fn compile_ordinary_property_get_capture_to_value(
        &mut self,
        capture: &OrdinaryPropertyGetCaptureIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let raw = self.evaluate_raw_ordinary_property_reference(capture, function)?;
        let ReadOrdinaryPropertyReferenceLocals {
            reference,
            old_value,
        } = self.emit_get_value_from_raw_ordinary_property_reference(raw, function)?;
        for (name, value) in [
            (
                capture.receiver_storage_name(),
                &reference.base_and_receiver,
            ),
            (capture.target_storage_name(), &reference.target_object),
            (capture.key_storage_name(), reference.property_key.value()),
        ] {
            let storage = self
                .lookup_binding(name)
                .expect("Get capture owns its activation slot");
            self.write_binding_from_locals(storage, value, function);
        }
        output.copy_from(&old_value, function);
        reference.clear(function);
        old_value.clear(function);
        Ok(())
    }

    fn compile_captured_ordinary_property_write_to_value(
        &mut self,
        write: &CapturedOrdinaryPropertyWriteIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let target = schema.reserve_value_local(function);
        let key_value = schema.reserve_value_local(function);
        for (name, value) in [
            (write.receiver_storage_name(), &receiver),
            (write.target_storage_name(), &target),
            (write.key_storage_name(), &key_value),
        ] {
            let storage = self
                .lookup_binding(name)
                .expect("captured PutValue owns its activation slot");
            self.read_binding_to_locals(storage, value, function)?;
        }
        let rhs = schema.reserve_value_local(function);
        self.compile_expr_to_value(write.rhs(), &rhs, function)?;
        // Capture stored a completed String/Symbol key. Its checked fast path
        // restores the owner without repeating any user coercion.
        let key = self.emit_value_to_property_key_locals(&key_value, function)?;
        key_value.clear(function);
        let reference = CanonicalOrdinaryPropertyReferenceLocals {
            base_and_receiver: receiver,
            target_object: target,
            property_key: key,
        };
        self.emit_ordinary_reference_set(
            &reference,
            &rhs,
            write.strictness(),
            RuntimeErrorMessage::CANNOT_ASSIGN_TO_PROPERTY,
            function,
        )?;
        output.copy_from(&rhs, function);
        reference.clear(function);
        rhs.clear(function);
        Ok(())
    }

    fn emit_result_from_read_ordinary_property_reference(
        &mut self,
        read: ReadOrdinaryPropertyReferenceLocals,
        mutation: &OrdinaryPropertyEagerCompoundAssignmentIr,
        function: &mut Function,
    ) -> Result<ReadyToWriteOrdinaryPropertyReferenceLocals, EmitError> {
        let ReadOrdinaryPropertyReferenceLocals {
            reference,
            old_value,
        } = read;
        let result = self.runtime_schema().reserve_value_local(function);
        self.push_scope();
        let (_, id) =
            self.retain_expression_operand(mutation.old_value_binding(), &old_value, function);
        let compiled = self.compile_expr_to_value(mutation.result(), &result, function);
        self.pop_scope();
        self.release_local_binding(id, function);
        compiled?;
        Ok(ReadyToWriteOrdinaryPropertyReferenceLocals {
            reference,
            old_value,
            result,
        })
    }

    fn emit_put_value_from_ready_ordinary_property_reference(
        &mut self,
        ready: ReadyToWriteOrdinaryPropertyReferenceLocals,
        strictness: Strictness,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let ReadyToWriteOrdinaryPropertyReferenceLocals {
            reference,
            old_value,
            result,
        } = ready;
        self.emit_ordinary_reference_set(
            &reference,
            &result,
            strictness,
            RuntimeErrorMessage::CANNOT_ASSIGN_TO_PROPERTY,
            function,
        )?;
        output.copy_from(&result, function);
        result.clear(function);
        reference.clear(function);
        old_value.clear(function);
        Ok(())
    }

    fn compile_ordinary_property_eager_compound_assignment_to_value(
        &mut self,
        mutation: &OrdinaryPropertyEagerCompoundAssignmentIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let raw = self.evaluate_raw_ordinary_property_reference(mutation, function)?;
        let read = self.emit_get_value_from_raw_ordinary_property_reference(raw, function)?;
        let ready =
            self.emit_result_from_read_ordinary_property_reference(read, mutation, function)?;
        self.emit_put_value_from_ready_ordinary_property_reference(
            ready,
            mutation.strictness(),
            output,
            function,
        )
    }

    fn emit_numeric_reference_old_value(
        &mut self,
        value_kind: NumericUpdateValueKind,
        old: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        match value_kind {
            NumericUpdateValueKind::BigInt => {}
            NumericUpdateValueKind::Dynamic | NumericUpdateValueKind::Number => {
                let pending = schema.reserve_completion(function);
                pending.initialize(function);
                match value_kind {
                    NumericUpdateValueKind::Dynamic => {
                        self.emit_value_to_numeric_locals(old, &pending, function)?
                    }
                    NumericUpdateValueKind::Number => {
                        self.emit_value_to_number_payload(old, &pending, function)?
                    }
                    NumericUpdateValueKind::BigInt => unreachable!("handled without conversion"),
                }
                self.completion().copy_from(&pending, function);
                self.emit_propagate_current_throw_if_needed(function);
                old.copy_from(pending.value(), function);
                pending.clear(function);
            }
        }
        Ok(())
    }

    fn emit_get_numeric_value_from_raw_ordinary_property_reference(
        &mut self,
        raw: EvaluatedRawOrdinaryPropertyReferenceLocals,
        value_kind: NumericUpdateValueKind,
        function: &mut Function,
    ) -> Result<ReadOrdinaryPropertyNumericUpdateLocals, EmitError> {
        let ReadOrdinaryPropertyReferenceLocals {
            reference,
            old_value,
        } = self.emit_get_value_from_raw_ordinary_property_reference(raw, function)?;
        self.emit_numeric_reference_old_value(value_kind, &old_value, function)?;
        Ok(ReadOrdinaryPropertyNumericUpdateLocals {
            reference,
            old_value,
        })
    }

    fn emit_numeric_update_from_read_ordinary_property_reference(
        &mut self,
        read: ReadOrdinaryPropertyNumericUpdateLocals,
        update: &OrdinaryPropertyNumericUpdateIr,
        function: &mut Function,
    ) -> Result<ReadyToWriteOrdinaryPropertyNumericUpdateLocals, EmitError> {
        let ReadOrdinaryPropertyNumericUpdateLocals {
            reference,
            old_value,
        } = read;
        let new_value = self.runtime_schema().reserve_value_local(function);
        self.emit_numeric_update_to_locals(
            update.op(),
            update.value_kind(),
            &old_value,
            &new_value,
            function,
        )?;
        Ok(ReadyToWriteOrdinaryPropertyNumericUpdateLocals {
            reference,
            old_value,
            new_value,
        })
    }

    fn emit_put_value_from_ready_ordinary_property_numeric_update(
        &mut self,
        ready: ReadyToWriteOrdinaryPropertyNumericUpdateLocals,
        update: &OrdinaryPropertyNumericUpdateIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let ReadyToWriteOrdinaryPropertyNumericUpdateLocals {
            reference,
            old_value,
            new_value,
        } = ready;
        self.emit_ordinary_reference_set(
            &reference,
            &new_value,
            update.strictness(),
            RuntimeErrorMessage::CANNOT_ASSIGN_TO_PROPERTY,
            function,
        )?;
        output.copy_from(
            match update.return_mode() {
                UpdateReturnMode::Prefix => &new_value,
                UpdateReturnMode::Postfix => &old_value,
            },
            function,
        );
        new_value.clear(function);
        reference.clear(function);
        old_value.clear(function);
        Ok(())
    }

    fn compile_ordinary_property_numeric_update_to_value(
        &mut self,
        update: &OrdinaryPropertyNumericUpdateIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let raw = self.evaluate_raw_ordinary_property_reference(update, function)?;
        let read = self.emit_get_numeric_value_from_raw_ordinary_property_reference(
            raw,
            update.value_kind(),
            function,
        )?;
        let ready =
            self.emit_numeric_update_from_read_ordinary_property_reference(read, update, function)?;
        self.emit_put_value_from_ready_ordinary_property_numeric_update(
            ready, update, output, function,
        )
    }

    /// The sole expression publisher carries all three value words. A static
    /// IR kind never replaces an acquired callback's actual reference or tag.
    pub(crate) fn compile_expr_to_value(
        &mut self,
        expr: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        match &expr.expr {
            ExprIr::ForeignCall(call) => self.emit_foreign_call(call, output, function)?,
            ExprIr::DynamicImport {
                specifier,
                options,
                referrer,
                phase,
            } => self.emit_dynamic_import(
                *referrer,
                *phase,
                specifier,
                options.as_deref(),
                output,
                function,
            )?,
            ExprIr::ImportMeta { module } => self.emit_import_meta(*module, output, function)?,
            ExprIr::ModuleNamespace { mode, exports } => {
                self.emit_module_namespace(*mode, exports, output, function)?
            }
            ExprIr::ModuleEntryEvaluation(entry) => {
                self.emit_module_entry_evaluation(entry, output, function)?
            }
            ExprIr::ModuleExecutionGraph(graph) => {
                self.emit_module_execution_graph(graph, output, function)?
            }
            ExprIr::ModuleBindingRead(target) => {
                self.emit_module_binding_read(target, output, function)?
            }
            ExprIr::JsonModuleValue(plan) => self.emit_json_module_value(plan, output, function)?,
            ExprIr::BytesModuleValue(plan) => {
                self.emit_bytes_module_value(plan, output, function)?
            }
            ExprIr::ModuleEvaluate(plan) => self.emit_module_evaluate(plan, output, function)?,
            ExprIr::ModuleHasAsyncDependencies(plan) => {
                self.emit_module_has_async_dependencies(plan, output, function)?
            }
            ExprIr::ModuleDeferredImportEvaluate(plan) => {
                self.emit_module_deferred_import_evaluate(plan, output, function)?
            }
            ExprIr::DeferredModuleEvaluate(plan) => {
                self.emit_deferred_module_evaluate(plan, output, function)?
            }
            ExprIr::ModuleNamespacePublish {
                module,
                mode,
                namespace,
            } => {
                self.emit_module_namespace_publish(*module, *mode, namespace, function)?;
                output.set_undefined(function);
            }
            ExprIr::Undefined | ExprIr::ArrayHole => output.set_undefined(function),
            ExprIr::Null => output.set_scalar(ScalarValue::Null, function),
            ExprIr::Boolean(value) => output.set_scalar(ScalarValue::Boolean(*value), function),
            ExprIr::Number(bits) => {
                output.set_scalar(ScalarValue::NumberBits(*bits as i64), function)
            }
            ExprIr::BigInt(value) => self.compile_bigint_literal_to_value(value, output, function),
            ExprIr::WellKnownSymbol(symbol) => {
                let symbol = schema.reserve_gc_local(function).initialize(
                    self.emit_well_known_symbol_reference(*symbol, function)?,
                    function,
                );
                output.set_reference(&symbol, schema, function);
                symbol.clear(function);
            }
            ExprIr::Symbol { description } => {
                let description_slot = schema.reserve_gc_local::<StringValue, Nullable>(function);
                let description_root = match description {
                    None => description_slot.initialize_null(schema, function),
                    Some(description) => {
                        let value = schema.reserve_value_local(function);
                        self.compile_expr_to_value(description, &value, function)?;
                        let root = description_slot.initialize(
                            value
                                .cast_reference::<StringValue>(schema, function)
                                .nullable(),
                            function,
                        );
                        value.clear(function);
                        root
                    }
                };
                let symbol = schema
                    .reserve_gc_local::<crate::gc_types::SymbolValue, NonNullable>(function)
                    .initialize(
                        schema
                            .struct_type::<crate::gc_types::SymbolValue>()
                            .construct(
                                (
                                    GcOperand::reference(&description_root, schema),
                                    GcOperand::null(schema),
                                    GcOperand::i64(0),
                                ),
                                function,
                            ),
                        function,
                    );
                output.set_reference(&symbol, schema, function);
                symbol.clear(function);
                description_root.clear(function);
            }
            ExprIr::String(value) => {
                let text = schema
                    .reserve_gc_local::<StringValue, NonNullable>(function)
                    .initialize(
                        self.emit_interned_string_reference(value, function)?,
                        function,
                    );
                output.set_reference(&text, schema, function);
                text.clear(function);
            }
            ExprIr::TemplateObject(template) => {
                let array = schema
                    .reserve_gc_local::<crate::gc_types::ArrayObject, NonNullable>(function)
                    .initialize(self.emit_template_object(template, function)?, function);
                output.set_reference(&array, schema, function);
                array.clear(function);
            }
            ExprIr::RegExpLiteral {
                source,
                flags,
                static_compilation,
            } => match static_compilation {
                Some(StaticRegExpCompilation::Program(program)) => {
                    self.compile_regexp_literal_to_value(source, flags, program, output, function)?
                }
                Some(compilation @ StaticRegExpCompilation::InvalidSyntax { .. }) => self
                    .emit_expression_native_error(
                        NativeErrorKind::SyntaxError,
                        self.strings.source_runtime_error_message(
                            SourceRuntimeErrorMessage::RegExp(compilation),
                        )?,
                        function,
                    )?,
                None => {
                    self.emit_reject_runtime_semantics(
                        scriptw_ir::RuntimeSemanticRejection::Gap(
                            scriptw_ir::RuntimeSemanticGap::RegExpRuntimePatternCompilation,
                        ),
                        function,
                    );
                    output.set_undefined(function);
                }
            },
            ExprIr::FunctionValue(id)
            | ExprIr::ClassFieldNamedFunctionValue {
                function_id: id, ..
            } => {
                let meta = self.functions.get(id).cloned().ok_or_else(|| {
                    EmitError::unsupported(format!("unknown compiled function `{id}`"))
                })?;
                if let Some(slot) = meta
                    .standard_builtin
                    .and_then(crate::module::standard_builtin_constructor_realm_slot)
                {
                    // Trusted references use the Realm's installed constructor,
                    // including its semantic prototype, independent of globals.
                    let realm = self.load_current_realm(function);
                    self.emit_load_non_array_realm_intrinsic(&realm, slot, output, function);
                    realm.clear(function);
                } else {
                    let callable = schema
                        .reserve_gc_local::<crate::gc_types::FunctionObject, NonNullable>(function)
                        .initialize(self.emit_function_value_payload(&meta, function)?, function);
                    if let ExprIr::ClassFieldNamedFunctionValue { name, .. } = &expr.expr {
                        self.emit_class_field_function_name(&callable, name, function)?;
                    }
                    output.set_reference(&callable, schema, function);
                    callable.clear(function);
                }
            }
            ExprIr::This => self.compile_this_to_locals(output, function)?,
            ExprIr::ExecutionGlobalObject => {
                self.emit_execution_global_object_to_locals(output, function);
            }
            ExprIr::Arguments => {
                let storage = self
                    .lookup_binding(LEXICAL_ARGUMENTS_NAME)
                    .ok_or_else(|| EmitError::unsupported("missing arguments binding"))?;
                self.read_binding_to_locals(storage, output, function)?;
            }
            ExprIr::ObjectLiteral(properties) => {
                self.compile_object_literal_payload(properties, output, function)?
            }
            ExprIr::ObjectPropertyDefinition(definition) => {
                self.compile_object_property_definition_payload(definition, output, function)?
            }
            ExprIr::ArrayLiteral(elements) => {
                self.compile_array_literal_payload(elements, output, function)?
            }
            ExprIr::ArrayAccumulation(accumulation) => {
                self.compile_array_accumulation_payload(accumulation, output, function)?
            }
            ExprIr::Identifier(name) => self.compile_identifier_to_value(name, output, function)?,
            ExprIr::GlobalPropertyRead { name } => {
                self.emit_global_property_read(name, output, function)?
            }
            ExprIr::GlobalIdentifierRead { name } => self.emit_global_identifier_read(
                name,
                crate::environments::environment_reference::EnvironmentIdentifierRead::Value,
                output,
                function,
            )?,
            ExprIr::AssignIdentifier { name, value } => {
                if self.is_script_global_binding(name) && self.lookup_binding(name).is_none() {
                    self.compile_global_identifier_assignment_to_value(
                        name,
                        value,
                        if self.is_current_function_strict() {
                            Strictness::Strict
                        } else {
                            Strictness::Sloppy
                        },
                        output,
                        function,
                    )?;
                } else {
                    let rhs = schema.reserve_value_local(function);
                    self.compile_expr_to_value(value, &rhs, function)?;
                    let storage = self.lookup_binding(name).ok_or_else(|| {
                        EmitError::unsupported(format!("unbound identifier `{name}`"))
                    })?;
                    self.write_binding_from_locals_checked(storage, &rhs, function)?;
                    self.mirror_binding_to_global_object(name, storage, function)?;
                    output.copy_from(&rhs, function);
                    rhs.clear(function);
                }
            }
            ExprIr::GlobalPropertyWrite {
                name,
                value,
                strictness,
                ..
            } => {
                self.compile_global_identifier_assignment_to_value(
                    name,
                    value,
                    *strictness,
                    output,
                    function,
                )?;
            }
            ExprIr::PropertyWrite {
                target,
                key,
                value,
                strictness,
            } => {
                self.with_reference_strictness(*strictness, function, |emitter, function| {
                    emitter.compile_property_write_to_locals(target, key, value, output, function)
                })?;
            }
            ExprIr::OrdinaryPropertyAssignment(assignment) => {
                self.compile_ordinary_property_assignment_to_value(assignment, output, function)?
            }
            ExprIr::OrdinaryPropertyLogicalAssignment(assignment) => self
                .compile_ordinary_property_logical_assignment_to_value(
                    assignment, output, function,
                )?,
            ExprIr::OrdinaryPropertyGetCapture(capture) => {
                self.compile_ordinary_property_get_capture_to_value(capture, output, function)?
            }
            ExprIr::CapturedOrdinaryPropertyWrite(write) => {
                self.compile_captured_ordinary_property_write_to_value(write, output, function)?
            }
            ExprIr::OrdinaryPropertyNumericUpdate(update) => {
                self.compile_ordinary_property_numeric_update_to_value(update, output, function)?
            }
            ExprIr::OrdinaryPropertyEagerCompoundAssignment(mutation) => self
                .compile_ordinary_property_eager_compound_assignment_to_value(
                    mutation, output, function,
                )?,
            ExprIr::UpdateIdentifier {
                name,
                op,
                return_mode,
                value_kind,
            } => {
                let storage = self.lookup_binding(name).ok_or_else(|| {
                    EmitError::unsupported(format!("unbound identifier `{name}`"))
                })?;
                let old = schema.reserve_value_local(function);
                let new = schema.reserve_value_local(function);
                self.read_binding_to_locals(storage, &old, function)?;
                self.emit_numeric_reference_old_value(*value_kind, &old, function)?;
                self.emit_numeric_update_to_locals(*op, *value_kind, &old, &new, function)?;
                self.write_binding_from_locals_checked(storage, &new, function)?;
                self.mirror_binding_to_global_object(name, storage, function)?;
                output.copy_from(
                    match return_mode {
                        UpdateReturnMode::Prefix => &new,
                        UpdateReturnMode::Postfix => &old,
                    },
                    function,
                );
                new.clear(function);
                old.clear(function);
            }
            ExprIr::CompoundAssignIdentifier { name, op, value } => {
                let storage = self.lookup_binding(name).ok_or_else(|| {
                    EmitError::unsupported(format!("unbound identifier `{name}`"))
                })?;
                let old = schema.reserve_value_local(function);
                let result = schema.reserve_value_local(function);
                self.read_binding_to_locals(storage, &old, function)?;
                self.compile_retained_compound_result(*op, &old, value, &result, function)?;
                self.write_binding_from_locals_checked(storage, &result, function)?;
                self.mirror_binding_to_global_object(name, storage, function)?;
                output.copy_from(&result, function);
                result.clear(function);
                old.clear(function);
            }
            ExprIr::EnvironmentIdentifier(identifier) => self
                .compile_environment_identifier_to_value(
                    identifier,
                    &crate::functions::CallContinuation::Continue,
                    output,
                    function,
                )?,
            ExprIr::UnaryPlus { expr } => {
                if expr.kind == ValueKind::Number {
                    self.compile_expr_to_value(expr, output, function)?;
                } else {
                    let value = schema.reserve_value_local(function);
                    let pending = schema.reserve_completion(function);
                    pending.initialize(function);
                    self.compile_expr_to_value(expr, &value, function)?;
                    self.emit_value_to_number_payload(&value, &pending, function)?;
                    self.completion().copy_from(&pending, function);
                    self.emit_propagate_current_throw_if_needed(function);
                    output.copy_from(pending.value(), function);
                    pending.clear(function);
                    value.clear(function);
                }
            }
            ExprIr::UnaryMinusNumeric { expr } => {
                self.compile_unary_minus_numeric_to_locals(expr, output, function)?
            }
            ExprIr::UnaryBitwiseNumeric { op, expr } => {
                self.compile_unary_bitwise_numeric_to_locals(*op, expr, output, function)?
            }
            ExprIr::Void { expr: operand } | ExprIr::DeleteValue { expr: operand } => {
                let value = schema.reserve_value_local(function);
                self.compile_expr_to_value(operand, &value, function)?;
                value.clear(function);
                output.set_scalar(
                    if matches!(&expr.expr, ExprIr::DeleteValue { .. }) {
                        ScalarValue::Boolean(true)
                    } else {
                        ScalarValue::Undefined
                    },
                    function,
                );
            }
            ExprIr::DeleteIdentifier { .. } => {
                output.set_scalar(ScalarValue::Boolean(false), function)
            }
            ExprIr::DeleteGlobalProperty { name, strictness } => {
                let deleted = schema.reserve_i32_local(function);
                self.emit_global_identifier_delete(name, deleted, *strictness, function)?;
                output.set_boolean(deleted, function);
                schema.release_i32_local(deleted, function);
            }
            ExprIr::DeleteOptionalPropertyChain(delete) => {
                self.compile_delete_optional_property_chain_to_value(delete, output, function)?;
            }
            ExprIr::DeleteProperty {
                target,
                key,
                strictness,
            } => {
                let deleted = schema.reserve_i32_local(function);
                self.compile_delete_property_i32(target, key, *strictness, function)?;
                deleted.store(function);
                output.set_boolean(deleted, function);
                schema.release_i32_local(deleted, function);
            }
            ExprIr::TypeOf { expr } => self.compile_typeof_payload(expr, output, function)?,
            ExprIr::TypeOfUnresolvedIdentifier { name } => {
                let value = schema.reserve_value_local(function);
                self.emit_global_identifier_read(
                    name,
                    crate::environments::environment_reference::EnvironmentIdentifierRead::Typeof,
                    &value,
                    function,
                )?;
                self.emit_typeof_value(&value, output, function)?;
                value.clear(function);
            }
            ExprIr::NewTarget => self.compile_new_target_to_locals(output, function)?,
            ExprIr::LogicalNot { expr } => {
                let result = schema.reserve_i32_local(function);
                self.compile_truthy_i32(expr, function)?;
                function.instruction(&Instruction::I32Eqz);
                result.store(function);
                output.set_boolean(result, function);
                schema.release_i32_local(result, function);
            }
            ExprIr::SpecOperation {
                operation,
                operands,
            } => self.compile_spec_operation_to_locals(*operation, operands, output, function)?,
            ExprIr::BinaryNumber { op, lhs, rhs } => {
                if expr.kind == ValueKind::BigInt {
                    self.compile_bigint_arithmetic_to_locals(
                        BigIntHelperOp::from_arithmetic(*op),
                        lhs,
                        rhs,
                        output,
                        function,
                    )?;
                } else {
                    self.compile_binary_number_to_value(*op, lhs, rhs, output, function)?;
                }
            }
            ExprIr::CoerciveBinaryNumber { op, lhs, rhs } => {
                self.compile_coercive_binary_number_to_locals(*op, lhs, rhs, output, function)?
            }
            ExprIr::BitwiseNumeric { op, lhs, rhs } => {
                self.compile_bitwise_numeric_to_locals(*op, lhs, rhs, output, function)?
            }
            ExprIr::StringFromCharCode { code } => {
                self.compile_string_from_char_code_to_value(code, output, function)?
            }
            ExprIr::CoerciveAdd { lhs, rhs } => {
                self.compile_coercive_add_to_locals(lhs, rhs, output, function)?
            }
            ExprIr::StringConcat { lhs, rhs } => {
                self.compile_string_concat_payload(lhs, rhs, output, function)?
            }
            ExprIr::CompareNumber { op, lhs, rhs } => {
                let result = schema.reserve_i32_local(function);
                self.compile_number_comparison_i32(*op, lhs, rhs, function)?;
                result.store(function);
                output.set_boolean(result, function);
                schema.release_i32_local(result, function);
            }
            ExprIr::CompareValue { op, lhs, rhs } => {
                let result = schema.reserve_i32_local(function);
                self.compile_compare_value_i32(*op, lhs, rhs, function)?;
                result.store(function);
                output.set_boolean(result, function);
                schema.release_i32_local(result, function);
            }
            ExprIr::StrictEquality { op, lhs, rhs } => {
                let result = schema.reserve_i32_local(function);
                self.compile_strict_equality_i32(lhs, rhs, function)?;
                if *op == EqualityBinaryOp::StrictNotEqual {
                    function.instruction(&Instruction::I32Eqz);
                }
                result.store(function);
                output.set_boolean(result, function);
                schema.release_i32_local(result, function);
            }
            ExprIr::LooseEquality { op, lhs, rhs } => {
                let result = schema.reserve_i32_local(function);
                self.compile_loose_equality_i32(lhs, rhs, function)?;
                if *op == EqualityBinaryOp::LooseNotEqual {
                    function.instruction(&Instruction::I32Eqz);
                }
                result.store(function);
                output.set_boolean(result, function);
                schema.release_i32_local(result, function);
            }
            ExprIr::LogicalShortCircuit { op, lhs, rhs } => {
                let left = schema.reserve_value_local(function);
                self.compile_expr_to_value(lhs, &left, function)?;
                match op {
                    LogicalBinaryOp::And | LogicalBinaryOp::Or => {
                        self.compile_truthy_tagged_i32(&left, function)?;
                        if *op == LogicalBinaryOp::Or {
                            function.instruction(&Instruction::I32Eqz);
                        }
                    }
                    LogicalBinaryOp::Coalesce => {
                        self.compile_nullish_tagged_i32(left.tag(), function)?
                    }
                }
                self.open_frame(ControlFrameKind::If, function);
                self.compile_expr_to_value(rhs, output, function)?;
                function.instruction(&Instruction::Else);
                output.copy_from(&left, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                left.clear(function);
            }
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                self.compile_truthy_i32(condition, function)?;
                self.open_frame(ControlFrameKind::If, function);
                self.compile_expr_to_value(then_expr, output, function)?;
                function.instruction(&Instruction::Else);
                self.compile_expr_to_value(else_expr, output, function)?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            ExprIr::Comma { lhs, rhs } => {
                let value = schema.reserve_value_local(function);
                self.compile_expr_to_value(lhs, &value, function)?;
                value.clear(function);
                self.compile_expr_to_value(rhs, output, function)?;
            }
            ExprIr::MaterializeBinding { name, value, body } => {
                self.compile_materialized_binding_to_value(name, value, body, output, function)?
            }
            ExprIr::ArrayDestructure {
                value,
                pattern,
                evaluation,
            } => self.compile_array_destructure_to_locals(
                value,
                pattern,
                *evaluation,
                output,
                function,
            )?,
            ExprIr::ObjectDestructure { value, pattern } => {
                self.compile_object_destructure_to_locals(value, pattern, output, function)?
            }
            ExprIr::ObjectDestructuringOperation(operation) => {
                self.compile_object_destructuring_operation_to_value(operation, output, function)?
            }
            ExprIr::CallNamed { name, args } => self.emit_call(
                name,
                args,
                &crate::functions::CallContinuation::Continue,
                output,
                function,
            )?,
            ExprIr::CaptureOptionalCallReference(capture) => {
                let ExprIr::OptionalPropertyChain { target, chain } =
                    &capture.chain_expression().expr
                else {
                    unreachable!("private optional Reference constructor owns the chain")
                };
                self.compile_optional_property_chain_to_value(
                    target,
                    chain,
                    Some(capture.receiver()),
                    &crate::functions::CallContinuation::Continue,
                    output,
                    function,
                )?;
            }
            ExprIr::CaptureArgumentList(_) => {
                return Err(EmitError::unsupported(
                    "compiler invariant: argument-list capture requires its paired lexical publication",
                ));
            }
            ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_) => {
                return Err(EmitError::unsupported("spread argument outside call"));
            }
            ExprIr::AssertSameValue {
                actual, expected, ..
            } => {
                self.emit_assert_same_value(
                    actual,
                    expected,
                    self.strings.source_runtime_error_message(
                        SourceRuntimeErrorMessage::Expression(expr),
                    )?,
                    function,
                )?;
                output.set_undefined(function);
            }
            ExprIr::RuntimeThrow { name, .. } => {
                self.emit_expression_native_error(
                    *name,
                    self.strings.source_runtime_error_message(
                        SourceRuntimeErrorMessage::Expression(expr),
                    )?,
                    function,
                )?;
            }
            ExprIr::CallIndirect {
                direct_eval,
                callee,
                this_arg,
                args,
                static_regexp_compilation,
            } => self.emit_indirect_call(
                callee,
                this_arg.as_deref(),
                args,
                static_regexp_compilation.as_ref(),
                direct_eval.as_ref(),
                &crate::functions::CallContinuation::Continue,
                output,
                function,
            )?,
            ExprIr::Construct {
                callee,
                args,
                static_regexp_compilation,
            } => self.emit_construct(
                callee,
                args,
                static_regexp_compilation.as_ref(),
                output,
                function,
            )?,
            ExprIr::ClassDefinition(class) => {
                self.compile_class_definition_payload(class, output, function)?
            }
            ExprIr::CallMethod {
                receiver,
                key,
                args,
            } => self.emit_method_call(
                receiver,
                key,
                args,
                &crate::functions::CallContinuation::Continue,
                output,
                function,
            )?,
            ExprIr::InstanceOf { lhs, rhs } | ExprIr::In { lhs, rhs } => {
                let result = schema.reserve_i32_local(function);
                if matches!(&expr.expr, ExprIr::InstanceOf { .. }) {
                    self.emit_instanceof_i32(lhs, rhs, function)?;
                } else {
                    self.emit_in_i32(lhs, rhs, function)?;
                }
                result.store(function);
                output.set_boolean(result, function);
                schema.release_i32_local(result, function);
            }
            ExprIr::PropertyRead { target, key } => {
                self.compile_property_read_to_locals(target, key, output, function)?
            }
            ExprIr::OptionalPropertyChain { target, chain } => self
                .compile_optional_property_chain_to_value(
                    target,
                    chain,
                    None,
                    &crate::functions::CallContinuation::Continue,
                    output,
                    function,
                )?,
            ExprIr::SuperConstruct { args } => {
                let constructor = schema.reserve_value_local(function);
                let new_target = schema.reserve_value_local(function);
                self.emit_prepare_super_construct_to_locals(&new_target, &constructor, function)?;
                let arguments = self.emit_call_args_vector(args, function)?;
                self.emit_super_construct_with_prepared_arg_vector(
                    &constructor,
                    &new_target,
                    &arguments,
                    output,
                    function,
                )?;
                arguments.clear(function);
                new_target.clear(function);
                constructor.clear(function);
            }
            ExprIr::SuperNewTarget => {
                self.emit_get_derived_new_target_to_locals(output, function)?;
            }
            ExprIr::SuperConstructor => {
                self.emit_get_super_constructor_to_locals(output, function)?;
            }
            ExprIr::PreparedSuperConstruct(prepared) => {
                let constructor = schema.reserve_value_local(function);
                let new_target = schema.reserve_value_local(function);
                self.compile_expr_to_value(prepared.constructor(), &constructor, function)?;
                self.compile_expr_to_value(prepared.new_target(), &new_target, function)?;
                let arguments = self.emit_call_args_vector(prepared.arguments(), function)?;
                self.emit_super_construct_with_prepared_arg_vector(
                    &constructor,
                    &new_target,
                    &arguments,
                    output,
                    function,
                )?;
                arguments.clear(function);
                new_target.clear(function);
                constructor.clear(function);
            }
            ExprIr::SuperPropertyRead { key, receiver } => {
                self.compile_super_property_read_to_value(key, receiver, output, function)?
            }
            ExprIr::SuperPropertyWrite {
                key,
                receiver,
                value,
                strictness,
            } => self.compile_super_property_write_to_value(
                key,
                receiver,
                value,
                *strictness,
                output,
                function,
            )?,
            ExprIr::SuperPropertyMutation(mutation) => {
                self.compile_super_property_mutation_to_value(mutation, output, function)?
            }
            ExprIr::PrivateRead {
                target,
                private_name_id,
            } => self.compile_private_read_to_locals(target, *private_name_id, output, function)?,
            ExprIr::PrivateWrite {
                target,
                private_name_id,
                value,
            } => self.compile_private_write_to_locals(
                target,
                *private_name_id,
                value,
                output,
                function,
            )?,
            ExprIr::PrivateIn {
                private_name_id,
                rhs,
            } => {
                let value = schema.reserve_value_local(function);
                let result = schema.reserve_i32_local(function);
                self.compile_expr_to_value(rhs, &value, function)?;
                self.emit_is_heap_object_like_tag_i32(value.tag(), function);
                self.open_frame(ControlFrameKind::If, function);
                let token = self.emit_private_name_token_to_local(*private_name_id, function)?;
                self.emit_private_brand_has_i32(&value, &token, result, function);
                token.clear(function);
                function.instruction(&Instruction::Else);
                self.emit_expression_native_error(
                    NativeErrorKind::TypeError,
                    RuntimeErrorMessage::RIGHT_HAND_SIDE_OF_PRIVATE_IN_IS_NOT_AN_OBJECT,
                    function,
                )?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                output.set_boolean(result, function);
                schema.release_i32_local(result, function);
                value.clear(function);
            }
        }
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    fn compile_identifier_to_value(
        &mut self,
        name: &str,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if name == LEXICAL_THIS_NAME {
            return self.compile_this_to_locals(output, function);
        }
        // An Identifier carries an already resolved storage name. A source
        // parameter/capture named like a public global or host function wins;
        // compiler-owned global objects have their own expression variant.
        if let Some(storage) = self.lookup_binding(name) {
            return self.read_binding_to_locals(storage, output, function);
        }
        if self.should_read_script_global_property(name) {
            return self.emit_global_property_read(name, output, function);
        }
        if self.is_script_global_binding(name) && self.lookup_binding(name).is_none() {
            return self.emit_global_property_read(name, output, function);
        }
        if let Some(host) = self.compiled_host_builtin_by_name(name) {
            let schema = self.runtime_schema();
            let meta = self
                .functions
                .get(&host.function_id())
                .cloned()
                .ok_or_else(|| {
                    EmitError::unsupported(format!("unknown host function `{}`", host.as_str()))
                })?;
            let callable = schema
                .reserve_gc_local::<crate::gc_types::FunctionObject, NonNullable>(function)
                .initialize(self.emit_function_value_payload(&meta, function)?, function);
            output.set_reference(&callable, schema, function);
            callable.clear(function);
            return Ok(());
        }
        Err(EmitError::unsupported(format!(
            "unbound identifier `{name}`"
        )))
    }

    fn compile_retained_compound_result(
        &mut self,
        op: ArithmeticBinaryOp,
        old: &ValueLocals,
        rhs: &TypedExpr,
        result: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.push_scope();
        let (lhs, id) = self.retain_expression_operand("\0compound.old", old, function);
        let compiled = if op == ArithmeticBinaryOp::Add {
            self.compile_coercive_add_to_locals(&lhs, rhs, result, function)
        } else {
            self.compile_coercive_binary_number_to_locals(op, &lhs, rhs, result, function)
        };
        self.pop_scope();
        self.release_local_binding(id, function);
        compiled
    }

    fn compile_bigint_literal_to_value(
        &self,
        literal: &scriptw_ir::BigIntLiteralIr,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let (sign, limbs) = literal.signed_magnitude_limbs();
        let length = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(
            i32::try_from(limbs.len()).expect("validated BigInt literal array size"),
        ));
        length.store(function);
        let construction = crate::gc_types::BigIntConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        let index = schema.reserve_i32_local(function);
        let word = schema.reserve_i64_local(function);
        for (position, limb) in limbs.iter().enumerate() {
            function.instruction(&Instruction::I32Const(position as i32));
            index.store(function);
            function.instruction(&Instruction::I64Const(*limb as i64));
            word.store(function);
            construction.write(index, word, schema, function);
        }
        let negative = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(i32::from(sign < 0)));
        negative.store(function);
        let value = schema
            .reserve_gc_local::<crate::gc_types::BigIntValue, NonNullable>(function)
            .initialize(construction.publish(negative, schema, function), function);
        output.set_reference(&value, schema, function);
        value.clear(function);
        schema.release_i32_local(negative, function);
        schema.release_i64_local(word, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
    }

    fn compile_string_from_char_code_to_value(
        &mut self,
        code: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let input = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        self.compile_expr_to_value(code, &input, function)?;
        self.emit_value_to_number_payload(&input, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        let unit = schema.reserve_i64_local(function);
        self.emit_to_uint16_i64_from_number_payload(pending.value().scalar(), unit, function);
        let length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let code_unit = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(1));
        length.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        unit.load(function);
        function.instruction(&Instruction::I32WrapI64);
        code_unit.store(function);
        let construction = crate::gc_types::StringConstruction::allocate(
            schema,
            schema.reserve_gc_local::<crate::gc_types::CodeUnitArray, NonNullable>(function),
            length,
            function,
        );
        construction.write(index, code_unit, schema, function);
        let text = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(construction.publish(schema, function), function);
        output.set_reference(&text, schema, function);
        text.clear(function);
        schema.release_i32_local(code_unit, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
        schema.release_i64_local(unit, function);
        pending.clear(function);
        input.clear(function);
        Ok(())
    }

    pub(crate) fn compile_expr_to_binding(
        &mut self,
        expr: &TypedExpr,
        storage: BindingStorage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let value = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(expr, &value, function)?;
        self.write_binding_from_locals(storage, &value, function);
        value.clear(function);
        Ok(())
    }

    fn compile_materialized_binding_to_value(
        &mut self,
        name: &str,
        value: &TypedExpr,
        body: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let initialized = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(value, &initialized, function)?;
        self.push_scope();
        let (_, id) = self.retain_expression_operand(name, &initialized, function);
        let result = self.compile_expr_to_value(body, output, function);
        self.pop_scope();
        self.release_local_binding(id, function);
        initialized.clear(function);
        result
    }
}
