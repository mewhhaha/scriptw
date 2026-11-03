//! Native byte ReadableStreams. All observable objects, callbacks, queued bytes
//! and Promise continuations use the single Wasm-GC object model.
//!
//! This slice supports byte sources and default readers; BYOB, auto-allocation,
//! generic streams, tee, piping and async iteration are separate work.
use super::super::*;
use crate::functions::{
    NonArrayRealmIntrinsicSlot as Slot, OrdinaryDefaultPrototype,
    RealmFunctionMaterializationContext,
};
use crate::gc_types::*;
use crate::heap::PromiseSettlement;
use wasm_encoder::Ieee64;

mod constructor;
mod controller;
mod queue;
mod reader;

pub(crate) const STREAM_BUILTINS: &[StandardBuiltinId] = &[
    StandardBuiltinId::ReadableStreamConstructor,
    StandardBuiltinId::ReadableStreamGetReader,
    StandardBuiltinId::ReadableStreamCancel,
    StandardBuiltinId::ReadableStreamLocked,
    StandardBuiltinId::ReadableStreamDefaultReaderConstructor,
    StandardBuiltinId::ReadableStreamDefaultReaderRead,
    StandardBuiltinId::ReadableStreamDefaultReaderCancel,
    StandardBuiltinId::ReadableStreamDefaultReaderReleaseLock,
    StandardBuiltinId::ReadableStreamDefaultReaderClosed,
    StandardBuiltinId::ReadableByteStreamControllerConstructor,
    StandardBuiltinId::ReadableByteStreamControllerEnqueue,
    StandardBuiltinId::ReadableByteStreamControllerClose,
    StandardBuiltinId::ReadableByteStreamControllerError,
    StandardBuiltinId::ReadableByteStreamControllerDesiredSize,
    StandardBuiltinId::ReadableStreamStartFulfilled,
    StandardBuiltinId::ReadableStreamStartRejected,
    StandardBuiltinId::ReadableStreamPullFulfilled,
    StandardBuiltinId::ReadableStreamPullRejected,
    StandardBuiltinId::ReadableStreamCancelFulfilled,
    StandardBuiltinId::ReadableStreamPullIfNeeded,
];
pub(crate) fn is_stream_builtin(builtin: StandardBuiltinId) -> bool {
    STREAM_BUILTINS.contains(&builtin)
}
pub(crate) const STREAM_POOL_STRINGS: &[&str] = &[
    "ReadableStream",
    "ReadableStreamDefaultReader",
    "ReadableByteStreamController",
    "autoAllocateChunkSize",
    "cancel",
    "pull",
    "start",
    "type",
    "bytes",
    "highWaterMark",
    "size",
    "getReader",
    "locked",
    "read",
    "releaseLock",
    "closed",
    "enqueue",
    "close",
    "error",
    "desiredSize",
    "mode",
    "byob",
];

