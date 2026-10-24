use super::*;
use crate::function_layout::ProgramInitializeIndex;
mod empty_strict;
mod global_declarations;
mod program_initialize;
pub(crate) use program_initialize::{CompletedProgramInitializeBody, ProgramInitializeCompilation};

impl<'a> FunctionBuilder<'a> {
    fn normalize_base_class_constructor_result(&mut self, function: &mut Function) {
        if !self.current_function_meta().is_some_and(|meta| {
            meta.protocol().class_kind() == ClassFunctionKind::Constructor
                && !meta.is_derived_constructor
        }) {
            return;
        }
        self.completion.kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_NORMAL as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.completion.value().copy_from(
            self.body_entry_locals()
                .expect("base class body owns a callable entry")
                .this_value(),
            function,
        );
        function.instruction(&Instruction::End);
    }

    pub(super) fn compile_callable(&mut self) -> Result<EmittedFunction, EmitError> {
        let entry = self
            .planned_entry()
            .expect("source callable builder owns a planned entry")
            .clone();
        let body = self.compile()?;
        Ok(EmittedFunction::from_completed_callable(
            CompletedCallableBody { entry, body },
        ))
    }

    pub(super) fn compile_builtin_callable(&mut self) -> Result<EmittedFunction, EmitError> {
        let entry = self
            .planned_entry()
            .expect("native builder owns a planned entry")
            .clone();
        let body = self.compile_builtin()?;
        Ok(EmittedFunction::from_completed_callable(
            CompletedCallableBody { entry, body },
        ))
    }

    pub(super) fn compile(&mut self) -> Result<Function, EmitError> {
        if self.is_program_initialize() || (self.is_main() && self.uses_heap) {
            return Err(EmitError::unsupported(
                "a GC bootstrap or heap main requires its dedicated typed compilation entry",
            ));
        }
        self.compile_javascript_body(None)
    }

    pub(super) fn compile_main_with_program_initialize(
        &mut self,
        initializer: Option<ProgramInitializeIndex>,
    ) -> Result<Function, EmitError> {
        if !self.is_main() || self.uses_heap != initializer.is_some() {
            return Err(EmitError::unsupported(
                "main and its typed P initializer plan disagree",
            ));
        }
        self.compile_javascript_body(initializer)
    }

