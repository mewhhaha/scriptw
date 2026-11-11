use super::generator_loop::ResumableGeneratorLoop;
use super::generator_switch::ResumableGeneratorSwitch;
use super::*;

#[derive(Clone)]
enum GeneratorStatementListPurpose {
    ClassicLoop,
    Switch {
        discriminant: OwnedEnvBindingIr,
    },
    Iteration {
        head: OwnedEnvBindingIr,
        key: OwnedEnvBindingIr,
        enumerator: OwnedEnvBindingIr,
    },
    Iterator {
        head: OwnedEnvBindingIr,
        incoming: OwnedEnvBindingIr,
    },
}

/// This compile-time scope is constructed only from checked source owners.
/// Its values occupy the original invocation's actual owned BindingCells.
#[derive(Clone)]
pub(crate) struct GeneratorStatementListValueContext {
    value: OwnedEnvBindingIr,
    purpose: GeneratorStatementListPurpose,
    break_target: ControlTarget,
    active: bool,
    enclosing: Option<Box<Self>>,
}

#[must_use = "a saved StatementList value must be restored after normal evaluation"]
pub(crate) struct SavedStatementListValue {
    value: ValueLocals,
}

impl FunctionBuilder<'_> {
    pub(super) fn begin_resumable_classic_loop_value(
        &mut self,
        plan: ResumableGeneratorLoop<'_>,
        break_target: ControlTarget,
    ) -> Result<(), EmitError> {
        if !self
            .current_function_meta()
            .is_some_and(|meta| meta.protocol().execution_kind() == plan.execution_kind())
            || self
                .body_entry_locals()
                .and_then(|entry| entry.resume_frame())
                .is_none()
            || !self
                .owned_env_bindings
                .iter()
                .any(|owned| owned == plan.value_binding())
        {
            return Err(EmitError::unsupported(
                "compiler invariant: classic-loop completion requires its exact owned activation cell",
            ));
        }
        let enclosing = self.statement_list_value_context.take().map(Box::new);
        self.statement_list_value_context = Some(GeneratorStatementListValueContext {
            value: plan.value_binding().clone(),
            purpose: GeneratorStatementListPurpose::ClassicLoop,
            break_target,
            active: false,
            enclosing,
        });
        Ok(())
    }

    pub(super) fn compile_resumable_operand_region(
        &mut self,
        block: &BlockIr,
        entry_state: u32,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_resumable_operand_region_then(block, entry_state, function, |_, _| Ok(()))
    }

    /// Finish consumes the staged value before its exact temporary scope is
    /// retired. Keep inactive contexts present for whole abrupt root cleanup.
    pub(super) fn compile_resumable_operand_region_then(
        &mut self,
        block: &BlockIr,
        entry_state: u32,
        function: &mut Function,
        finish: impl FnOnce(&mut Self, &mut Function) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let previous_scopes = self.binding_scopes.clone();
        let previous_context = self.statement_list_value_context.clone();
        let previous_environment_depth = self.environment_depth;
        if let Some(context) = &mut self.statement_list_value_context {
            context.active = false;
        }
        self.push_scope();
        let result = self
            .compile_resumable_block_contents(block, entry_state, true, function)
            .and_then(|()| finish(self, function));
        self.binding_scopes = previous_scopes;
        self.statement_list_value_context = previous_context;
        self.environment_depth = previous_environment_depth;
        result
    }

    /// A For initializer or mixed If selector publishes bindings used by later
    /// phases in the enclosing scope. Keep their compiler aliases there while
    /// the checked invocation cells retain their values across suspension.
    pub(super) fn compile_resumable_operand_region_in_current_scope(
        &mut self,
        block: &BlockIr,
        entry_state: u32,
        function: &mut Function,
        finish: impl FnOnce(&mut Self, &mut Function) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        if block.lexical_environment.is_some() {
            return Err(EmitError::unsupported(
                "compiler invariant: shared operand bindings require the enclosing Environment",
            ));
        }
        let previous_context = self.statement_list_value_context.clone();
        let previous_environment_depth = self.environment_depth;
        if let Some(context) = &mut self.statement_list_value_context {
            context.active = false;
        }
        let result = self
            .compile_resumable_block_contents(block, entry_state, true, function)
            .and_then(|()| finish(self, function));
        self.statement_list_value_context = previous_context;
        self.environment_depth = previous_environment_depth;
        result
    }

    pub(super) fn begin_resumable_switch_statement_list_value(
        &mut self,
        plan: ResumableGeneratorSwitch<'_>,
        break_target: ControlTarget,
    ) -> Result<(), EmitError> {
        let value = plan.value_binding();
        let discriminant = plan.discriminant_binding();
        if !self
            .current_function_meta()
            .is_some_and(|meta| meta.protocol().execution_kind() == plan.execution_kind())
            || self
                .body_entry_locals()
                .and_then(|entry| entry.resume_frame())
                .is_none()
            || [value, discriminant]
                .into_iter()
                .any(|binding| !self.owned_env_bindings.iter().any(|owned| owned == binding))
        {
            return Err(EmitError::unsupported(
                "compiler invariant: Switch completion requires its exact owned resumable cells",
            ));
        }
        let enclosing = self.statement_list_value_context.take().map(Box::new);
        self.statement_list_value_context = Some(GeneratorStatementListValueContext {
            value: value.clone(),
            purpose: GeneratorStatementListPurpose::Switch {
                discriminant: discriminant.clone(),
            },
            break_target,
            active: false,
            enclosing,
        });
        Ok(())
    }

    pub(super) fn begin_resumable_iteration_value(
        &mut self,
        plan: &scriptw_ir::AsyncGeneratorForInIr,
        break_target: ControlTarget,
    ) -> Result<(), EmitError> {
        let bindings = [
            plan.value_binding(),
            plan.head_binding(),
            plan.key_binding(),
            plan.enumerator_binding(),
        ];
        if !self.current_function_meta().is_some_and(|meta| {
            meta.protocol().execution_kind() == Self::for_in_execution_kind(plan)
        }) || self
            .body_entry_locals()
            .and_then(|entry| entry.resume_frame())
            .is_none()
            || bindings
                .into_iter()
                .any(|binding| !self.owned_env_bindings.iter().any(|owned| owned == binding))
        {
            return Err(EmitError::unsupported(
                "compiler invariant: ForIn completion requires its exact owned resumable cells",
            ));
        }
        // The enumerator writes this cell directly. Source initialization still
        // needs an Identifier alias, adjusted by each nested Environment scope.
        self.bind_owned_environment_alias(plan.key_binding());
        let enclosing = self.statement_list_value_context.take().map(Box::new);
        self.statement_list_value_context = Some(GeneratorStatementListValueContext {
            value: plan.value_binding().clone(),
            purpose: GeneratorStatementListPurpose::Iteration {
                head: plan.head_binding().clone(),
                key: plan.key_binding().clone(),
                enumerator: plan.enumerator_binding().clone(),
            },
            break_target,
            active: false,
            enclosing,
        });
        Ok(())
    }

    pub(super) fn end_generator_statement_list_value(&mut self) {
        let current = self
            .statement_list_value_context
            .take()
            .expect("an emitted source owner owns its completion context");
        self.statement_list_value_context = current.enclosing.map(|context| *context);
    }

    pub(super) fn begin_async_generator_iterator_value(
        &mut self,
        plan: &scriptw_ir::AsyncGeneratorForOfIr,
        break_target: ControlTarget,
    ) -> Result<(), EmitError> {
        let execution = match plan.execution() {
            scriptw_ir::ResumableRegionProtocolIr::Generator => FunctionExecutionKind::Generator,
            scriptw_ir::ResumableRegionProtocolIr::Async => FunctionExecutionKind::Async,
            scriptw_ir::ResumableRegionProtocolIr::AsyncGenerator => {
                FunctionExecutionKind::AsyncGenerator
            }
        };
        if !self
            .current_function_meta()
            .is_some_and(|meta| meta.protocol().execution_kind() == execution)
            || self
                .body_entry_locals()
                .and_then(|entry| entry.resume_frame())
                .is_none()
            || [
                plan.head_binding(),
                plan.incoming_binding(),
                plan.value_binding(),
            ]
            .into_iter()
            .any(|binding| !self.owned_env_bindings.iter().any(|owned| owned == binding))
        {
            return Err(EmitError::unsupported(
                "compiler invariant: complete iterator completion requires its exact source activation cells",
            ));
        }
        // IteratorValue writes the invocation cell without a source declaration.
        // Publish its alias before attaching the per-iteration Environment.
        self.bind_owned_environment_alias(plan.incoming_binding());
        let enclosing = self.statement_list_value_context.take().map(Box::new);
        self.statement_list_value_context = Some(GeneratorStatementListValueContext {
            value: plan.value_binding().clone(),
            purpose: GeneratorStatementListPurpose::Iterator {
                head: plan.head_binding().clone(),
                incoming: plan.incoming_binding().clone(),
            },
            break_target,
            active: false,
            enclosing,
        });
        Ok(())
    }

    pub(super) fn activate_generator_statement_list_value(&mut self) {
        self.statement_list_value_context
            .as_mut()
            .expect("a source body owns its completion context")
            .active = true;
    }

    pub(super) fn deactivate_generator_statement_list_value(&mut self) {
        self.statement_list_value_context
            .as_mut()
            .expect("a source owner owns its completion context")
            .active = false;
    }

    pub(crate) fn has_generator_statement_list_value(&self) -> bool {
        self.statement_list_value_context
            .as_ref()
            .is_some_and(|context| context.active)
    }

    // Resolve from the stable invocation record, never a cached lexical hop.
    // This is the same concrete Environment/BindingCell storage used by source
    // bindings and suspended Identifier References.
    fn generator_statement_list_cell(
        &self,
        binding: &OwnedEnvBindingIr,
        function: &mut Function,
    ) -> GcLocal<BindingCell> {
        let schema = self.runtime_schema();
        let frame = self
            .body_entry_locals()
            .and_then(|entry| entry.resume_frame())
            .expect("a checked source owner owns its invocation frame");
        let environment = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<InvocationFrame>()
                    .field(InvocationFrameSchema::INVOCATION_ENVIRONMENT)
                    .read(frame, schema, function)
                    .reference(),
                function,
            );
        let declarative = schema
            .reserve_gc_local::<DeclarativeEnvironment, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::DECLARATIVE)
                    .read(&environment, schema, function)
                    .reference(),
                function,
            );
        let cells = schema
            .reserve_gc_local::<BindingCellTable, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<DeclarativeEnvironment>()
                    .field(DeclarativeEnvironmentSchema::CELLS)
                    .read(&declarative, schema, function)
                    .reference(),
                function,
            );
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(binding.slot as i32));
        index.store(function);
        let cell = schema
            .reserve_gc_local::<BindingCell, NonNullable>(function)
            .initialize(
                schema
                    .array_type::<BindingCellTable>()
                    .read(&cells, index, schema, function)
                    .reference(),
                function,
            );
        schema.release_i32_local(index, function);
        cells.clear(function);
        declarative.clear(function);
        environment.clear(function);
        cell
    }

    pub(super) fn write_generator_statement_list_binding(
        &self,
        binding: &OwnedEnvBindingIr,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let cell = self.generator_statement_list_cell(binding, function);
        schema
            .struct_type::<BindingCell>()
            .write_value(&cell, value, schema, function);
        schema.field(BindingCellSchema::INITIALIZED).write(
            &cell,
            GcOperand::boolean(true),
            schema,
            function,
        );
        cell.clear(function);
    }

    pub(super) fn read_generator_statement_list_binding(
        &self,
        binding: &OwnedEnvBindingIr,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let cell = self.generator_statement_list_cell(binding, function);
        schema
            .struct_type::<BindingCell>()
            .read_value(&cell, value, schema, function);
        cell.clear(function);
    }

    pub(crate) fn emit_checkpoint_generator_statement_list_value(&self, function: &mut Function) {
        let Some(context) = self
            .statement_list_value_context
            .as_ref()
            .filter(|context| context.active)
        else {
            return;
        };
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.write_generator_statement_list_binding(
            &context.value,
            self.completion().value(),
            function,
        );
        function.instruction(&Instruction::End);
    }

    pub(super) fn emit_restore_generator_statement_list_value(&self, function: &mut Function) {
        let Some(context) = self
            .statement_list_value_context
            .as_ref()
            .filter(|context| context.active)
        else {
            return;
        };
        let value = self.runtime_schema().reserve_value_local(function);
        self.read_generator_statement_list_binding(&context.value, &value, function);
        self.completion().set_normal(&value, function);
        value.clear(function);
    }

    pub(super) fn emit_generator_statement_list_entry(
        &mut self,
        entry_state: u32,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if self.has_generator_statement_list_value() {
            self.emit_resumable_state_equals(entry_state, function)?;
            self.open_frame(ControlFrameKind::If, function);
            self.emit_checkpoint_generator_statement_list_value(function);
            function.instruction(&Instruction::Else);
            self.emit_restore_generator_statement_list_value(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        Ok(())
    }

    pub(super) fn compile_empty_statement_completion(
        &mut self,
        statement: &StatementIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !self.has_generator_statement_list_value() {
            return self.compile_statement(statement, function);
        }
        // This boundary comes from the checked original source item. Internal
        // captures, branches and yielded initializer prefixes are not items.
        self.emit_checkpoint_generator_statement_list_value(function);
        self.statement_list_value_context.as_mut().unwrap().active = false;
        let result = self.compile_statement(statement, function);
        self.statement_list_value_context.as_mut().unwrap().active = true;
        result?;
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_restore_generator_statement_list_value(function);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(super) fn emit_retire_generator_statement_list_values(
        &self,
        target: Option<ControlTarget>,
        function: &mut Function,
    ) {
        if self.statement_list_value_context.is_none() {
            return;
        }
        let mut current = self.statement_list_value_context.as_ref();
        let undefined = self.runtime_schema().reserve_value_local(function);
        undefined.set_undefined(function);
        while let Some(context) = current {
            if target.is_none_or(|target| target.frame <= context.break_target.frame) {
                self.write_generator_statement_list_binding(&context.value, &undefined, function);
                match &context.purpose {
                    GeneratorStatementListPurpose::ClassicLoop => {}
                    GeneratorStatementListPurpose::Switch { discriminant } => {
                        self.write_generator_statement_list_binding(
                            discriminant,
                            &undefined,
                            function,
                        );
                    }
                    GeneratorStatementListPurpose::Iteration {
                        head,
                        key,
                        enumerator,
                    } => {
                        self.write_generator_statement_list_binding(head, &undefined, function);
                        self.write_generator_statement_list_binding(key, &undefined, function);
                        let schema = self.runtime_schema();
                        let cell = self.generator_statement_list_cell(enumerator, function);
                        schema
                            .struct_type::<BindingCell>()
                            .field(BindingCellSchema::FOR_IN_ENUMERATION_RECORD)
                            .write(&cell, GcOperand::null(schema), schema, function);
                        cell.clear(function);
                    }
                    GeneratorStatementListPurpose::Iterator { head, incoming } => {
                        self.write_generator_statement_list_binding(head, &undefined, function);
                        self.write_generator_statement_list_binding(incoming, &undefined, function);
                    }
                }
            }
            current = context.enclosing.as_deref();
        }
        undefined.clear(function);
    }

    pub(super) fn generator_statement_list_branch_retires(&self, target: ControlTarget) -> bool {
        let mut current = self.statement_list_value_context.as_ref();
        while let Some(context) = current {
            if target.frame <= context.break_target.frame {
                return true;
            }
            current = context.enclosing.as_deref();
        }
        false
    }

    pub(crate) fn emit_statement_result(&self, function: &mut Function) {
        self.completion().value().set_undefined(function);
        self.set_completion_kind(CompletionKind::Normal, function);
    }

    pub(crate) fn save_current_completion(
        &self,
        saved: &CompletionLocals,
        function: &mut Function,
    ) {
        saved.copy_from(self.completion(), function);
    }

    // UpdateEmpty retains the preceding StatementList value separately from
    // operand temporaries and restores it only for the normal empty result.
    pub(crate) fn save_statement_list_value(
        &self,
        function: &mut Function,
    ) -> SavedStatementListValue {
        let value = self.runtime_schema().reserve_value_local(function);
        value.copy_from(self.completion().value(), function);
        SavedStatementListValue { value }
    }

    pub(crate) fn restore_statement_list_value(
        &mut self,
        saved: SavedStatementListValue,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_propagate_current_throw_if_needed(function);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.completion().value().copy_from(&saved.value, function);
        function.instruction(&Instruction::End);
        saved.value.clear(function);
        Ok(())
    }

    pub(super) fn compile_iteration_condition(
        &mut self,
        condition: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let saved = self.save_statement_list_value(function);
        self.compile_truthy_i32(condition, function)?;
        self.restore_statement_list_value(saved, function)
    }
}
