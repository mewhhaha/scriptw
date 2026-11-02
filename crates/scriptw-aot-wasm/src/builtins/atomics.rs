use super::super::*;
use super::binary_data::BufferAccess;
use super::data_view_access::DataViewElement;
use crate::gc_types::*;
use scriptw_runtime::AgentHostOperation;
mod wait_async_result;

pub(crate) const ATOMICS_PUBLICATION_ORDER: [StandardBuiltinId; 14] = [
    StandardBuiltinId::AtomicsAdd,
    StandardBuiltinId::AtomicsAnd,
    StandardBuiltinId::AtomicsCompareExchange,
    StandardBuiltinId::AtomicsExchange,
    StandardBuiltinId::AtomicsLoad,
    StandardBuiltinId::AtomicsNotify,
    StandardBuiltinId::AtomicsOr,
    StandardBuiltinId::AtomicsPause,
    StandardBuiltinId::AtomicsStore,
    StandardBuiltinId::AtomicsSub,
    StandardBuiltinId::AtomicsWait,
    StandardBuiltinId::AtomicsWaitAsync,
    StandardBuiltinId::AtomicsXor,
    StandardBuiltinId::AtomicsIsLockFree,
];

#[derive(Clone, Copy)]
enum AtomicsIntegerOperation {
    Load,
    Store,
    Add,
    Sub,
    And,
    Or,
    Xor,
    Exchange,
    CompareExchange,
}
impl AtomicsIntegerOperation {
    fn writes(self) -> bool {
        !matches!(self, Self::Load)
    }
    fn argument_count(self) -> u8 {
        match self {
            Self::Load => 0,
            Self::CompareExchange => 2,
            Self::Store
            | Self::Add
            | Self::Sub
            | Self::And
            | Self::Or
            | Self::Xor
            | Self::Exchange => 1,
        }
    }
    fn receiver_error(self) -> RuntimeErrorMessage {
        match self {
            Self::Load => RuntimeErrorMessage::ATOMICS_LOAD_REQUIRES_AN_INTEGER_TYPED_ARRAY,
            Self::Store => RuntimeErrorMessage::ATOMICS_STORE_REQUIRES_AN_INTEGER_TYPED_ARRAY,
            Self::Add => RuntimeErrorMessage::ATOMICS_ADD_REQUIRES_AN_INTEGER_TYPED_ARRAY,
            Self::Sub => RuntimeErrorMessage::ATOMICS_SUB_REQUIRES_AN_INTEGER_TYPED_ARRAY,
            Self::And => RuntimeErrorMessage::ATOMICS_AND_REQUIRES_AN_INTEGER_TYPED_ARRAY,
            Self::Or => RuntimeErrorMessage::ATOMICS_OR_REQUIRES_AN_INTEGER_TYPED_ARRAY,
            Self::Xor => RuntimeErrorMessage::ATOMICS_XOR_REQUIRES_AN_INTEGER_TYPED_ARRAY,
            Self::Exchange => RuntimeErrorMessage::ATOMICS_EXCHANGE_REQUIRES_AN_INTEGER_TYPED_ARRAY,
            Self::CompareExchange => {
                RuntimeErrorMessage::ATOMICS_COMPAREEXCHANGE_REQUIRES_AN_INTEGER_TYPED_ARRAY
            }
        }
    }
    fn index_error(self) -> RuntimeErrorMessage {
        match self {
            Self::Load => RuntimeErrorMessage::ATOMICS_LOAD_INDEX_OUT_OF_RANGE,
            Self::Store => RuntimeErrorMessage::ATOMICS_STORE_INDEX_OUT_OF_RANGE,
            Self::Add => RuntimeErrorMessage::ATOMICS_ADD_INDEX_OUT_OF_RANGE,
            Self::Sub => RuntimeErrorMessage::ATOMICS_SUB_INDEX_OUT_OF_RANGE,
            Self::And => RuntimeErrorMessage::ATOMICS_AND_INDEX_OUT_OF_RANGE,
            Self::Or => RuntimeErrorMessage::ATOMICS_OR_INDEX_OUT_OF_RANGE,
            Self::Xor => RuntimeErrorMessage::ATOMICS_XOR_INDEX_OUT_OF_RANGE,
            Self::Exchange => RuntimeErrorMessage::ATOMICS_EXCHANGE_INDEX_OUT_OF_RANGE,
            Self::CompareExchange => {
                RuntimeErrorMessage::ATOMICS_COMPAREEXCHANGE_INDEX_OUT_OF_RANGE
            }
        }
    }
}
#[derive(Clone, Copy)]
enum AtomicsWaitOutcome {
    Ok,
    NotEqual,
    TimedOut,
}
impl AtomicsWaitOutcome {
    fn spelling(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::NotEqual => "not-equal",
            Self::TimedOut => "timed-out",
        }
    }
}
#[derive(Clone, Copy)]
enum AtomicsWaitAsyncTimeoutCheckpointMode {
    Drain,
    Poll,
}

