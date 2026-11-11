//! AsyncIterator disposal owns complete Get/Call/PromiseResolve completions.
use super::super::*;
use crate::emit::AccessorThrowRouting;
use crate::gc_types::{CompletionLocals, GcLocal, PromiseCapability};

impl FunctionBuilder<'_> {
    pub(crate) fn emit_async_iterator_prototype_async_dispose(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let constructor = self.emit_current_function_realm_intrinsic_promise_constructor(f);
        let capability =
            self.emit_new_current_function_realm_intrinsic_promise_capability(constructor, f)?;
        let promise = s.reserve_value_local(f);
        self.emit_read_promise_capability_promise(&capability, &promise, f);
        let receiver = s.reserve_value_local(f);
        let boxed_receiver = s.reserve_value_local(f);
        let method = s.reserve_value_local(f);
        let awaited = s.reserve_value_local(f);
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        let pending = s.reserve_completion(f);
        self.compile_this_to_locals(&receiver, f)?;
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.compile_nullish_tagged_i32(receiver.tag(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::ASYNCITERATOR_ASYNCDISPOSE_RECEIVER_IS_NULL_OR_UNDEFINED,
            &pending,
            f,
        )?;
        self.emit_ai_dispose_reject_abrupt(&capability, &pending, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_value_to_current_function_realm_object_locals(&receiver, &pending, f)?;
        self.emit_ai_dispose_reject_abrupt(&capability, &pending, exit, f)?;
        boxed_receiver.copy_from(pending.value(), f);
        let key = self.emit_function_string_key("return", f)?;
        // GetV boxes only the lookup base and preserves the original receiver.
        self.emit_object_read_with_throw_routing(
            &boxed_receiver,
            &receiver,
            &key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            f,
        )?;
        key.clear(f);
        self.emit_ai_dispose_reject_abrupt(&capability, &pending, exit, f)?;
        method.copy_from(pending.value(), f);
        self.compile_nullish_tagged_i32(method.tag(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        self.emit_call_promise_capability(
            &capability,
            PromiseSettlement::Fulfill,
            &undefined,
            &pending,
            f,
        )?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_is_callable_i32(&method, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::ASYNCITERATOR_ASYNCDISPOSE_RETURN_METHOD_IS_NOT_CALLABLE,
            &pending,
            f,
        )?;
        self.emit_ai_dispose_reject_abrupt(&capability, &pending, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // The normative argument List is empty, including on callable Proxies.
        let arguments = self.emit_pre_evaluated_arg_vector(&[], f);
        self.emit_function_or_proxy_call_with_argv(&method, &receiver, &arguments, &pending, f)?;
        arguments.clear(f);
        self.emit_ai_dispose_reject_abrupt(&capability, &pending, exit, f)?;
        awaited.copy_from(pending.value(), f);
        let realm = self.emit_execution_realm(f);
        let context = self.emit_realm_function_materialization_context_from_realm(&realm, f);
        let builtin = StandardBuiltinId::AsyncIteratorPrototypeAsyncDisposeFulfilled;
        let metadata = self
            .functions
            .get(&builtin.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported("missing AsyncIterator asyncDispose fulfillment entry")
            })?;
        let callback = s.reserve_gc_local(f).initialize(
            self.emit_function_value_payload_in_realm(&metadata, &context, f)?,
            f,
        );
        let fulfilled = s.reserve_value_local(f);
        fulfilled.set_reference(&callback, s, f);
        // The outer capability handles both branches. Undefined rejection is
        // the ordinary rejection passthrough; the fulfillment callback captures
        // nothing and discards the awaited return value.
        self.emit_intrinsic_await_with_handlers(&awaited, &fulfilled, &undefined, &capability, f)?;
        pending.copy_from(self.completion(), f);
        self.emit_ai_dispose_reject_abrupt(&capability, &pending, exit, f)?;
        fulfilled.clear(f);
        callback.clear(f);
        self.release_realm_function_materialization_context(context, f);
        realm.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&promise, f);
        pending.clear(f);
        undefined.clear(f);
        awaited.clear(f);
        method.clear(f);
        boxed_receiver.clear(f);
        receiver.clear(f);
        promise.clear(f);
        capability.clear(f);
        Ok(())
    }

    fn emit_ai_dispose_reject_abrupt(
        &mut self,
        capability: &GcLocal<PromiseCapability>,
        pending: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let settled = self.runtime_schema().reserve_completion(f);
        self.emit_call_promise_capability(
            capability,
            PromiseSettlement::Reject,
            pending.value(),
            &settled,
            f,
        )?;
        settled.clear(f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_async_iterator_prototype_async_dispose_fulfilled(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let undefined = self.runtime_schema().reserve_value_local(f);
        undefined.set_undefined(f);
        self.completion().set_normal(&undefined, f);
        undefined.clear(f);
        Ok(())
    }
}