impl FunctionBuilder<'_> {
    pub(crate) fn emit_initialize_stream_intrinsics(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        use crate::intrinsics::IntrinsicPropertyLayoutId as Layout;
        for (ctor, slot, layout) in [
            (
                StandardBuiltinId::ReadableStreamConstructor,
                Slot::ReadableStreamPrototype,
                Layout::ReadableStreamPrototype,
            ),
            (
                StandardBuiltinId::ReadableStreamDefaultReaderConstructor,
                Slot::ReadableStreamDefaultReaderPrototype,
                Layout::ReadableStreamDefaultReaderPrototype,
            ),
            (
                StandardBuiltinId::ReadableByteStreamControllerConstructor,
                Slot::ReadableByteStreamControllerPrototype,
                Layout::ReadableByteStreamControllerPrototype,
            ),
        ] {
            let s = self.runtime_schema();
            let header = s.reserve_gc_local(f).initialize(
                self.emit_alloc_intrinsic_layout_header(
                    object_prototype,
                    layout,
                    realm.realm(),
                    f,
                )?,
                f,
            );
            let prototype = s.reserve_value_local(f);
            prototype.set_reference(&header, s, f);
            self.emit_store_non_array_realm_intrinsic(realm.realm(), slot, &prototype, f);
            let constructor = self.init_builtin_constructor_object(ctor, realm, &prototype, f)?;
            constructor.clear(f);
            prototype.clear(f);
            header.clear(f);
        }
        Ok(())
    }

    pub(crate) fn emit_stream_builtin(
        &mut self,
        builtin: StandardBuiltinId,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        use StandardBuiltinId as B;
        match builtin {
            B::ReadableStreamConstructor => self.emit_stream_constructor(f),
            B::ReadableStreamGetReader | B::ReadableStreamDefaultReaderConstructor => {
                self.emit_stream_get_reader(builtin == B::ReadableStreamDefaultReaderConstructor, f)
            }
            B::ReadableStreamCancel | B::ReadableStreamDefaultReaderCancel => {
                self.emit_stream_cancel(builtin == B::ReadableStreamDefaultReaderCancel, f)
            }
            B::ReadableStreamLocked => self.emit_stream_locked(f),
            B::ReadableStreamDefaultReaderRead => self.emit_stream_reader_read(f),
            B::ReadableStreamDefaultReaderClosed => self.emit_stream_reader_closed(f),
            B::ReadableStreamDefaultReaderReleaseLock => self.emit_stream_reader_release(f),
            B::ReadableByteStreamControllerConstructor => {
                let output = self.runtime_schema().reserve_completion(f);
                self.emit_throw_current_function_realm_type_error(
                    RuntimeErrorMessage::STREAM_ILLEGAL_CONSTRUCTOR,
                    &output,
                    f,
                )?;
                self.completion().copy_from(&output, f);
                output.clear(f);
                Ok(())
            }
            B::ReadableByteStreamControllerEnqueue => self.emit_stream_controller_enqueue(f),
            B::ReadableByteStreamControllerClose => self.emit_stream_controller_close(f),
            B::ReadableByteStreamControllerError => self.emit_stream_controller_error(f),
            B::ReadableByteStreamControllerDesiredSize => {
                self.emit_stream_controller_desired_size(f)
            }
            B::ReadableStreamPullIfNeeded => self.emit_stream_pull_if_needed(f),
            B::ReadableStreamStartFulfilled
            | B::ReadableStreamStartRejected
            | B::ReadableStreamPullFulfilled
            | B::ReadableStreamPullRejected => self.emit_stream_reaction(builtin, f),
            B::ReadableStreamCancelFulfilled => {
                self.completion().value().set_undefined(f);
                self.completion().set_kind(CompletionKind::Normal, f);
                Ok(())
            }
            _ => Err(EmitError::unsupported(
                "non-stream builtin reached stream dispatch",
            )),
        }
    }

    fn stream_end_if(&mut self, f: &mut Function) {
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }
    fn stream_end_block(&mut self, f: &mut Function) {
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }
    fn stream_abrupt(
        &mut self,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) {
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        output.copy_from(pending, f);
        self.emit_branch_to_target(exit, f);
        self.stream_end_if(f);
    }
    fn stream_error_exit(
        &mut self,
        message: RuntimeErrorMessage,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_throw_current_function_realm_type_error(message, output, f)?;
        self.emit_branch_to_target(exit, f);
        Ok(())
    }
    fn stream_is_undefined(&self, value: &ValueLocals, f: &mut Function) {
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
    }
    fn stream_is_state(
        &self,
        stream: &GcLocal<ReadableStreamObject>,
        state: ReadableStreamState,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let k = s.reserve_i32_local(f);
        s.field(ReadableStreamObjectSchema::STATE)
            .read(stream, s, f)
            .store(k, f);
        k.load(f);
        f.instruction(&Instruction::I32Const(GcI32Constant::encode(state)));
        f.instruction(&Instruction::I32Eq);
        s.release_i32_local(k, f);
    }
    fn stream_new_promise(
        &mut self,
        f: &mut Function,
    ) -> Result<GcLocal<PromiseObject>, EmitError> {
        let s = self.runtime_schema();
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let p = self.emit_alloc_promise_in_realm(&realm, f)?;
        realm.clear(f);
        Ok(p)
    }
    fn stream_this<T: JavaScriptReference + GcStructHeapType>(
        &mut self,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<T>, EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let value = self.emit_binary_require_ref::<T>(
            &receiver,
            RuntimeErrorMessage::STREAM_RECEIVER,
            output,
            exit,
            f,
        )?;
        receiver.clear(f);
        Ok(value)
    }
    fn stream_controller(
        &self,
        stream: &GcLocal<ReadableStreamObject>,
        f: &mut Function,
    ) -> GcLocal<ReadableByteStreamControllerObject> {
        let s = self.runtime_schema();
        s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamObjectSchema::CONTROLLER)
                .read(stream, s, f)
                .reference()
                .require_non_null(f),
            f,
        )
    }
    fn stream_callback(
        &mut self,
        controller: &GcLocal<ReadableByteStreamControllerObject>,
        builtin: StandardBuiltinId,
        f: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let s = self.runtime_schema();
        let realm = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::REALM)
                .read(controller, s, f)
                .reference(),
            f,
        );
        let materialization =
            self.emit_realm_function_materialization_context_from_realm(&realm, f);
        let capture = s
            .reserve_gc_local::<BuiltinClosureCapture, Nullable>(f)
            .initialize(
                s.struct_type::<BuiltinClosureCapture>()
                    .publish(
                        BuiltinClosurePayload::ReadableStreamController(controller),
                        s,
                        f,
                    )
                    .nullable(),
                f,
            );
        let meta = self
            .functions
            .get(&builtin.function_id())
            .cloned()
            .ok_or_else(|| EmitError::unsupported("missing stream continuation"))?;
        let callable = s.reserve_gc_local(f).initialize(
            self.emit_function_value_payload_in_realm_with_capture(
                &meta,
                &materialization,
                &capture,
                f,
            )?,
            f,
        );
        let value = s.reserve_value_local(f);
        value.set_reference(&callable, s, f);
        callable.clear(f);
        capture.clear(f);
        self.release_realm_function_materialization_context(materialization, f);
        realm.clear(f);
        Ok(value)
    }
    fn stream_capture(&self, f: &mut Function) -> GcLocal<ReadableByteStreamControllerObject> {
        let s = self.runtime_schema();
        let context = self
            .body_entry_locals()
            .expect("native stream continuation")
            .function_context()
            .expect("stream continuation context");
        let capture = s.reserve_gc_local(f).initialize(
            s.field(FunctionContextSchema::BUILTIN_CAPTURE)
                .read(context, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let kind = s.reserve_i32_local(f);
        s.field(BuiltinClosureCaptureSchema::KIND)
            .read(&capture, s, f)
            .store(kind, f);
        kind.load(f);
        f.instruction(&Instruction::I32Const(GcI32Constant::encode(
            BuiltinClosureCaptureKind::ReadableStreamController,
        )));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        s.release_i32_local(kind, f);
        let controller = s.reserve_gc_local(f).initialize(
            s.field(BuiltinClosureCaptureSchema::READABLE_STREAM_CONTROLLER)
                .read(&capture, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        capture.clear(f);
        controller
    }
    fn stream_call_pull(
        &mut self,
        controller: &GcLocal<ReadableByteStreamControllerObject>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let callback =
            self.stream_callback(controller, StandardBuiltinId::ReadableStreamPullIfNeeded, f)?;
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        let argv = self.emit_pre_evaluated_arg_vector(&[], f);
        let pending = s.reserve_completion(f);
        self.emit_function_or_proxy_call_with_argv(&callback, &undefined, &argv, &pending, f)?;
        pending.clear(f);
        argv.clear(f);
        undefined.clear(f);
        callback.clear(f);
        Ok(())
    }
    fn stream_observe(
        &mut self,
        controller: &GcLocal<ReadableByteStreamControllerObject>,
        value: &ValueLocals,
        start: bool,
        output: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let promise = self.stream_new_promise(f)?;
        self.emit_resolve_promise_record(&promise, value, f)?;
        self.stream_observe_promise(controller, &promise, start, f)?;
        promise.clear(f);
        output.initialize(f);
        Ok(())
    }
    fn stream_observe_promise(
        &mut self,
        controller: &GcLocal<ReadableByteStreamControllerObject>,
        promise: &GcLocal<PromiseObject>,
        start: bool,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let fulfilled = self.stream_callback(
            controller,
            if start {
                StandardBuiltinId::ReadableStreamStartFulfilled
            } else {
                StandardBuiltinId::ReadableStreamPullFulfilled
            },
            f,
        )?;
        let rejected = self.stream_callback(
            controller,
            if start {
                StandardBuiltinId::ReadableStreamStartRejected
            } else {
                StandardBuiltinId::ReadableStreamPullRejected
            },
            f,
        )?;
        self.emit_perform_promise_then_without_capability(promise, &fulfilled, &rejected, f)?;
        rejected.clear(f);
        fulfilled.clear(f);
        Ok(())
    }
    /// Capture the callback Realm before invocation: a callable Proxy can revoke
    /// itself while running. The undefined default algorithm uses this Realm.
    fn stream_callback_realm(
        &mut self,
        callback: &ValueLocals,
        f: &mut Function,
    ) -> Result<GcLocal<RealmRecord>, EmitError> {
        let s = self.runtime_schema();
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        self.stream_is_undefined(callback, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let obtained = self.emit_get_function_realm(callback, f);
        let resolved = self.emit_route_function_realm_result(
            obtained,
            crate::functions::FunctionRealmRevokedRoute::UseCurrentRealm { realm: &realm },
            f,
        )?;
        realm.replace(resolved.realm().load(s, f), f);
        self.release_resolved_function_realm_local(resolved, f);
        self.stream_end_if(f);
        Ok(realm)
    }
    /// Web-IDL Promise<T> always creates a fresh capability and resolves it with
    /// the JS result. Throwing callbacks first create a rejected Promise and
    /// pass it through that same conversion; all reaction timing is preserved.
    fn stream_callback_promise(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        completion: &CompletionLocals,
        f: &mut Function,
    ) -> Result<GcLocal<PromiseObject>, EmitError> {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        value.copy_from(completion.value(), f);
        completion.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let rejected = self.emit_alloc_promise_in_realm(realm, f)?;
        self.emit_settle_promise_record(&rejected, PromiseSettlement::Reject, &value, f)?;
        value.set_reference(&rejected, s, f);
        rejected.clear(f);
        self.stream_end_if(f);
        let promise = self.emit_alloc_promise_in_realm(realm, f)?;
        self.emit_resolve_promise_record(&promise, &value, f)?;
        value.clear(f);
        Ok(promise)
    }
}
