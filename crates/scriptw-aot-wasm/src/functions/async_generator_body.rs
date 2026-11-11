//! The saved body keeps whole completions and explicit Await ownership.

use super::*;
use crate::gc_types::{
    AsyncGeneratorActivation, AsyncGeneratorActivationSchema, AsyncGeneratorReturnStage, GcI32Constant, GcLocal, GcOperand, StoredValue,
};
use crate::runtime_helpers::HelperParameters;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_mark_async_generator_return_awaited(
        &self,
        activation: &GcLocal<AsyncGeneratorActivation>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::RETURN_STAGE)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorReturnStage::Awaited),
                schema,
                function,
            );
    }

    pub(crate) fn emit_start_async_generator_body(
        &mut self,
        activation: &GcLocal<AsyncGeneratorActivation>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let base = self.runtime_helper_base()?;
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::AsyncGeneratorStartBodyArguments::new(
                    activation,
                    self.completion(),
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(self.completion(), function);
        Ok(())
    }

    pub(crate) fn compile_async_generator_start_body_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut function = self
            .begin_helper_body(crate::runtime_helpers::RuntimeHelperId::AsyncGeneratorStartBody);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::AsyncGeneratorStartBodyParameters>(
                &mut function,
            );
        self.completion()
            .copy_from(&parameters.pending, &mut function);
        self.emit_start_async_generator_body_inner(&parameters.activation, &mut function)?;
        self.completion().emit(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    // Only this registered body may emit the scheduling algorithm. Its nested
    // drain operations use the independent registered queue helper.
    fn emit_start_async_generator_body_inner(
        &mut self,
        activation: &GcLocal<AsyncGeneratorActivation>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let row = schema.struct_type::<AsyncGeneratorActivation>();
        let caller_realm = self.load_current_realm(function);
        let incoming = schema.reserve_completion(function);
        incoming.copy_from(self.completion(), function);
        let body = schema.reserve_completion(function);
        let awaited = schema.reserve_completion(function);
        let status = schema.reserve_i32_local(function);
        let resume = schema.reserve_i32_local(function);
        let point = schema.reserve_i32_local(function);
        row.field(AsyncGeneratorActivationSchema::RESUME_POINT)
            .read(activation, schema, function)
            .store(point, function);
        point.load(function);
        function.instruction(&Instruction::I32Const(
            crate::gc_types::INITIALIZING_RESUME_POINT,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        row.field(AsyncGeneratorActivationSchema::RESUME_POINT)
            .write(activation, GcOperand::i32(0), schema, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let stopped = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        row.field(AsyncGeneratorActivationSchema::EXECUTION_STATE)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorExecutionState::Executing),
                schema,
                function,
            );
        row.field(AsyncGeneratorActivationSchema::BODY_STATUS)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorBodyStatus::Running),
                schema,
                function,
            );
        row.field(AsyncGeneratorActivationSchema::RETURN_STAGE)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorReturnStage::Unawaited),
                schema,
                function,
            );
        self.emit_saved_async_generator_body_call(activation, &body, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(body.value(), function),
            function,
        );
        row.field(AsyncGeneratorActivationSchema::BODY_RESULT)
            .write(
                activation,
                GcOperand::reference(&stored, schema),
                schema,
                function,
            );
        stored.clear(function);
        row.field(AsyncGeneratorActivationSchema::BODY_STATUS)
            .read(activation, schema, function)
            .store(status, function);
        status.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorBodyStatus::Yield,
        )));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let delegate = schema.reserve_gc_local(function).initialize(
            row.field(AsyncGeneratorActivationSchema::DELEGATE)
                .read(activation, schema, function)
                .reference(),
            function,
        );
        delegate.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        // Plain Yield owns Await(value); delegation has already produced its
        // iterator value and uses the direct AsyncGeneratorYield operation.
        self.emit_async_generator_yield_reactions(activation, body.value(), function)?;
        awaited.copy_from(self.completion(), function);
        awaited.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        // PromiseResolve failed before Await could suspend. Feed the original
        // Throw to the retained yield continuation in this same invocation.
        function.instruction(&Instruction::I32Const(1));
        resume.store(function);
        self.emit_async_generator_resume_value(
            activation,
            awaited.value(),
            resume,
            AsyncGeneratorResumeKind::Fulfill,
            AsyncGeneratorResumeKind::Reject,
            function,
        );
        self.completion().initialize(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        resume.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        self.emit_complete_async_generator_yield(
            activation,
            body.value(),
            resume,
            Some(&caller_realm),
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        delegate.clear(function);
        resume.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_branch_to_target(stopped, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        status.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorBodyStatus::Await,
        )));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        // Its actual reactions retain the activation and all live frame roots.
        self.emit_branch_to_target(stopped, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        status.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorBodyStatus::Running,
        )));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        body.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Return.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        row.field(AsyncGeneratorActivationSchema::BODY_STATUS)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorBodyStatus::Complete),
                schema,
                function,
            );
        row.field(AsyncGeneratorActivationSchema::EXECUTION_STATE)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorExecutionState::DrainingQueue),
                schema,
                function,
            );
        row.field(AsyncGeneratorActivationSchema::RETURN_STAGE)
            .read(activation, schema, function)
            .store(status, function);
        status.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorReturnStage::Awaited,
        )));
        function.instruction(&Instruction::I32Eq);
        // Source `return;` reaches the body with an actual undefined value and
        // no Await obligation. Expression returns have already performed their
        // mandatory IR Await, even when that expression evaluates to undefined.
        // External .return(value) keeps its separate queue/yield-return Await.
        body.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        body.set_normal(body.value(), function);
        self.emit_complete_async_generator_step(
            activation,
            &body,
            AsyncGeneratorCompleteStepKind::Completed,
            Some(&caller_realm),
            function,
        )?;
        self.emit_drain_async_generator_queue(activation, function)?;
        function.instruction(&Instruction::Else);
        self.emit_async_generator_await_return_reactions(
            activation,
            body.value(),
            &awaited,
            function,
        )?;
        awaited.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_complete_async_generator_step(
            activation,
            &awaited,
            AsyncGeneratorCompleteStepKind::Completed,
            Some(&caller_realm),
            function,
        )?;
        self.emit_drain_async_generator_queue(activation, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        body.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        row.field(AsyncGeneratorActivationSchema::BODY_STATUS)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorBodyStatus::Throw),
                schema,
                function,
            );
        function.instruction(&Instruction::Else);
        body.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        body.initialize(function);
        row.field(AsyncGeneratorActivationSchema::BODY_STATUS)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorBodyStatus::Complete),
                schema,
                function,
            );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        row.field(AsyncGeneratorActivationSchema::EXECUTION_STATE)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorExecutionState::DrainingQueue),
                schema,
                function,
            );
        self.emit_complete_async_generator_step(
            activation,
            &body,
            AsyncGeneratorCompleteStepKind::Completed,
            Some(&caller_realm),
            function,
        )?;
        self.emit_drain_async_generator_queue(activation, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&incoming, function);
        schema.release_i32_local(point, function);
        schema.release_i32_local(resume, function);
        schema.release_i32_local(status, function);
        awaited.clear(function);
        body.clear(function);
        incoming.clear(function);
        caller_realm.clear(function);
        Ok(())
    }
}