    fn compile_javascript_body(
        &mut self,
        initializer: Option<ProgramInitializeIndex>,
    ) -> Result<Function, EmitError> {
        if self.functions.module_execution_record_count() == 0
            && self.current_function_meta().is_some_and(|meta| {
                matches!(
                    meta.protocol(),
                    FunctionProtocolIr::ModuleActivation
                        | FunctionProtocolIr::AsyncModuleActivation
                )
            })
        {
            return Err(EmitError::unsupported(
                "private module activation requires its validated execution graph",
            ));
        }
        if self.uses_heap
            && self
                .planned_entry()
                .is_some_and(|entry| entry.has_source_activation())
        {
            if let Some(plan) = self.function_arguments_protocol.take_empty_strict_body() {
                return self.compile_empty_strict_body(plan);
            }
        }
        let mut function = self.take_owned_body();

        self.push_scope();
        if self.uses_heap {
            if self.is_main() {
                let index = initializer.expect("heap main owns its completed P bootstrap");
                let program = self
                    .runtime_schema()
                    .call_program_initialize(index, &mut function);
                self.bind_entry_program(program, &mut function);
            }
            self.init_current_realm(&mut function)?;
            self.init_current_env(&mut function)?;
            self.init_template_source_execution(&mut function)?;
            self.initialize_direct_eval_execution_context(&mut function)?;
            if let FunctionModuleState::PreparedScript(unit, _) = self.module_state {
                self.emit_instantiate_prepared_script_declarations(unit, &mut function)?;
            }
            self.init_runtime_roots(&mut function)?;
            self.emit_initialize_main_global_lexicals(&mut function)?;
            self.init_script_global_object(&mut function)?;
            self.bind_captured_bindings(&mut function);
        }
        self.emit_enter_source_activation(&mut function)?;
        let source_activation_epilogue = self.open_source_activation_epilogue(&mut function);
        let schema = self.runtime_schema();
        let frame = self
            .body_entry_locals()
            .and_then(|entry| entry.resume_frame())
            .map(|frame| {
                schema
                    .reserve_gc_local::<crate::gc_types::InvocationFrame, NonNullable>(
                        &mut function,
                    )
                    .initialize(frame.load(schema, &mut function), &mut function)
            });
        if let Some(frame) = &frame {
            let initialized = schema.reserve_i32_local(&mut function);
            schema
                .struct_type::<crate::gc_types::InvocationFrame>()
                .field(crate::gc_types::InvocationFrameSchema::INITIALIZED)
                .read(frame, schema, &mut function)
                .store(initialized, &mut function);
            initialized.load(&mut function);
            schema.release_i32_local(initialized, &mut function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
        }
        self.bind_self_function(&mut function)?;
        if let Some(constructor_meta) = self
            .current_function_meta()
            .filter(|meta| {
                meta.protocol().class_kind() == ClassFunctionKind::Constructor
                    && !meta.is_derived_constructor
            })
            .cloned()
        {
            let context = schema
                .reserve_gc_local::<crate::gc_types::FunctionContext, NonNullable>(&mut function)
                .initialize(
                    self.body_entry_locals()
                        .expect("base class owns its entry")
                        .function_context()
                        .expect("base class owns its context")
                        .load(schema, &mut function),
                    &mut function,
                );
            let receiver = schema.reserve_value_local(&mut function);
            receiver.copy_from(
                self.body_entry_locals()
                    .expect("base class owns its entry")
                    .this_value(),
                &mut function,
            );
            self.emit_initialize_instance_elements(
                &constructor_meta,
                &context,
                &receiver,
                &mut function,
            )?;
            receiver.clear(&mut function);
            context.clear(&mut function);
        }
        self.bind_parameters(&mut function)?;
        self.completion
            .set_kind(CompletionKind::Normal, &mut function);
        for name in self.source_frame_var_names() {
            // A main-frame write name always owns storage. Internal functions
            // may instead reuse their parameter or implicit `arguments`
            // binding, preserving their existing hoisting behavior.
            let reuses_function_binding = !self.is_main()
                && (self.params.iter().any(|param| param.name == name)
                    || (name == LEXICAL_ARGUMENTS_NAME
                        && self.function_arguments_protocol.present().is_some()));
            if reuses_function_binding {
                continue;
            }
            let storage = if let Some(slot) = self.owned_env_slot(&name) {
                BindingStorage::EnvSlot { slot, hops: 0 }
            } else {
                self.allocate_initialized_var_binding(&mut function)
            };
            self.initialize_function_var_binding(&name, storage, &mut function)?;
            self.binding_scopes
                .last_mut()
                .expect("binding scope stack must exist")
                .insert(name, storage);
        }
        self.completion.value().set_undefined(&mut function);
        if let Some(frame) = &frame {
            self.initialize_direct_lexical_bindings(&self.body.statements, &mut function);
            schema
                .struct_type::<crate::gc_types::InvocationFrame>()
                .field(crate::gc_types::InvocationFrameSchema::INITIALIZED)
                .write(
                    frame,
                    crate::gc_types::GcOperand::boolean(true),
                    schema,
                    &mut function,
                );
            function.instruction(&Instruction::End);
        }
        if self
            .current_function_meta()
            .is_some_and(|meta| meta.protocol() == FunctionProtocolIr::AsyncModuleActivation)
        {
            let crate::function_entry::ResumableEntryLocals::Async(activation) = self
                .body_entry_locals()
                .expect("async module owns an entry")
                .resume_activation()
                .expect("async module owns an activation")
            else {
                return Err(EmitError::unsupported(
                    "async module requires its typed AsyncActivation",
                ));
            };
            let mode = schema.reserve_i32_local(&mut function);
            schema
                .struct_type::<crate::gc_types::AsyncActivation>()
                .field(crate::gc_types::AsyncActivationSchema::MODULE_ENTRY_MODE)
                .read(activation, schema, &mut function)
                .store(mode, &mut function);
            let state = self
                .body_entry_locals()
                .expect("async module owns an entry")
                .resume_point()
                .expect("async module owns a resume point");
            mode.load(&mut function);
            function.instruction(&Instruction::I32Const(
                crate::gc_types::GcI32Constant::encode(AsyncModuleEntryMode::Execute),
            ));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            let final_state = self
                .body
                .statements
                .iter()
                .rev()
                .find_map(Self::async_statement_exit_state)
                .expect("an async module has an instantiation boundary");
            state.load(&mut function);
            function.instruction(&Instruction::I32Eqz);
            state.load(&mut function);
            function.instruction(&Instruction::I32Const(final_state as i32));
            function.instruction(&Instruction::I32GtU);
            function.instruction(&Instruction::I32Or);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::Unreachable);
            function.instruction(&Instruction::End);
            function.instruction(&Instruction::Else);
            state.load(&mut function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::Unreachable);
            function.instruction(&Instruction::End);
            function.instruction(&Instruction::End);
            mode.load(&mut function);
            function.instruction(&Instruction::I32Const(
                crate::gc_types::GcI32Constant::encode(AsyncModuleEntryMode::Allocate),
            ));
            function.instruction(&Instruction::I32Eq);
            schema.release_i32_local(mode, &mut function);
            function.instruction(&Instruction::If(BlockType::Empty));
            self.completion.initialize(&mut function);
            self.emit_return_current_completion(&mut function);
            function.instruction(&Instruction::End);
        }
        if self.current_function_meta().is_some_and(|meta| {
            matches!(
                meta.protocol().execution_kind(),
                FunctionExecutionKind::Generator | FunctionExecutionKind::AsyncGenerator
            )
        }) {
            self.body_entry_locals()
                .expect("suspended body owns an entry")
                .resume_point()
                .expect("suspended body owns a resume point")
                .load(&mut function);
            function.instruction(&Instruction::I32Const(
                crate::gc_types::INITIALIZING_RESUME_POINT,
            ));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            self.completion.initialize(&mut function);
            self.emit_return_current_completion(&mut function);
            function.instruction(&Instruction::End);
        }
        if let Some(frame) = frame {
            frame.clear(&mut function);
        }
        if self
            .current_function_meta()
            .is_some_and(|meta| meta.is_synthetic_default_derived_constructor)
        {
            let arguments = schema
                .reserve_gc_local::<crate::gc_types::ValueArray, NonNullable>(&mut function)
                .initialize(
                    self.body_entry_locals()
                        .expect("derived body owns an entry")
                        .arguments()
                        .load(schema, &mut function),
                    &mut function,
                );
            let pending = schema.reserve_completion(&mut function);
            pending.initialize(&mut function);
            self.emit_super_construct_with_arg_vector(&arguments, &pending, &mut function)?;
            self.completion.copy_from(&pending, &mut function);
            pending.clear(&mut function);
            arguments.clear(&mut function);
            self.emit_propagate_current_throw_if_needed(&mut function);
        }
        let module_entry_kind = if self.is_main() {
            scriptw_ir::ModuleEntryEvaluationIr::in_root_block(self.body).map(|entry| entry.kind())
        } else {
            None
        };
        if module_entry_kind.is_some() {
            self.initialize_module_entry_completion(&mut function);
        }
        let main_job_checkpoint = if self.is_main() && self.uses_heap {
            let target = self.open_frame(ControlFrameKind::Block, &mut function);
            self.completion_exit.enter_main_job_checkpoint(target);
            Some(target)
        } else {
            None
        };
        if let Some(id) = self.module_prelude_id {
            let phase = self
                .global_prelude_phase
                .expect("global prelude has its P-local phase owner");
            phase.enter(&mut function);
            self.emit_module_prelude(id, &mut function)?;
            phase.completed(&mut function);
        }
        if let Some(id) = self.script_entry_after_prelude_id {
            self.emit_script_entry_after_prelude(id, &mut function)?;
        }
        self.compile_block_contents(self.body, &mut function)?;
        if let Some(target) = main_job_checkpoint {
            self.completion_exit.leave_main_job_checkpoint(target);
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
        }
        if matches!(self.return_abi(), ReturnAbi::MultiValue)
            && !matches!(self.module_state, FunctionModuleState::PreparedScript(_, _))
            && !self
                .current_function_meta()
                .is_some_and(|meta| meta.protocol().class_kind() == ClassFunctionKind::Constructor)
        {
            self.completion.value().set_undefined(&mut function);
        }
        self.normalize_base_class_constructor_result(&mut function);
        self.emit_derived_constructor_body_result(&mut function)?;
        if self.is_main() && self.uses_heap {
            self.emit_finish_weak_job(&mut function)?;
            if self
                .functions
                .monotonic_clock_nanos_import_function_index()
                .is_some()
            {
                function.instruction(&Instruction::Block(BlockType::Empty));
                function.instruction(&Instruction::Loop(BlockType::Empty));
                self.emit_drain_promise_jobs(&mut function)?;
                let progressed = self.emit_drain_atomics_wait_async_timeouts(&mut function)?;
                progressed.load(&mut function);
                self.schema.release_i32_local(progressed, &mut function);
                function.instruction(&Instruction::BrIf(0));
                function.instruction(&Instruction::End);
                function.instruction(&Instruction::End);
            } else {
                self.emit_drain_promise_jobs(&mut function)?;
            }
            if let Some(kind) = module_entry_kind {
                self.emit_module_entry_checkpoint(kind, &mut function)?;
            }
            // Every job that could still attach a handler has now run, so a
            // promise still marked unhandled really is an unhandled rejection.
            let promise_rejection_policy = match self.module_state {
                FunctionModuleState::Main(_, policy) => policy,
                FunctionModuleState::PreparedScript(_, _)
                | FunctionModuleState::Internal(_)
                | FunctionModuleState::RuntimeOperation(_)
                | FunctionModuleState::ProgramInitialize => {
                    unreachable!("only the main export reports unhandled rejections")
                }
            };
            self.emit_report_unhandled_rejection(promise_rejection_policy, &mut function)?;
            self.emit_capture_final_throw_constructor_name(&mut function)?;
        }
        self.pop_scope();

        self.finish_source_activation_epilogue(source_activation_epilogue, &mut function);
        self.completion.emit(&mut function);
        self.clear_body_entry_roots(&mut function);
        if let Some(execution) = self.template_source_execution.take() {
            execution.clear(&mut function);
        }
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn init_current_realm(&mut self, function: &mut Function) -> Result<(), EmitError> {
        if !self.is_main() || !self.uses_heap {
            return Ok(());
        }
        let schema = self.schema;
        let slot = schema.reserve_gc_local::<RealmRecord, NonNullable>(function);
        let realm = slot.initialize(self.emit_alloc_realm_record(1, 1, function)?, function);
        self.replace_current_realm(&realm, function);
        // The actual Global Environment owns the defining Realm before the
        // script activation adds its lexical/captured cells as a child.
        let global = self.emit_alloc_realm_global_environment(&realm, function)?;
        self.replace_current_environment(global.load(schema, function).nullable(), function);
        global.clear(function);
        realm.clear(function);
        Ok(())
    }

    fn compile_builtin(&mut self) -> Result<Function, EmitError> {
        let Some(function_id) = self.function_id.clone() else {
            return Err(EmitError::unsupported(
                "unsupported in scriptw wasm-aot first slice: missing builtin id",
            ));
        };
        let mut function = self.take_owned_body();
        self.push_scope();
        self.init_current_env(&mut function)?;
        self.initialize_direct_eval_execution_context(&mut function)?;
        self.completion
            .set_kind(CompletionKind::Normal, &mut function);
        self.completion.value().set_undefined(&mut function);
        if let Some(builtin) = StandardBuiltinId::from_function_id(&function_id) {
            self.compile_standard_builtin(builtin, &mut function)?;
        } else {
            match HostBuiltinId::from_function_id(&function_id) {
                Some(HostBuiltinId::Print) => self.compile_host_print_builtin(&mut function)?,
                Some(HostBuiltinId::Gc) => self.compile_host_gc_builtin(&mut function)?,
                Some(HostBuiltinId::AssertThrows) => {
                    self.compile_host_assert_throws_builtin(&mut function)?
                }
                Some(HostBuiltinId::IsConstructor) => {
                    self.compile_host_is_constructor_builtin(&mut function)?
                }
                Some(HostBuiltinId::CreateRealm) => {
                    self.compile_host_create_realm_builtin(&mut function)?
                }
                Some(HostBuiltinId::RealmEvalScript) => {
                    self.compile_host_realm_eval_script_builtin(&mut function)?
                }
                Some(HostBuiltinId::AsyncDisposableStackSyncDispose) => {
                    self.emit_async_disposable_stack_sync_dispose(&mut function)?
                }
                Some(HostBuiltinId::GeneratorFunctionConstructor) => self
                    .compile_dynamic_function_constructor_builtin(
                        DynamicFunctionKind::Generator,
                        &mut function,
                    )?,
                Some(HostBuiltinId::AsyncFunctionConstructor) => self
                    .compile_dynamic_function_constructor_builtin(
                        DynamicFunctionKind::Async,
                        &mut function,
                    )?,
                Some(HostBuiltinId::AsyncGeneratorFunctionConstructor) => self
                    .compile_dynamic_function_constructor_builtin(
                        DynamicFunctionKind::AsyncGenerator,
                        &mut function,
                    )?,
                Some(HostBuiltinId::CreateHTMLDDA) => {
                    self.compile_host_create_html_dda_builtin(&mut function)?
                }
                Some(HostBuiltinId::GetAbstractModuleSource) => {
                    self.compile_host_get_abstract_module_source_builtin(&mut function)?
                }
                Some(HostBuiltinId::HTMLDDA) => {
                    self.compile_host_html_dda_builtin(&mut function)?
                }
                Some(HostBuiltinId::ParseInt) => {
                    self.compile_host_parse_int_builtin(&mut function)?
                }
                Some(HostBuiltinId::ParseFloat) => {
                    self.compile_host_parse_float_builtin(&mut function)?
                }
                Some(HostBuiltinId::DetachArrayBuffer) => {
                    self.compile_host_detach_array_buffer_builtin(&mut function)?
                }
                Some(HostBuiltinId::AgentStart) => {
                    self.compile_host_agent_start_builtin(&mut function)?
                }
                Some(HostBuiltinId::AgentBroadcast) => {
                    self.compile_host_agent_broadcast_builtin(&mut function)?
                }
                Some(HostBuiltinId::AgentReceiveBroadcast) => {
                    self.compile_host_agent_receive_broadcast_builtin(&mut function)?
                }
                Some(HostBuiltinId::AgentReport) => {
                    self.compile_host_agent_report_builtin(&mut function)?
                }
                Some(HostBuiltinId::AgentGetReport) => {
                    self.compile_host_agent_get_report_builtin(&mut function)?
                }
                Some(HostBuiltinId::AgentSleep) => {
                    self.compile_host_agent_sleep_builtin(&mut function)?
                }
                Some(HostBuiltinId::AgentMonotonicNow) => {
                    self.compile_host_agent_monotonic_now_builtin(&mut function)?
                }
                Some(HostBuiltinId::AgentLeaving) => {
                    self.compile_host_agent_leaving_builtin(&mut function)?
                }
                None => {
                    return Err(EmitError::unsupported(format!(
                        "unsupported in scriptw wasm-aot first slice: unknown builtin `{function_id}`"
                    )));
                }
            }
        }
        self.pop_scope();
        self.emit_leave_source_activation(&mut function);
        self.completion.emit(&mut function);
        self.clear_body_entry_roots(&mut function);
        if let Some(execution) = self.template_source_execution.take() {
            execution.clear(&mut function);
        }
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
