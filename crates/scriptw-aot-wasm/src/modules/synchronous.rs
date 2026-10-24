//! Private module activations publish canonical environments before import links.

use super::*;
use scriptw_ir::{
    DeferredModuleEvaluationIr, ModuleActivationKindIr, ModuleCellIr, ModuleEvaluationIr,
    ModuleExecutionGraphIr, ModuleImportBindingIr, ModuleNamespaceModeIr, ModuleRequestPhaseIr,
};

impl FunctionBuilder<'_> {
    pub(crate) fn data_module_realm(
        &mut self,
        module: u32,
        function: &mut Function,
    ) -> Result<GcLocal<RealmRecord>, EmitError> {
        if !self
            .function_id
            .as_ref()
            .and_then(|id| self.functions.get(id))
            .is_some_and(|owner| owner.protocol() == FunctionProtocolIr::ModuleActivation)
        {
            return Err(EmitError::unsupported(
                "Synthetic data evaluation requires its private module activation",
            ));
        }
        let schema = self.runtime_schema();
        let record = self.module_record_local(module, function)?;
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::REALM)
                .read(&record, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        record.clear(function);
        Ok(realm)
    }

    pub(super) fn module_record_local(
        &mut self,
        module: u32,
        function: &mut Function,
    ) -> Result<GcLocal<ModuleRecord>, EmitError> {
        let realm = self.load_current_realm(function);
        let record = self.module_record_in_realm(module, &realm, function)?;
        realm.clear(function);
        Ok(record)
    }

    pub(crate) fn module_record_in_realm(
        &mut self,
        module: u32,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<GcLocal<ModuleRecord>, EmitError> {
        if module >= self.functions.module_execution_record_count() {
            return Err(EmitError::unsupported(
                "private module operation requires its validated execution graph",
            ));
        }
        let schema = self.runtime_schema();
        let graph = self.emit_module_graph_in_realm(realm, self.current_program(), function);
        let records = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleGraph>()
                .field(ModuleGraphSchema::MODULES)
                .read(&graph, schema, function)
                .reference(),
            function,
        );
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(module as i32));
        index.store(function);
        let record = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ModuleRegistry>()
                .read(&records, index, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        schema.release_i32_local(index, function);
        records.clear(function);
        graph.clear(function);
        Ok(record)
    }

    pub(crate) fn module_namespace_cell(
        &self,
        record: &GcLocal<ModuleRecord>,
        mode: ModuleNamespaceModeIr,
        function: &mut Function,
    ) -> GcLocal<BindingCell> {
        let schema = self.runtime_schema();
        let field = match mode {
            ModuleNamespaceModeIr::Eager => ModuleRecordSchema::NAMESPACE_CELL,
            ModuleNamespaceModeIr::Deferred => ModuleRecordSchema::DEFERRED_NAMESPACE_CELL,
        };
        schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleRecord>()
                .field(field)
                .read(record, schema, function)
                .reference(),
            function,
        )
    }

    fn module_cell(
        &mut self,
        target: &ModuleCellIr,
        function: &mut Function,
    ) -> Result<GcLocal<BindingCell>, EmitError> {
        let module = match target {
            ModuleCellIr::Binding { module, .. }
            | ModuleCellIr::Namespace { module, .. }
            | ModuleCellIr::ModuleSource { module } => *module,
        };
        let schema = self.runtime_schema();
        let record = self.module_record_local(module, function)?;
        let cell = match target {
            ModuleCellIr::Binding { slot, .. } => {
                let environment = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<ModuleRecord>()
                        .field(ModuleRecordSchema::ENVIRONMENT)
                        .read(&record, schema, function)
                        .reference()
                        .require_non_null(function),
                    function,
                );
                let cell = self.emit_environment_cell_local(&environment, *slot, function);
                environment.clear(function);
                cell
            }
            ModuleCellIr::Namespace { mode, .. } => {
                self.module_namespace_cell(&record, *mode, function)
            }
            ModuleCellIr::ModuleSource { module } => {
                self.module_source_cell(&record, *module, function)?
            }
        };
        record.clear(function);
        Ok(cell)
    }

    pub(crate) fn emit_module_import_binding(
        &mut self,
        import: &ModuleImportBindingIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let slot = self
            .owned_env_slot(&import.name)
            .ok_or_else(|| EmitError::unsupported("module import must own an environment slot"))?;
        let cell = self.module_cell(&import.target, function)?;
        self.emit_initialize_indirect_binding(slot, &cell, function);
        cell.clear(function);
        Ok(())
    }

    pub(crate) fn emit_module_binding_read(
        &mut self,
        target: &ModuleCellIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let cell = self.module_cell(target, function)?;
        let initialized = self.emit_read_environment_cell(&cell, output, function);
        cell.clear(function);
        initialized.load(function);
        function.instruction(&Instruction::I32Eqz);
        schema.release_i32_local(initialized, function);
        self.open_frame(ControlFrameKind::If, function);
        let pending = schema.reserve_completion(function);
        self.emit_throw_runtime_error(
            NativeErrorKind::ReferenceError,
            RuntimeErrorMessage::MODULE_BINDING_ACCESSED_BEFORE_INITIALIZATION,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_module_namespace_publish(
        &mut self,
        module: u32,
        mode: ModuleNamespaceModeIr,
        namespace: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        self.compile_expr_to_value(namespace, &value, function)?;
        let record = self.module_record_local(module, function)?;
        let cell = self.module_namespace_cell(&record, mode, function);
        self.emit_initialize_environment_cell(&cell, &value, function);
        cell.clear(function);
        record.clear(function);
        value.clear(function);
        Ok(())
    }

    pub(super) fn emit_private_module_resume(
        &mut self,
        record: &GcLocal<ModuleRecord>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let activation = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::GENERATOR)
                .read(record, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        schema
            .struct_type::<GeneratorActivation>()
            .field(GeneratorActivationSchema::STATUS)
            .write(
                &activation,
                GcOperand::constant(GeneratorState::Executing),
                schema,
                function,
            );
        self.emit_saved_generator_body_call(&activation, result, function);
        // The body publishes suspension in its actual activation. Completion
        // targets belong to Break/Continue and cannot carry a resume point.
        let state = schema.reserve_i32_local(function);
        schema
            .struct_type::<GeneratorActivation>()
            .field(GeneratorActivationSchema::STATUS)
            .read(&activation, schema, function)
            .store(state, function);
        state.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            GeneratorState::SuspendedYield,
        )));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema
            .struct_type::<GeneratorActivation>()
            .field(GeneratorActivationSchema::STATUS)
            .write(
                &activation,
                GcOperand::constant(GeneratorState::Completed),
                schema,
                function,
            );
        let frame = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<GeneratorActivation>()
                .field(GeneratorActivationSchema::FRAME)
                .read(&activation, schema, function)
                .reference(),
            function,
        );
        let retired = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<PrivateArgumentListTable>()
                .fixed(std::iter::empty(), function),
            function,
        );
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::PRIVATE_ARGUMENT_LISTS)
            .write(
                &frame,
                GcOperand::reference(&retired, schema),
                schema,
                function,
            );
        retired.clear(function);
        frame.clear(function);
        function.instruction(&Instruction::Else);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Ne);
        schema
            .struct_type::<GeneratorActivation>()
            .field(GeneratorActivationSchema::RESUME_POINT)
            .read(&activation, schema, function)
            .store(state, function);
        state.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i32_local(state, function);
        activation.clear(function);
        Ok(())
    }

    pub(crate) fn emit_module_execution_graph(
        &mut self,
        graph: &ModuleExecutionGraphIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if graph.initializer().is_some() {
            let schema = self.runtime_schema();
            let realm = self.load_current_realm(function);
            let pending = schema.reserve_completion(function);
            self.emit_module_initialize_in_realm(&realm, &pending, function)?;
            self.completion().copy_from(&pending, function);
            output.copy_from(pending.value(), function);
            pending.clear(function);
            realm.clear(function);
            self.emit_propagate_current_throw_if_needed(function);
            return Ok(());
        }
        for activation in graph.activations() {
            let expected = match activation.kind() {
                ModuleActivationKindIr::Synchronous => FunctionProtocolIr::ModuleActivation,
                ModuleActivationKindIr::Async => FunctionProtocolIr::AsyncModuleActivation,
            };
            if !self
                .functions
                .get(activation.function())
                .is_some_and(|owner| owner.protocol() == expected)
            {
                return Err(EmitError::unsupported(
                    "module activation requires its matching private function protocol",
                ));
            }
        }
        let schema = self.runtime_schema();
        let realm = self.load_current_realm(function);
        let existing = self.emit_module_graph_in_realm(&realm, self.current_program(), function);
        existing.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        existing.clear(function);
        let length = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(graph.record_count() as i32));
        length.store(function);
        let records = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ModuleRegistry>()
                .filled(GcOperand::null(schema), length, function),
            function,
        );
        let previous = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<RealmRecord>()
                .field(RealmRecordSchema::MODULES)
                .read(&realm, schema, function)
                .reference(),
            function,
        );
        let program = schema
            .reserve_gc_local(function)
            .initialize(self.current_program().reference(schema, function), function);
        let graph_record = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<ModuleGraph>().construct(
                (
                    GcOperand::reference(&records, schema),
                    GcOperand::i64(1),
                    GcOperand::i64(
                        i64::from(graph.record_count())
                            .checked_mul(i64::from(graph.record_count()))
                            .ok_or_else(|| {
                                EmitError::unsupported("module graph parent budget overflows")
                            })?,
                    ),
                    GcOperand::i64(0),
                    GcOperand::reference(&program, schema),
                    GcOperand::reference(&previous, schema),
                ),
                function,
            ),
            function,
        );
        program.clear(function);
        previous.clear(function);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let error = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&undefined, function),
            function,
        );
        let index = schema.reserve_i32_local(function);
        for module in 0..graph.record_count() {
            let namespace =
                self.emit_allocate_environment_cell(&undefined, false, false, true, function);
            let deferred =
                self.emit_allocate_environment_cell(&undefined, false, false, true, function);
            let source =
                self.emit_allocate_environment_cell(&undefined, false, false, true, function);
            let is_source = graph
                .source_records()
                .iter()
                .any(|source| source.module() == module);
            let record = schema.reserve_gc_local(function).initialize(
                schema.struct_type::<ModuleRecord>().construct(
                    (
                        GcOperand::reference(&graph_record, schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::reference(&namespace, schema),
                        GcOperand::reference(&deferred, schema),
                        GcOperand::reference(&source, schema),
                        GcOperand::constant(ModuleActivationKind::Synchronous),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::constant(if is_source {
                            ModuleEvaluationState::Evaluated
                        } else {
                            ModuleEvaluationState::Linked
                        }),
                        GcOperand::constant(if is_source {
                            ModuleEvaluationCompletion::Normal
                        } else {
                            ModuleEvaluationCompletion::Empty
                        }),
                        GcOperand::constant(if is_source {
                            ModuleBodyState::Completed
                        } else {
                            ModuleBodyState::NotStarted
                        }),
                        GcOperand::reference(&error, schema),
                        GcOperand::i64(0),
                        GcOperand::i64(0),
                        GcOperand::i64(0),
                        GcOperand::i64(0),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                    ),
                    function,
                ),
                function,
            );
            if is_source {
                schema
                    .struct_type::<ModuleRecord>()
                    .field(ModuleRecordSchema::REALM)
                    .write(
                        &record,
                        GcOperand::nullable_reference(&realm, schema),
                        schema,
                        function,
                    );
            }
            function.instruction(&Instruction::I32Const(module as i32));
            index.store(function);
            schema.array_type::<ModuleRegistry>().write(
                &records,
                index,
                GcOperand::nullable_reference(&record, schema),
                schema,
                function,
            );
            record.clear(function);
            source.clear(function);
            deferred.clear(function);
            namespace.clear(function);
        }
        schema
            .struct_type::<RealmRecord>()
            .field(RealmRecordSchema::MODULES)
            .write(
                &realm,
                GcOperand::nullable_reference(&graph_record, schema),
                schema,
                function,
            );
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        self.emit_module_source_namespaces(graph, function)?;
        for activation in graph.activations() {
            let target = self.module_record_local(activation.module(), function)?;
            let meta = self
                .functions
                .get(activation.function())
                .expect("validated module activation")
                .clone();
            let callable = schema
                .reserve_gc_local(function)
                .initialize(self.emit_function_value_payload(&meta, function)?, function);
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::FUNCTION)
                .write(
                    &target,
                    GcOperand::nullable_reference(&callable, schema),
                    schema,
                    function,
                );
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::REALM)
                .write(
                    &target,
                    GcOperand::nullable_reference(&realm, schema),
                    schema,
                    function,
                );
            let arguments = self.emit_pre_evaluated_arg_vector(&[], function);
            let frame = self.emit_alloc_invocation_frame(
                &callable, &undefined, &undefined, &arguments, function,
            );
            let initial = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&undefined, function),
                function,
            );
            match activation.kind() {
                ModuleActivationKindIr::Synchronous => {
                    let state = schema.reserve_gc_local(function).initialize(
                        schema.struct_type::<GeneratorActivation>().construct(
                            (
                                GcOperand::reference(&frame, schema),
                                GcOperand::reference(&initial, schema),
                                GcOperand::constant(GeneratorResumeKind::Normal),
                                GcOperand::null(schema),
                                GcOperand::i32(crate::gc_types::INITIALIZING_RESUME_POINT),
                                GcOperand::constant(GeneratorState::SuspendedStart),
                            ),
                            function,
                        ),
                        function,
                    );
                    self.emit_saved_generator_body_call(&state, &pending, function);
                    self.completion().copy_from(&pending, function);
                    self.emit_propagate_current_throw_if_needed(function);
                    schema
                        .struct_type::<GeneratorActivation>()
                        .field(GeneratorActivationSchema::RESUME_POINT)
                        .write(&state, GcOperand::i32(0), schema, function);
                    schema
                        .struct_type::<ModuleRecord>()
                        .field(ModuleRecordSchema::GENERATOR)
                        .write(
                            &target,
                            GcOperand::nullable_reference(&state, schema),
                            schema,
                            function,
                        );
                    state.clear(function);
                }
                ModuleActivationKindIr::Async => {
                    let promise = self.emit_alloc_promise_in_realm(&realm, function)?;
                    let state = schema.reserve_gc_local(function).initialize(
                        schema.struct_type::<AsyncActivation>().construct(
                            (
                                GcOperand::reference(&frame, schema),
                                GcOperand::reference(&initial, schema),
                                GcOperand::constant(AwaitCompletionKind::Normal),
                                GcOperand::reference(&promise, schema),
                                GcOperand::i32(0),
                                GcOperand::boolean(false),
                                GcOperand::reference(&realm, schema),
                                GcOperand::constant(AsyncModuleEntryMode::Allocate),
                            ),
                            function,
                        ),
                        function,
                    );
                    self.emit_saved_async_body_call(&state, &pending, function);
                    self.completion().copy_from(&pending, function);
                    self.emit_propagate_current_throw_if_needed(function);
                    schema
                        .struct_type::<ModuleRecord>()
                        .field(ModuleRecordSchema::ACTIVATION_KIND)
                        .write(
                            &target,
                            GcOperand::constant(ModuleActivationKind::Async),
                            schema,
                            function,
                        );
                    schema
                        .struct_type::<ModuleRecord>()
                        .field(ModuleRecordSchema::ASYNC_ACTIVATION)
                        .write(
                            &target,
                            GcOperand::nullable_reference(&state, schema),
                            schema,
                            function,
                        );
                    state.clear(function);
                    promise.clear(function);
                }
            }
            let environment = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<InvocationFrame>()
                    .field(InvocationFrameSchema::INVOCATION_ENVIRONMENT)
                    .read(&frame, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::ENVIRONMENT)
                .write(
                    &target,
                    GcOperand::reference(&environment, schema),
                    schema,
                    function,
                );
            environment.clear(function);
            let mut requests = Vec::with_capacity(activation.requests().len());
            for request in activation.requests() {
                let dependency = self.module_record_local(request.target(), function)?;
                let phase = match request.phase() {
                    ModuleRequestPhaseIr::Evaluation => ModuleRequestPhase::Evaluation,
                    ModuleRequestPhaseIr::Defer => ModuleRequestPhase::Defer,
                };
                requests.push(schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<ModuleRequest>().construct(
                        (
                            GcOperand::constant(phase),
                            GcOperand::reference(&dependency, schema),
                        ),
                        function,
                    ),
                    function,
                ));
                dependency.clear(function);
            }
            let request_table = schema.reserve_gc_local(function).initialize(
                schema.array_type::<ModuleRequestTable>().fixed(
                    requests
                        .iter()
                        .map(|request| GcOperand::reference(request, schema)),
                    function,
                ),
                function,
            );
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::REQUESTS)
                .write(
                    &target,
                    GcOperand::nullable_reference(&request_table, schema),
                    schema,
                    function,
                );
            request_table.clear(function);
            for request in requests.into_iter().rev() {
                request.clear(function);
            }
            initial.clear(function);
            frame.clear(function);
            arguments.clear(function);
            callable.clear(function);
            target.clear(function);
        }
        // All canonical records and environments precede instantiation/import links.
        for activation in graph.activations() {
            let target = self.module_record_local(activation.module(), function)?;
            match activation.kind() {
                ModuleActivationKindIr::Synchronous => {
                    self.emit_private_module_resume(&target, &pending, function)?
                }
                ModuleActivationKindIr::Async => {
                    let state = schema.reserve_gc_local(function).initialize(
                        schema
                            .struct_type::<ModuleRecord>()
                            .field(ModuleRecordSchema::ASYNC_ACTIVATION)
                            .read(&target, schema, function)
                            .reference()
                            .require_non_null(function),
                        function,
                    );
                    schema
                        .struct_type::<AsyncActivation>()
                        .field(AsyncActivationSchema::MODULE_ENTRY_MODE)
                        .write(
                            &state,
                            GcOperand::constant(AsyncModuleEntryMode::Instantiate),
                            schema,
                            function,
                        );
                    self.emit_saved_async_body_call(&state, &pending, function);
                    state.clear(function);
                }
            }
            self.completion().copy_from(&pending, function);
            self.emit_propagate_current_throw_if_needed(function);
            target.clear(function);
        }
        output.set_undefined(function);
        self.completion().initialize(function);
        pending.clear(function);
        schema.release_i32_local(index, function);
        error.clear(function);
        undefined.clear(function);
        graph_record.clear(function);
        records.clear(function);
        schema.release_i32_local(length, function);
        realm.clear(function);
        Ok(())
    }

    pub(crate) fn emit_module_evaluate(
        &mut self,
        plan: &ModuleEvaluationIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if self.emit_module_evaluation_capability_rejection(plan.module(), output, function) {
            return Ok(());
        }
        let schema = self.runtime_schema();
        let record = self.module_record_local(plan.module(), function)?;
        let pending = schema.reserve_completion(function);
        let program = self.emit_module_record_program(&record, function);
        schema
            .call_helper(
                ModuleEvaluateArguments::new(&record, &program),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        program.clear(function);
        record.clear(function);
        self.completion().copy_from(&pending, function);
        output.copy_from(pending.value(), function);
        pending.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    pub(crate) fn emit_module_has_async_dependencies(
        &mut self,
        plan: &ModuleEvaluationIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if self.emit_module_evaluation_capability_rejection(plan.module(), output, function) {
            return Ok(());
        }
        let schema = self.runtime_schema();
        let record = self.module_record_local(plan.module(), function)?;
        let gathered = self.emit_module_gather_call(&record, function)?;
        let found = schema.reserve_i32_local(function);
        gathered.count.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        found.store(function);
        output.set_boolean(found, function);
        schema.release_i32_local(found, function);
        gathered.clear(schema, function);
        record.clear(function);
        Ok(())
    }

    pub(crate) fn emit_module_deferred_import_evaluate(
        &mut self,
        plan: &ModuleEvaluationIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if self.emit_module_evaluation_capability_rejection(plan.module(), output, function) {
            return Ok(());
        }
        let schema = self.runtime_schema();
        let record = self.module_record_local(plan.module(), function)?;
        let pending = schema.reserve_completion(function);
        let program = self.emit_module_record_program(&record, function);
        schema
            .call_helper(
                ModuleDeferredImportArguments::new(&record, self.current_environment(), &program),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        program.clear(function);
        record.clear(function);
        self.completion().copy_from(&pending, function);
        output.copy_from(pending.value(), function);
        pending.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    pub(crate) fn emit_deferred_module_evaluate(
        &mut self,
        plan: &DeferredModuleEvaluationIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if self.emit_module_evaluation_capability_rejection(plan.module(), output, function) {
            return Ok(());
        }
        let schema = self.runtime_schema();
        let record = self.module_record_local(plan.module(), function)?;
        let ready = schema.reserve_i32_local(function);
        let pending = schema.reserve_completion(function);
        let program = self.emit_module_record_program(&record, function);
        schema
            .call_helper(
                ModuleReadyArguments::new(&record, &program),
                self.runtime_helper_base()?,
                function,
            )
            .store(ready, function);
        ready.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::DEFERRED_MODULE_IS_NOT_READY_FOR_SYNCHRONOUS_EVALUATION,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .call_helper(
                ModuleEvaluateArguments::new(&record, &program),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        program.clear(function);
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        let promise = schema.reserve_gc_local(function).initialize(
            pending
                .value()
                .cast_reference::<PromiseObject>(schema, function),
            function,
        );
        self.emit_module_read_settled_evaluation(&promise, &pending, function)?;
        promise.clear(function);
        record.clear(function);
        schema.release_i32_local(ready, function);
        self.completion().copy_from(&pending, function);
        output.copy_from(pending.value(), function);
        pending.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }
}
