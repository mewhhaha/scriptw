//! Private typed entries for retained embedding. JavaScript operations reuse
//! the canonical compiled property, call, Promise and module machinery.
use crate::data::RuntimeErrorMessage;
use crate::emit::{ControlFrameKind, FunctionBuilder};
use crate::gc_types::*;
use crate::module::EmitError;
use crate::runtime_helpers::*;
use crate::{PromiseRejectionPolicy, *};

pub const EMBEDDING_EXPORT: &str = "scriptw.embed.export";
pub const EMBEDDING_CALL: &str = "scriptw.embed.call";
pub const EMBEDDING_READ: &str = "scriptw.embed.read";
pub const EMBEDDING_CHECKPOINT: &str = "scriptw.embed.checkpoint";
pub const EMBEDDING_STRING: &str = "scriptw.embed.string";
pub const EMBEDDING_BYTES: &str = "scriptw.embed.bytes";
pub const EMBEDDING_BYTES_READ: &str = "scriptw.embed.bytes.read";
pub const EMBEDDING_PROMISE: &str = "scriptw.embed.promise";
pub const EMBEDDING_CANCEL: &str = "scriptw.embed.cancel";

impl FunctionBuilder<'_> {
    /// A returned intrinsic Promise has crossed to a host observer before the
    /// job checkpoint can report rejections. This is intrinsic host adoption:
    /// no observable .then, constructor, species or coercion is consulted.
    fn emit_embedding_adopt_result(&mut self, function: &mut Function) {
        let schema = self.runtime_schema();
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(
            crate::emit::CompletionKind::Normal.code() as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.completion().value().reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<PromiseObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let promise = schema.reserve_gc_local(function).initialize(
            self.completion()
                .value()
                .cast_reference::<PromiseObject>(schema, function),
            function,
        );
        schema.field(PromiseObjectSchema::HANDLED).write(
            &promise,
            GcOperand::boolean(true),
            schema,
            function,
        );
        promise.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    fn emit_embedding_drain(&mut self, function: &mut Function) -> Result<(), EmitError> {
        // Weak-job finalization uses the builder's completion as scratch. Keep
        // the operation's exact Normal/Throw value rooted across that boundary.
        let pending = self.runtime_schema().reserve_completion(function);
        pending.copy_from(self.completion(), function);
        self.emit_finish_weak_job(function)?;
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        if self
            .functions
            .monotonic_clock_nanos_import_function_index()
            .is_some()
        {
            // Notifications and elapsed waitAsync deadlines do not enqueue a
            // Promise job themselves. Poll even when the job queue is empty;
            // the blocking drain would wait indefinitely on an infinite wait.
            self.emit_poll_atomics_wait_async_timeouts(function)?;
        }
        self.emit_drain_promise_jobs(function)
    }

    pub(crate) fn compile_embedding_call(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::EmbeddingCall);
        let parameters = self.helper_parameters::<EmbeddingCallParameters>(&mut function);
        self.replace_current_realm(&parameters.caller_execution_realm, &mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_function_or_proxy_call_with_argv(
            &parameters.callee,
            &parameters.this_value,
            &parameters.arguments,
            &result,
            &mut function,
        )?;
        self.completion().copy_from(&result, &mut function);
        result.clear(&mut function);
        self.emit_embedding_adopt_result(&mut function);
        self.emit_embedding_drain(&mut function)?;
        self.completion().emit(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_embedding_read_property(
        &mut self,
        target: &ValueLocals,
        key: &GcLocal<StringValue>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let key_value = schema.reserve_value_local(function);
        key_value.set_reference(key, schema, function);
        schema
            .call_helper(
                DynamicPropertyReadArguments::new(
                    target,
                    target,
                    &key_value,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(self.completion(), function);
        key_value.clear(function);
        self.emit_embedding_adopt_result(function);
        self.emit_embedding_drain(function)
    }

    pub(crate) fn compile_embedding_read(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::EmbeddingRead);
        let parameters = self.helper_parameters::<EmbeddingReadParameters>(&mut function);
        self.replace_current_realm(&parameters.caller_execution_realm, &mut function);
        self.emit_embedding_read_property(&parameters.target, &parameters.key, &mut function)?;
        self.completion().emit(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(crate) fn compile_embedding_export(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::EmbeddingExport);
        let parameters = self.helper_parameters::<EmbeddingExportParameters>(&mut function);
        self.replace_current_realm(&parameters.caller_execution_realm, &mut function);
        let module = self
            .functions
            .module_graph()
            .and_then(|graph| graph.module_entry())
            .ok_or_else(|| {
                EmitError::unsupported("retained exports require an actual Module entry")
            })?;
        let schema = self.runtime_schema();
        let record =
            self.module_record_in_realm(module, &parameters.caller_execution_realm, &mut function)?;
        let cell = self.module_namespace_cell(
            &record,
            scriptw_ir::ModuleNamespaceModeIr::Eager,
            &mut function,
        );
        let namespace = schema.reserve_value_local(&mut function);
        let initialized = self.emit_read_environment_cell(&cell, &namespace, &mut function);
        initialized.load(&mut function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut function);
        // Host lifecycle admission guarantees initialization, and the linked
        // graph guarantees that this exact entry namespace was materialized.
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(initialized, &mut function);
        cell.clear(&mut function);
        record.clear(&mut function);
        self.emit_embedding_read_property(&namespace, &parameters.key, &mut function)?;
        namespace.clear(&mut function);
        self.completion().emit(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(crate) fn compile_embedding_checkpoint(
        &mut self,
        policy: PromiseRejectionPolicy,
    ) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::EmbeddingCheckpoint);
        let parameters = self.helper_parameters::<EmbeddingCheckpointParameters>(&mut function);
        self.replace_current_realm(&parameters.caller_execution_realm, &mut function);
        self.completion().initialize(&mut function);
        self.emit_embedding_drain(&mut function)?;
        if self
            .functions
            .module_graph()
            .and_then(|graph| graph.module_entry())
            .is_some()
        {
            self.emit_module_entry_checkpoint(
                scriptw_ir::ModuleEntryEvaluationKindIr::Promise,
                &mut function,
            )?;
        }
        self.emit_report_unhandled_rejection(policy, &mut function)?;
        self.emit_capture_final_throw_constructor_name(&mut function)?;
        self.completion().emit(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(crate) fn compile_embedding_string(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::EmbeddingString);
        let parameters = self.helper_parameters::<EmbeddingStringParameters>(&mut function);
        self.completion().initialize(&mut function);
        let output = self.runtime_schema().reserve_value_local(&mut function);
        self.emit_foreign_string_result(&parameters.units, &output, &mut function)?;
        self.completion().set_normal(&output, &mut function);
        output.clear(&mut function);
        self.completion().emit(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(crate) fn compile_embedding_bytes(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::EmbeddingBytes);
        let parameters = self.helper_parameters::<EmbeddingBytesParameters>(&mut function);
        self.replace_current_realm(&parameters.caller_execution_realm, &mut function);
        self.completion().initialize(&mut function);
        let output = self.runtime_schema().reserve_value_local(&mut function);
        self.emit_foreign_bytes_result(&parameters.bytes, &output, &mut function)?;
        self.completion().set_normal(&output, &mut function);
        output.clear(&mut function);
        self.completion().emit(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(crate) fn compile_embedding_cancel(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::EmbeddingCancel);
        let parameters = self.helper_parameters::<EmbeddingCancelParameters>(&mut function);
        self.replace_current_realm(&parameters.caller_execution_realm, &mut function);
        let error = self.runtime_schema().reserve_value_local(&mut function);
        self.emit_runtime_error_object(
            NativeErrorKind::Error,
            RuntimeErrorMessage::FOREIGN_HOST_OPERATION_CANCELLED,
            &error,
            &mut function,
        )?;
        self.completion().set_normal(&error, &mut function);
        error.clear(&mut function);
        self.completion().emit(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(crate) fn compile_embedding_bytes_read(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::EmbeddingBytesRead);
        let parameters = self.helper_parameters::<EmbeddingBytesReadParameters>(&mut function);
        let bytes = self.emit_foreign_bytes_read(&parameters.value, &mut function)?;
        bytes.load(self.runtime_schema(), &mut function);
        bytes.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Normal value plus an independent host status: -1=not a Promise,
    /// 0=pending, 1=fulfilled, 2=rejected. The rejected value stays exact.
    pub(crate) fn compile_embedding_promise(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::EmbeddingPromise);
        let parameters = self.helper_parameters::<EmbeddingPromiseParameters>(&mut function);
        let schema = self.runtime_schema();
        let state = schema.reserve_i32_local(&mut function);
        state.set_constant(-1, &mut function);
        self.completion().initialize(&mut function);
        parameters.value.reference().load(&mut function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<PromiseObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, &mut function);
        let promise = schema.reserve_gc_local(&mut function).initialize(
            parameters
                .value
                .cast_reference::<PromiseObject>(schema, &mut function),
            &mut function,
        );
        schema.field(PromiseObjectSchema::HANDLED).write(
            &promise,
            GcOperand::boolean(true),
            schema,
            &mut function,
        );
        self.emit_load_promise_state_strict(&promise, state, &mut function);
        let stored = schema.reserve_gc_local(&mut function).initialize(
            schema
                .field(PromiseObjectSchema::RESULT)
                .read(&promise, schema, &mut function)
                .reference(),
            &mut function,
        );
        schema.struct_type::<StoredValue>().read_into(
            &stored,
            self.completion().value(),
            schema,
            &mut function,
        );
        stored.clear(&mut function);
        promise.clear(&mut function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().emit(&mut function);
        state.load(&mut function);
        schema.release_i32_local(state, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