/// Only early concrete brand/kind/bounds admission creates this retained state.
#[must_use]
struct PendingAtomicAccess {
    array: GcLocal<TypedArrayObject>,
    kind: I32Local,
    index: I64Local,
}
/// Backing storage is acquired anew after all index/value/count/timeout hooks.
#[must_use]
struct RevalidatedAtomicAccess {
    backing: BufferAccess,
    offset: I64Local,
    kind: I32Local,
}
impl PendingAtomicAccess {
    fn clear(self, s: &RuntimeSchema, f: &mut Function) {
        s.release_i64_local(self.index, f);
        s.release_i32_local(self.kind, f);
        self.array.clear(f);
    }
}
impl RevalidatedAtomicAccess {
    fn clear(self, s: &RuntimeSchema, f: &mut Function) {
        s.release_i32_local(self.kind, f);
        s.release_i64_local(self.offset, f);
        self.backing.clear(s, f);
    }
}

impl FunctionBuilder<'_> {
    fn emit_atomics_is_lock_free(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let input = s.reserve_value_local(f);
        let out = s.reserve_completion(f);
        let integer = s.reserve_i64_local(f);
        let flag = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        self.emit_value_to_number_payload(&input, &out, f)?;
        out.kind().load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            out.value().scalar(),
            integer,
            f,
        );
        f.instruction(&Instruction::I32Const(0));
        for size in [1.0, 2.0, 4.0, 8.0] {
            integer.load(f);
            f.instruction(&Instruction::F64ReinterpretI64);
            f.instruction(&Instruction::F64Const(Ieee64::from(size)));
            f.instruction(&Instruction::F64Eq);
            f.instruction(&Instruction::I32Or);
        }
        flag.store(f);
        out.value().set_boolean(flag, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&out, f);
        s.release_i32_local(flag, f);
        s.release_i64_local(integer, f);
        out.clear(f);
        input.clear(f);
        Ok(())
    }
    fn emit_atomics_pause(&mut self, f: &mut Function) -> Result<(), EmitError> {
        // The current normative operation takes no semantic operands. All source
        // arguments have already been evaluated by the shared invocation owner.
        self.completion().initialize(f);
        Ok(())
    }

    fn emit_atomics_notify(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let out = s.reserve_completion(f);
        out.initialize(f);
        let pending = s.reserve_completion(f);
        let input = s.reserve_value_local(f);
        let index_arg = s.reserve_value_local(f);
        let count_arg = s.reserve_value_local(f);
        let count = s.reserve_i64_local(f);
        let notified = s.reserve_i64_local(f);
        let width = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        self.emit_builtin_arg_to_value(1, &index_arg, f);
        self.emit_builtin_arg_to_value(2, &count_arg, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let prepared = self.emit_atomics_prepare(
            &input,
            &index_arg,
            true,
            false,
            false,
            RuntimeErrorMessage::ATOMICS_NOTIFY_REQUIRES_AN_INT32ARRAY_OR_BIGINT64ARRAY,
            RuntimeErrorMessage::ATOMICS_NOTIFY_INDEX_OUT_OF_RANGE,
            &pending,
            &out,
            exit,
            f,
        )?;
        count_arg.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(i64::MAX));
        count.store(f);
        f.instruction(&Instruction::Else);
        self.emit_value_to_number_payload(&count_arg, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            count,
            f,
        );
        count.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Max);
        f.instruction(&Instruction::I64TruncSatF64S);
        count.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Notify on a non-shared buffer returns zero after count coercion, without
        // introducing the RMW revalidation policy for a detached ordinary buffer.
        let view = s.reserve_gc_local(f).initialize(
            s.field(TypedArrayObjectSchema::VIEW)
                .read(&prepared.array, s, f)
                .reference(),
            f,
        );
        let owner = s.reserve_gc_local(f).initialize(
            s.field(BufferViewSchema::BUFFER)
                .read(&view, s, f)
                .reference(),
            f,
        );
        let owner_kind = s.reserve_i32_local(f);
        s.field(BufferOwnerSchema::KIND)
            .read(&owner, s, f)
            .store(owner_kind, f);
        f.instruction(&Instruction::I64Const(0));
        notified.store(f);
        owner_kind.load(f);
        f.instruction(&Instruction::I32Const(
            BufferOwnerKind::SharedArrayBuffer.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let access = self.emit_atomics_revalidate(
            &prepared,
            &pending,
            &out,
            exit,
            RuntimeErrorMessage::ATOMICS_NOTIFY_INDEX_OUT_OF_RANGE,
            f,
        )?;
        prepared.kind.load(f);
        f.instruction(&Instruction::I32Const(
            TypedArrayElementKind::BigInt64.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(8));
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::End);
        width.store(f);
        let notify = self
            .functions
            .gc_host_imports()
            .get(GcHostImport::NotifyAsyncWaiters)
            .ok_or_else(|| EmitError::unsupported("shared notify resource import is absent"))?;
        let _ = s
            .field(HostResourceSchema::RESOURCE)
            .read(&access.backing.resource, s, f);
        access.offset.load(f);
        width.load(f);
        count.load(f);
        notify.emit_call_instruction(f);
        notified.store(f);
        access.clear(s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        notified.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        notified.store(f);
        out.value().set_number(notified, f);
        s.release_i32_local(owner_kind, f);
        owner.clear(f);
        view.clear(f);
        prepared.clear(s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&out, f);
        s.release_i32_local(width, f);
        s.release_i64_local(notified, f);
        s.release_i64_local(count, f);
        count_arg.clear(f);
        index_arg.clear(f);
        input.clear(f);
        pending.clear(f);
        out.clear(f);
        Ok(())
    }

    fn emit_atomics_wait_outcome(
        &mut self,
        outcome: AtomicsWaitOutcome,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let string = s.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference(outcome.spelling(), f)?,
            f,
        );
        value.set_reference(&string, s, f);
        string.clear(f);
        Ok(())
    }
    fn emit_atomics_wait(&mut self, asynchronous: bool, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let out = s.reserve_completion(f);
        out.initialize(f);
        let pending = s.reserve_completion(f);
        let input = s.reserve_value_local(f);
        let index_arg = s.reserve_value_local(f);
        let expected = s.reserve_value_local(f);
        let timeout = s.reserve_value_local(f);
        let outcome = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        self.emit_builtin_arg_to_value(1, &index_arg, f);
        self.emit_builtin_arg_to_value(2, &expected, f);
        self.emit_builtin_arg_to_value(3, &timeout, f);
        let word = s.reserve_i64_local(f);
        let current = s.reserve_i64_local(f);
        let nanos = s.reserve_i64_local(f);
        let deadline = s.reserve_i64_local(f);
        let host_id = s.reserve_i64_local(f);
        let width = s.reserve_i32_local(f);
        let status = s.reserve_i32_local(f);
        let receiver_error = if asynchronous {
            RuntimeErrorMessage::ATOMICS_WAITASYNC_REQUIRES_A_SHARED_INT32ARRAY_OR_BIGINT64ARRAY
        } else {
            RuntimeErrorMessage::ATOMICS_WAIT_REQUIRES_A_SHARED_INT32ARRAY_OR_BIGINT64ARRAY
        };
        let index_error = if asynchronous {
            RuntimeErrorMessage::ATOMICS_WAITASYNC_INDEX_OUT_OF_RANGE
        } else {
            RuntimeErrorMessage::ATOMICS_WAIT_INDEX_OUT_OF_RANGE
        };
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let prepared = self.emit_atomics_prepare(
            &input,
            &index_arg,
            true,
            false,
            true,
            receiver_error,
            index_error,
            &pending,
            &out,
            exit,
            f,
        )?;
        self.emit_atomics_value(prepared.kind, &expected, word, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        self.emit_value_to_number_payload(&timeout, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        // Undefined/NaN timeout is +infinity; otherwise max(number,0), in ns.
        pending.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        pending.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Ne);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(-1));
        nanos.store(f);
        f.instruction(&Instruction::Else);
        pending.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
        f.instruction(&Instruction::F64Eq);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(-1));
        nanos.store(f);
        f.instruction(&Instruction::Else);
        pending.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Max);
        f.instruction(&Instruction::F64Const(Ieee64::from(1_000_000.0)));
        f.instruction(&Instruction::F64Mul);
        f.instruction(&Instruction::I64TruncSatF64S);
        nanos.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        if !asynchronous {
            f.instruction(&Instruction::Call(
                HOST_AGENT_CAN_SUSPEND_IMPORT_FUNCTION_INDEX,
            ));
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_binary_type_error(
                RuntimeErrorMessage::ATOMICS_WAIT_CANNOT_SUSPEND_THE_CURRENT_AGENT,
                &out,
                exit,
                f,
            )?;
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        let access =
            self.emit_atomics_revalidate(&prepared, &pending, &out, exit, index_error, f)?;
        prepared.kind.load(f);
        f.instruction(&Instruction::I32Const(
            TypedArrayElementKind::BigInt64.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(8));
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::End);
        width.store(f);
        if asynchronous {
            let immediate = self.open_frame(ControlFrameKind::Block, f);
            // Zero timeout still tests equality first. Positive registration performs
            // comparison and queue publication together in the native critical section.
            nanos.load(f);
            f.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_atomics_raw_operation(
                &access,
                AtomicsIntegerOperation::Load,
                word,
                word,
                current,
                f,
            )?;
            width.load(f);
            f.instruction(&Instruction::I32Const(4));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            word.load(f);
            f.instruction(&Instruction::I64Const(0xffff_ffff));
            f.instruction(&Instruction::I64And);
            word.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            current.load(f);
            word.load(f);
            f.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_atomics_wait_outcome(AtomicsWaitOutcome::TimedOut, &outcome, f)?;
            f.instruction(&Instruction::Else);
            self.emit_atomics_wait_outcome(AtomicsWaitOutcome::NotEqual, &outcome, f)?;
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            self.emit_atomics_wait_async_object(false, &outcome, &out, f)?;
            self.emit_branch_to_target(immediate, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            nanos.load(f);
            f.instruction(&Instruction::I64Const(0));
            f.instruction(&Instruction::I64LtS);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I64Const(i64::MAX));
            deadline.store(f);
            f.instruction(&Instruction::Else);
            f.instruction(&Instruction::Call(
                self.functions
                    .monotonic_clock_nanos_import_function_index()
                    .expect("waitAsync monotonic clock"),
            ));
            deadline.store(f);
            deadline.load(f);
            f.instruction(&Instruction::I64Const(i64::MAX));
            nanos.load(f);
            f.instruction(&Instruction::I64Sub);
            f.instruction(&Instruction::I64GtU);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I64Const(i64::MAX));
            deadline.store(f);
            f.instruction(&Instruction::Else);
            deadline.load(f);
            nanos.load(f);
            f.instruction(&Instruction::I64Add);
            deadline.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            // Native notify and the GC timeout checkpoint arbitrate against
            // the same deadline, including while this agent is blocked.
            let register = self
                .functions
                .gc_host_imports()
                .get(GcHostImport::RegisterAsyncWaiter)
                .expect("waitAsync register import");
            let _ = s
                .field(HostResourceSchema::RESOURCE)
                .read(&access.backing.resource, s, f);
            access.offset.load(f);
            width.load(f);
            word.load(f);
            deadline.load(f);
            register.emit_call_instruction(f);
            host_id.store(f);
            host_id.load(f);
            f.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_atomics_wait_outcome(AtomicsWaitOutcome::NotEqual, &outcome, f)?;
            self.emit_atomics_wait_async_object(false, &outcome, &out, f)?;
            self.emit_branch_to_target(immediate, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            let buffer = s.reserve_gc_local(f).initialize(
                s.field(BufferOwnerSchema::SHARED_ARRAY_BUFFER)
                    .read(&access.backing.owner, s, f)
                    .reference()
                    .require_non_null(f),
                f,
            );
            self.emit_atomics_wait_async_promise(
                &buffer,
                access.offset,
                deadline,
                host_id,
                &out,
                f,
            )?;
            buffer.clear(f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        } else {
            let wait = self
                .functions
                .gc_host_imports()
                .get(GcHostImport::SharedBufferWait)
                .expect("sync wait resource import");
            let _ = s
                .field(HostResourceSchema::RESOURCE)
                .read(&access.backing.resource, s, f);
            access.offset.load(f);
            width.load(f);
            word.load(f);
            nanos.load(f);
            wait.emit_call_instruction(f);
            status.store(f);
            for (code, result) in [
                (0, AtomicsWaitOutcome::Ok),
                (1, AtomicsWaitOutcome::NotEqual),
                (2, AtomicsWaitOutcome::TimedOut),
            ] {
                status.load(f);
                f.instruction(&Instruction::I32Const(code));
                f.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_atomics_wait_outcome(result, out.value(), f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        access.clear(s, f);
        prepared.clear(s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&out, f);
        s.release_i32_local(status, f);
        s.release_i32_local(width, f);
        s.release_i64_local(host_id, f);
        s.release_i64_local(deadline, f);
        s.release_i64_local(nanos, f);
        s.release_i64_local(current, f);
        s.release_i64_local(word, f);
        outcome.clear(f);
        timeout.clear(f);
        expected.clear(f);
        index_arg.clear(f);
        input.clear(f);
        pending.clear(f);
        out.clear(f);
        Ok(())
    }
    pub(super) fn emit_atomics_load_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_atomics_integer_operation(AtomicsIntegerOperation::Load, f)
    }
    pub(super) fn emit_atomics_store_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_atomics_integer_operation(AtomicsIntegerOperation::Store, f)
    }
    pub(super) fn emit_atomics_add_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_atomics_integer_operation(AtomicsIntegerOperation::Add, f)
    }
    pub(super) fn emit_atomics_sub_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_atomics_integer_operation(AtomicsIntegerOperation::Sub, f)
    }
    pub(super) fn emit_atomics_and_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_atomics_integer_operation(AtomicsIntegerOperation::And, f)
    }
    pub(super) fn emit_atomics_or_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_atomics_integer_operation(AtomicsIntegerOperation::Or, f)
    }
    pub(super) fn emit_atomics_xor_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_atomics_integer_operation(AtomicsIntegerOperation::Xor, f)
    }
    pub(super) fn emit_atomics_exchange_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_atomics_integer_operation(AtomicsIntegerOperation::Exchange, f)
    }
    pub(super) fn emit_atomics_compare_exchange_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_atomics_integer_operation(AtomicsIntegerOperation::CompareExchange, f)
    }
    pub(super) fn emit_atomics_is_lock_free_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_atomics_is_lock_free(f)
    }
    pub(super) fn emit_atomics_pause_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_atomics_pause(f)
    }
    pub(super) fn emit_atomics_notify_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_atomics_notify(f)
    }
    pub(super) fn emit_atomics_wait_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_atomics_wait(false, f)
    }
    pub(super) fn emit_atomics_wait_async_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_atomics_wait(true, f)
    }

    fn emit_atomics_prepare(
        &mut self,
        input: &ValueLocals,
        index_arg: &ValueLocals,
        waitable: bool,
        write: bool,
        require_shared: bool,
        receiver_error: RuntimeErrorMessage,
        index_error: RuntimeErrorMessage,
        pending: &CompletionLocals,
        out: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<PendingAtomicAccess, EmitError> {
        let s = self.runtime_schema();
        let array =
            self.emit_binary_require_ref::<TypedArrayObject>(input, receiver_error, out, exit, f)?;
        let length = s.reserve_i64_local(f);
        let kind = s.reserve_i32_local(f);
        let index = s.reserve_i64_local(f);
        if write {
            self.emit_validate_typed_array_write_view(&array, length, pending, f)?;
        } else {
            self.emit_validate_typed_array_view(&array, length, pending, f)?;
        }
        self.emit_binary_abrupt_exit(pending, out, exit, f);
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&array, s, f)
            .store(kind, f);
        f.instruction(&Instruction::I32Const(0));
        for element in TypedArrayElementKind::ALL {
            let admitted = if waitable {
                matches!(
                    element,
                    TypedArrayElementKind::Int32 | TypedArrayElementKind::BigInt64
                )
            } else {
                element.is_atomics_integer()
            };
            if admitted {
                kind.load(f);
                f.instruction(&Instruction::I32Const(element.encode()));
                f.instruction(&Instruction::I32Eq);
                f.instruction(&Instruction::I32Or);
            }
        }
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(receiver_error, out, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        if require_shared {
            let view = s.reserve_gc_local(f).initialize(
                s.field(TypedArrayObjectSchema::VIEW)
                    .read(&array, s, f)
                    .reference(),
                f,
            );
            let owner = s.reserve_gc_local(f).initialize(
                s.field(BufferViewSchema::BUFFER)
                    .read(&view, s, f)
                    .reference(),
                f,
            );
            let buffer_kind = s.reserve_i32_local(f);
            s.field(BufferOwnerSchema::KIND)
                .read(&owner, s, f)
                .store(buffer_kind, f);
            buffer_kind.load(f);
            f.instruction(&Instruction::I32Const(
                BufferOwnerKind::SharedArrayBuffer.encode(),
            ));
            f.instruction(&Instruction::I32Ne);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_binary_type_error(receiver_error, out, exit, f)?;
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            s.release_i32_local(buffer_kind, f);
            owner.clear(f);
            view.clear(f);
        }
        self.emit_to_index_i64_from_value_locals(index_arg, index, index_error, pending, f)?;
        self.emit_binary_abrupt_exit(pending, out, exit, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(index_error, out, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(length, f);
        Ok(PendingAtomicAccess { array, kind, index })
    }
    fn emit_atomics_revalidate(
        &mut self,
        access: &PendingAtomicAccess,
        pending: &CompletionLocals,
        out: &CompletionLocals,
        exit: ControlTarget,
        index_error: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<RevalidatedAtomicAccess, EmitError> {
        let s = self.runtime_schema();
        let length = s.reserve_i64_local(f);
        self.emit_validate_typed_array_view(&access.array, length, pending, f)?;
        self.emit_binary_abrupt_exit(pending, out, exit, f);
        let view = s.reserve_gc_local(f).initialize(
            s.field(TypedArrayObjectSchema::VIEW)
                .read(&access.array, s, f)
                .reference(),
            f,
        );
        let owner = s.reserve_gc_local(f).initialize(
            s.field(BufferViewSchema::BUFFER)
                .read(&view, s, f)
                .reference(),
            f,
        );
        let backing = self.emit_binary_buffer_access(&owner, f);
        let offset = s.reserve_i64_local(f);
        s.field(BufferViewSchema::BYTE_OFFSET)
            .read(&view, s, f)
            .store_i64(offset, f);
        for element in TypedArrayElementKind::ALL {
            if element.is_atomics_integer() {
                access.kind.load(f);
                f.instruction(&Instruction::I32Const(element.encode()));
                f.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, f);
                offset.load(f);
                access.index.load(f);
                f.instruction(&Instruction::I64Const(element.bytes_per_element() as i64));
                f.instruction(&Instruction::I64Mul);
                f.instruction(&Instruction::I64Add);
                offset.store(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        offset.load(f);
        backing.length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(index_error, out, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let kind = s.reserve_i32_local(f);
        access.kind.load(f);
        kind.store(f);
        owner.clear(f);
        view.clear(f);
        s.release_i64_local(length, f);
        Ok(RevalidatedAtomicAccess {
            backing,
            offset,
            kind,
        })
    }
    fn emit_atomics_value(
        &mut self,
        kind: I32Local,
        input: &ValueLocals,
        word: I64Local,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        kind.load(f);
        f.instruction(&Instruction::I32Const(
            TypedArrayElementKind::BigInt64.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        kind.load(f);
        f.instruction(&Instruction::I32Const(
            TypedArrayElementKind::BigUint64.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_to_bigint_u64_word_from_value_locals(input, word, pending, f)?;
        f.instruction(&Instruction::Else);
        self.emit_value_to_number_payload(input, pending, f)?;
        pending.kind().load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            pending.value().scalar(),
            f,
        );
        pending.value().set_number(pending.value().scalar(), f);
        self.emit_to_uint32_i64_from_number_payload(pending.value().scalar(), word, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_atomics_word_to_value(
        &mut self,
        kind: I32Local,
        word: I64Local,
        value: &ValueLocals,
        f: &mut Function,
    ) {
        for (element, data) in [
            (TypedArrayElementKind::Int8, DataViewElement::Int8),
            (TypedArrayElementKind::Uint8, DataViewElement::Uint8),
            (TypedArrayElementKind::Int16, DataViewElement::Int16),
            (TypedArrayElementKind::Uint16, DataViewElement::Uint16),
            (TypedArrayElementKind::Int32, DataViewElement::Int32),
            (TypedArrayElementKind::Uint32, DataViewElement::Uint32),
            (TypedArrayElementKind::BigInt64, DataViewElement::BigInt64),
            (TypedArrayElementKind::BigUint64, DataViewElement::BigUint64),
        ] {
            kind.load(f);
            f.instruction(&Instruction::I32Const(element.encode()));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_data_view_word_to_value(data, word, value, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
    }
    fn emit_atomics_raw_operation(
        &mut self,
        access: &RevalidatedAtomicAccess,
        op: AtomicsIntegerOperation,
        value: I64Local,
        replacement: I64Local,
        old: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let address = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let cursor = s.reserve_i64_local(f);
        let byte = s.reserve_i32_local(f);
        let next_word = s.reserve_i64_local(f);
        for element in TypedArrayElementKind::ALL {
            if !element.is_atomics_integer() {
                continue;
            }
            let width = element.bytes_per_element();
            access.kind.load(f);
            f.instruction(&Instruction::I32Const(element.encode()));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            access.backing.shared.load(f);
            self.open_frame(ControlFrameKind::If, f);
            if let Some(import) = self
                .functions
                .gc_host_imports()
                .get(GcHostImport::SharedBufferBase)
            {
                let _ = s
                    .field(HostResourceSchema::RESOURCE)
                    .read(&access.backing.resource, s, f);
                import.emit_call_instruction(f);
                access.offset.load(f);
                f.instruction(&Instruction::I64Add);
                address.store(f);
                address.load(f);
                f.instruction(&Instruction::I32WrapI64);
                match op {
                    AtomicsIntegerOperation::Load => {
                        f.instruction(&match width {
                            1 => Instruction::I64AtomicLoad8U(Self::shared_memarg8(0)),
                            2 => Instruction::I64AtomicLoad16U(Self::shared_memarg16(0)),
                            4 => Instruction::I64AtomicLoad32U(Self::shared_memarg32(0)),
                            8 => Instruction::I64AtomicLoad(Self::shared_memarg64(0)),
                            _ => unreachable!("integer width"),
                        });
                        old.store(f);
                    }
                    AtomicsIntegerOperation::Store => {
                        value.load(f);
                        f.instruction(&match width {
                            1 => Instruction::I64AtomicStore8(Self::shared_memarg8(0)),
                            2 => Instruction::I64AtomicStore16(Self::shared_memarg16(0)),
                            4 => Instruction::I64AtomicStore32(Self::shared_memarg32(0)),
                            8 => Instruction::I64AtomicStore(Self::shared_memarg64(0)),
                            _ => unreachable!("integer width"),
                        });
                        f.instruction(&Instruction::I64Const(0));
                        old.store(f);
                    }
                    AtomicsIntegerOperation::Add => {
                        value.load(f);
                        f.instruction(&match width {
                            1 => Instruction::I64AtomicRmw8AddU(Self::shared_memarg8(0)),
                            2 => Instruction::I64AtomicRmw16AddU(Self::shared_memarg16(0)),
                            4 => Instruction::I64AtomicRmw32AddU(Self::shared_memarg32(0)),
                            8 => Instruction::I64AtomicRmwAdd(Self::shared_memarg64(0)),
                            _ => unreachable!("integer width"),
                        });
                        old.store(f);
                    }
                    AtomicsIntegerOperation::Sub => {
                        value.load(f);
                        f.instruction(&match width {
                            1 => Instruction::I64AtomicRmw8SubU(Self::shared_memarg8(0)),
                            2 => Instruction::I64AtomicRmw16SubU(Self::shared_memarg16(0)),
                            4 => Instruction::I64AtomicRmw32SubU(Self::shared_memarg32(0)),
                            8 => Instruction::I64AtomicRmwSub(Self::shared_memarg64(0)),
                            _ => unreachable!("integer width"),
                        });
                        old.store(f);
                    }
                    AtomicsIntegerOperation::And => {
                        value.load(f);
                        f.instruction(&match width {
                            1 => Instruction::I64AtomicRmw8AndU(Self::shared_memarg8(0)),
                            2 => Instruction::I64AtomicRmw16AndU(Self::shared_memarg16(0)),
                            4 => Instruction::I64AtomicRmw32AndU(Self::shared_memarg32(0)),
                            8 => Instruction::I64AtomicRmwAnd(Self::shared_memarg64(0)),
                            _ => unreachable!("integer width"),
                        });
                        old.store(f);
                    }
                    AtomicsIntegerOperation::Or => {
                        value.load(f);
                        f.instruction(&match width {
                            1 => Instruction::I64AtomicRmw8OrU(Self::shared_memarg8(0)),
                            2 => Instruction::I64AtomicRmw16OrU(Self::shared_memarg16(0)),
                            4 => Instruction::I64AtomicRmw32OrU(Self::shared_memarg32(0)),
                            8 => Instruction::I64AtomicRmwOr(Self::shared_memarg64(0)),
                            _ => unreachable!("integer width"),
                        });
                        old.store(f);
                    }
                    AtomicsIntegerOperation::Xor => {
                        value.load(f);
                        f.instruction(&match width {
                            1 => Instruction::I64AtomicRmw8XorU(Self::shared_memarg8(0)),
                            2 => Instruction::I64AtomicRmw16XorU(Self::shared_memarg16(0)),
                            4 => Instruction::I64AtomicRmw32XorU(Self::shared_memarg32(0)),
                            8 => Instruction::I64AtomicRmwXor(Self::shared_memarg64(0)),
                            _ => unreachable!("integer width"),
                        });
                        old.store(f);
                    }
                    AtomicsIntegerOperation::Exchange => {
                        value.load(f);
                        f.instruction(&match width {
                            1 => Instruction::I64AtomicRmw8XchgU(Self::shared_memarg8(0)),
                            2 => Instruction::I64AtomicRmw16XchgU(Self::shared_memarg16(0)),
                            4 => Instruction::I64AtomicRmw32XchgU(Self::shared_memarg32(0)),
                            8 => Instruction::I64AtomicRmwXchg(Self::shared_memarg64(0)),
                            _ => unreachable!("integer width"),
                        });
                        old.store(f);
                    }
                    AtomicsIntegerOperation::CompareExchange => {
                        value.load(f);
                        replacement.load(f);
                        f.instruction(&match width {
                            1 => Instruction::I64AtomicRmw8CmpxchgU(Self::shared_memarg8(0)),
                            2 => Instruction::I64AtomicRmw16CmpxchgU(Self::shared_memarg16(0)),
                            4 => Instruction::I64AtomicRmw32CmpxchgU(Self::shared_memarg32(0)),
                            8 => Instruction::I64AtomicRmwCmpxchg(Self::shared_memarg64(0)),
                            _ => unreachable!("integer width"),
                        });
                        old.store(f);
                    }
                }
            } else {
                f.instruction(&Instruction::Unreachable);
            }
            f.instruction(&Instruction::Else);
            f.instruction(&Instruction::I64Const(0));
            old.store(f);
            f.instruction(&Instruction::I64Const(0));
            cursor.store(f);
            let loaded = self.open_frame(ControlFrameKind::Block, f);
            let next = self.open_frame(ControlFrameKind::Loop, f);
            cursor.load(f);
            f.instruction(&Instruction::I64Const(width as i64));
            f.instruction(&Instruction::I64GeU);
            self.emit_branch_if_to_target(loaded, f);
            access.offset.load(f);
            cursor.load(f);
            f.instruction(&Instruction::I64Add);
            index.store(f);
            access.backing.read_byte(index, byte, self, f)?;
            old.load(f);
            byte.load(f);
            f.instruction(&Instruction::I64ExtendI32U);
            cursor.load(f);
            f.instruction(&Instruction::I64Const(3));
            f.instruction(&Instruction::I64Shl);
            f.instruction(&Instruction::I64Shl);
            f.instruction(&Instruction::I64Or);
            old.store(f);
            self.emit_increment_local(cursor, 1, f);
            self.emit_branch_to_target(next, f);
            self.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            if op.writes() {
                match op {
                    AtomicsIntegerOperation::Load => unreachable!(),
                    AtomicsIntegerOperation::Store | AtomicsIntegerOperation::Exchange => {
                        value.load(f);
                        next_word.store(f);
                    }
                    AtomicsIntegerOperation::CompareExchange => {
                        replacement.load(f);
                        next_word.store(f);
                    }
                    AtomicsIntegerOperation::Add
                    | AtomicsIntegerOperation::Sub
                    | AtomicsIntegerOperation::And
                    | AtomicsIntegerOperation::Or
                    | AtomicsIntegerOperation::Xor => {
                        old.load(f);
                        value.load(f);
                        f.instruction(&match op {
                            AtomicsIntegerOperation::Add => Instruction::I64Add,
                            AtomicsIntegerOperation::Sub => Instruction::I64Sub,
                            AtomicsIntegerOperation::And => Instruction::I64And,
                            AtomicsIntegerOperation::Or => Instruction::I64Or,
                            AtomicsIntegerOperation::Xor => Instruction::I64Xor,
                            _ => unreachable!(),
                        });
                        next_word.store(f);
                    }
                }
                if matches!(op, AtomicsIntegerOperation::CompareExchange) {
                    old.load(f);
                    value.load(f);
                    if width < 8 {
                        f.instruction(&Instruction::I64Const((1i64 << (width * 8)) - 1));
                        f.instruction(&Instruction::I64And);
                    }
                    f.instruction(&Instruction::I64Eq);
                    self.open_frame(ControlFrameKind::If, f);
                }
                for offset in 0..width {
                    access.offset.load(f);
                    f.instruction(&Instruction::I64Const(offset as i64));
                    f.instruction(&Instruction::I64Add);
                    index.store(f);
                    next_word.load(f);
                    f.instruction(&Instruction::I64Const((offset * 8) as i64));
                    f.instruction(&Instruction::I64ShrU);
                    f.instruction(&Instruction::I32WrapI64);
                    byte.store(f);
                    access.backing.write_byte(index, byte, self, f)?;
                }
                if matches!(op, AtomicsIntegerOperation::CompareExchange) {
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                }
            }
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        s.release_i64_local(next_word, f);
        s.release_i32_local(byte, f);
        s.release_i64_local(cursor, f);
        s.release_i64_local(index, f);
        s.release_i64_local(address, f);
        Ok(())
    }
    fn emit_atomics_integer_operation(
        &mut self,
        op: AtomicsIntegerOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let out = s.reserve_completion(f);
        out.initialize(f);
        let pending = s.reserve_completion(f);
        let input = s.reserve_value_local(f);
        let index = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let replacement = s.reserve_value_local(f);
        let store_result = s.reserve_value_local(f);
        let word = s.reserve_i64_local(f);
        let next_word = s.reserve_i64_local(f);
        let old = s.reserve_i64_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        self.emit_builtin_arg_to_value(1, &index, f);
        self.emit_builtin_arg_to_value(2, &value, f);
        self.emit_builtin_arg_to_value(3, &replacement, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let prepared = self.emit_atomics_prepare(
            &input,
            &index,
            false,
            op.writes(),
            false,
            op.receiver_error(),
            op.index_error(),
            &pending,
            &out,
            exit,
            f,
        )?;
        if op.argument_count() > 0 {
            self.emit_atomics_value(prepared.kind, &value, word, &pending, f)?;
            self.emit_binary_abrupt_exit(&pending, &out, exit, f);
            store_result.copy_from(pending.value(), f);
        }
        if op.argument_count() > 1 {
            self.emit_atomics_value(prepared.kind, &replacement, next_word, &pending, f)?;
            self.emit_binary_abrupt_exit(&pending, &out, exit, f);
        }
        let access =
            self.emit_atomics_revalidate(&prepared, &pending, &out, exit, op.index_error(), f)?;
        self.emit_atomics_raw_operation(&access, op, word, next_word, old, f)?;
        if matches!(op, AtomicsIntegerOperation::Store) {
            out.set_normal(&store_result, f);
        } else {
            self.emit_atomics_word_to_value(prepared.kind, old, out.value(), f);
        }
        access.clear(s, f);
        prepared.clear(s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&out, f);
        s.release_i64_local(old, f);
        s.release_i64_local(next_word, f);
        s.release_i64_local(word, f);
        store_result.clear(f);
        replacement.clear(f);
        value.clear(f);
        index.clear(f);
        input.clear(f);
        pending.clear(f);
        out.clear(f);
        Ok(())
    }
}
