use super::*;
use crate::bigint::BigIntHelperOp;
use crate::operations::BigIntNumberPolicy;

/// Every receiver policy and algorithm is selected exhaustively at emission.
pub(super) enum TypedArrayNativeMethod {
    Reverse,
    CopyWithin,
    ToReversed,
    Sort,
    ToSorted,
    With,
}
enum TypedArrayCopyMethod {
    Reversed,
    With,
}
enum TypedArraySortMethod {
    InPlace,
    Copy,
}

impl TypedArrayNativeMethod {
    fn receiver_error(&self) -> RuntimeErrorMessage {
        match self {
            Self::Reverse => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_REVERSE_REQUIRES_TYPEDARRAY,
            Self::CopyWithin => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_COPYWITHIN_REQUIRES_TYPEDARRAY
            }
            Self::ToReversed => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_TOREVERSED_REQUIRES_TYPEDARRAY
            }
            Self::Sort => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SORT_REQUIRES_TYPEDARRAY,
            Self::ToSorted => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_TOSORTED_REQUIRES_TYPEDARRAY
            }
            Self::With => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_WITH_REQUIRES_TYPEDARRAY,
        }
    }
    fn writes_receiver(&self) -> bool {
        match self {
            Self::Reverse | Self::CopyWithin | Self::Sort => true,
            Self::ToReversed | Self::ToSorted | Self::With => false,
        }
    }
}

/// Admission owns the concrete reference and the single initial length snapshot.
/// Callers cannot construct it from an unvalidated value or a raw payload.
#[must_use]
struct TypedArrayMethodReceiver {
    object: GcLocal<TypedArrayObject>,
    length: I64Local,
    kind: I32Local,
}
impl TypedArrayMethodReceiver {
    fn clear(self, schema: &RuntimeSchema, f: &mut Function) {
        self.object.clear(f);
        schema.release_i64_local(self.length, f);
        schema.release_i32_local(self.kind, f);
    }
}

impl FunctionBuilder<'_> {
    fn emit_typed_array_method_abrupt_exit(
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
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    fn emit_typed_array_method_receiver(
        &mut self,
        method: &TypedArrayNativeMethod,
        value: &ValueLocals,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<TypedArrayMethodReceiver, EmitError> {
        let s = self.runtime_schema();
        value.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<TypedArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(method.receiver_error(), pending, f)?;
        output.copy_from(pending, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let object = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<TypedArrayObject>(s, f), f);
        let length = s.reserve_i64_local(f);
        let kind = s.reserve_i32_local(f);
        // Write admission precedes every index/value coercion, including empty
        // immutable receivers. Read methods may copy an immutable receiver.
        if method.writes_receiver() {
            self.emit_validate_typed_array_write_view(&object, length, pending, f)?;
        } else {
            self.emit_validate_typed_array_view(&object, length, pending, f)?;
        }
        self.emit_typed_array_method_abrupt_exit(pending, output, exit, f);
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&object, s, f)
            .store(kind, f);
        Ok(TypedArrayMethodReceiver {
            object,
            length,
            kind,
        })
    }

    fn emit_typed_array_sort_comparator_admission(
        &mut self,
        comparator: &ValueLocals,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        comparator.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_callable_i32(comparator, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::VALUE_IS_NOT_CALLABLE,
            pending,
            f,
        )?;
        output.copy_from(pending, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_typed_array_clamped_index(
        &mut self,
        input: &ValueLocals,
        length: I64Local,
        index: I64Local,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let integer = s.reserve_i64_local(f);
        self.emit_value_to_number_payload(input, pending, f)?;
        self.emit_typed_array_method_abrupt_exit(pending, output, exit, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            integer,
            f,
        );
        integer.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Lt);
        f.instruction(&Instruction::If(BlockType::Result(ValType::F64)));
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        integer.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Add);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Max);
        f.instruction(&Instruction::Else);
        integer.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Min);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I64TruncF64U);
        index.store(f);
        s.release_i64_local(integer, f);
        Ok(())
    }

    fn emit_typed_array_min_count(
        &self,
        left: I64Local,
        right: I64Local,
        output: I64Local,
        f: &mut Function,
    ) {
        left.load(f);
        right.load(f);
        left.load(f);
        right.load(f);
        f.instruction(&Instruction::I64LtS);
        f.instruction(&Instruction::Select);
        output.store(f);
    }

    fn emit_typed_array_copy_within_method(
        &mut self,
        receiver: &TypedArrayMethodReceiver,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let target_arg = s.reserve_value_local(f);
        let start_arg = s.reserve_value_local(f);
        let end_arg = s.reserve_value_local(f);
        let target = s.reserve_i64_local(f);
        let start = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let remaining = s.reserve_i64_local(f);
        let current_length = s.reserve_i64_local(f);
        self.emit_builtin_arg_to_value(0, &target_arg, f);
        self.emit_builtin_arg_to_value(1, &start_arg, f);
        self.emit_builtin_arg_to_value(2, &end_arg, f);
        self.emit_typed_array_clamped_index(
            &target_arg,
            receiver.length,
            target,
            pending,
            output,
            exit,
            f,
        )?;
        self.emit_typed_array_clamped_index(
            &start_arg,
            receiver.length,
            start,
            pending,
            output,
            exit,
            f,
        )?;
        end_arg.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        receiver.length.load(f);
        end.store(f);
        f.instruction(&Instruction::Else);
        self.emit_typed_array_clamped_index(
            &end_arg,
            receiver.length,
            end,
            pending,
            output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        end.load(f);
        start.load(f);
        f.instruction(&Instruction::I64Sub);
        count.store(f);
        receiver.length.load(f);
        target.load(f);
        f.instruction(&Instruction::I64Sub);
        remaining.store(f);
        self.emit_typed_array_min_count(count, remaining, count, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64GtS);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_validate_typed_array_view(&receiver.object, current_length, pending, f)?;
        self.emit_typed_array_method_abrupt_exit(pending, output, exit, f);
        current_length.load(f);
        start.load(f);
        f.instruction(&Instruction::I64Sub);
        remaining.store(f);
        self.emit_typed_array_min_count(count, remaining, count, f);
        current_length.load(f);
        target.load(f);
        f.instruction(&Instruction::I64Sub);
        remaining.store(f);
        self.emit_typed_array_min_count(count, remaining, count, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64GtS);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_copy_within_bytes(
            &receiver.object,
            start,
            target,
            count,
            pending,
            f,
        )?;
        self.emit_typed_array_method_abrupt_exit(pending, output, exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        for local in [current_length, remaining, count, end, start, target] {
            s.release_i64_local(local, f);
        }
        end_arg.clear(f);
        start_arg.clear(f);
        target_arg.clear(f);
        Ok(())
    }

    fn emit_typed_array_numeric_compare(
        &mut self,
        left_snapshot: &GcLocal<StoredValue>,
        right_snapshot: &GcLocal<StoredValue>,
        comparator: &ValueLocals,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let bits = s.reserve_i64_local(f);
        let undefined = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        undefined.set_undefined(f);
        comparator.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        // The private sort already owns immutable snapshots. Each callback still
        // gets a fresh argument vector, so arguments mutation or reentry cannot
        // overwrite another invocation or the values awaiting sorted writeback.
        let argv = self.emit_argument_vector_from_snapshots([left_snapshot, right_snapshot], f);
        self.emit_function_or_proxy_call_with_argv(comparator, &undefined, &argv, &pending, f)?;
        argv.clear(f);
        result.copy_from(&pending, f);
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_number_payload(pending.value(), result, f)?;
        result.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        result.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        result.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Ne);
        self.open_frame(ControlFrameKind::If, f);
        undefined.set_scalar(ScalarValue::NumberBits(0), f);
        result.set_normal(&undefined, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        let left = s.reserve_value_local(f);
        let right = s.reserve_value_local(f);
        self.emit_stored_value_to_locals(left_snapshot, &left, f);
        self.emit_stored_value_to_locals(right_snapshot, &right, f);
        left.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::BigInt.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_bigint_binary_op_to_locals(BigIntHelperOp::Compare, &left, &right, result, f)?;
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I64Const(0));
        bits.store(f);
        // Equal NaNs remain stable; a lone NaN sorts after every Number.
        left.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        left.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Ne);
        self.open_frame(ControlFrameKind::If, f);
        right.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        right.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Eq);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(1.0_f64.to_bits() as i64));
        bits.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        right.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        right.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Ne);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const((-1.0_f64).to_bits() as i64));
        bits.store(f);
        f.instruction(&Instruction::Else);
        left.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        right.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Lt);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const((-1.0_f64).to_bits() as i64));
        bits.store(f);
        f.instruction(&Instruction::Else);
        left.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        right.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Gt);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(1.0_f64.to_bits() as i64));
        bits.store(f);
        f.instruction(&Instruction::Else);
        left.scalar().load(f);
        f.instruction(&Instruction::I64Const(i64::MIN));
        f.instruction(&Instruction::I64Eq);
        right.scalar().load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const((-1.0_f64).to_bits() as i64));
        bits.store(f);
        f.instruction(&Instruction::Else);
        left.scalar().load(f);
        f.instruction(&Instruction::I64Eqz);
        right.scalar().load(f);
        f.instruction(&Instruction::I64Const(i64::MIN));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(1.0_f64.to_bits() as i64));
        bits.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        undefined.set_number(bits, f);
        result.set_normal(&undefined, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        right.clear(f);
        left.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        pending.clear(f);
        undefined.clear(f);
        s.release_i64_local(bits, f);
        Ok(())
    }

    fn emit_typed_array_sort_values(
        &mut self,
        source: &TypedArrayMethodReceiver,
        comparator: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<ValueArray>, EmitError> {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        let comparison = s.reserve_completion(f);
        let count32 = s.reserve_i32_local(f);
        let cursor32 = s.reserve_i32_local(f);
        let index = s.reserve_i64_local(f);
        let width = s.reserve_i64_local(f);
        let start = s.reserve_i64_local(f);
        let middle = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        let l = s.reserve_i64_local(f);
        let r = s.reserve_i64_local(f);
        let destination = s.reserve_i64_local(f);
        source.length.load(f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        source.length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        count32.store(f);
        value.set_undefined(f);
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&value, f), f);
        let left = s.reserve_gc_local(f).initialize(stored.load(s, f), f);
        let right = s.reserve_gc_local(f).initialize(stored.load(s, f), f);
        let list = s.reserve_gc_local(f).initialize(
            s.array_type::<ValueArray>()
                .filled(GcOperand::reference(&stored, s), count32, f),
            f,
        );
        let scratch = s.reserve_gc_local(f).initialize(
            s.array_type::<ValueArray>()
                .filled(GcOperand::reference(&stored, s), count32, f),
            f,
        );
        let swap = s.reserve_gc_local(f).initialize(list.load(s, f), f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let fill_exit = self.open_frame(ControlFrameKind::Block, f);
        let fill_again = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        source.length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(fill_exit, f);
        self.emit_typed_array_element_read_from_locals(&source.object, index, &value, f)?;
        stored.replace(s.struct_type::<StoredValue>().from_value(&value, f), f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor32.store(f);
        s.array_type::<ValueArray>()
            .write(&list, cursor32, GcOperand::reference(&stored, s), s, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(fill_again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // All source reads precede the first observable comparator call.
        f.instruction(&Instruction::I64Const(1));
        width.store(f);
        let pass_exit = self.open_frame(ControlFrameKind::Block, f);
        let pass_again = self.open_frame(ControlFrameKind::Loop, f);
        width.load(f);
        source.length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(pass_exit, f);
        f.instruction(&Instruction::I64Const(0));
        start.store(f);
        let run_exit = self.open_frame(ControlFrameKind::Block, f);
        let run_again = self.open_frame(ControlFrameKind::Loop, f);
        start.load(f);
        source.length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(run_exit, f);
        start.load(f);
        width.load(f);
        f.instruction(&Instruction::I64Add);
        middle.store(f);
        self.emit_typed_array_min_count(middle, source.length, middle, f);
        middle.load(f);
        width.load(f);
        f.instruction(&Instruction::I64Add);
        end.store(f);
        self.emit_typed_array_min_count(end, source.length, end, f);
        start.load(f);
        l.store(f);
        middle.load(f);
        r.store(f);
        start.load(f);
        destination.store(f);
        let merge_exit = self.open_frame(ControlFrameKind::Block, f);
        let merge_again = self.open_frame(ControlFrameKind::Loop, f);
        destination.load(f);
        end.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(merge_exit, f);
        l.load(f);
        middle.load(f);
        f.instruction(&Instruction::I64LtU);
        r.load(f);
        end.load(f);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        l.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor32.store(f);
        left.replace(
            s.array_type::<ValueArray>()
                .read(&list, cursor32, s, f)
                .reference(),
            f,
        );
        r.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor32.store(f);
        right.replace(
            s.array_type::<ValueArray>()
                .read(&list, cursor32, s, f)
                .reference(),
            f,
        );
        self.emit_typed_array_numeric_compare(&left, &right, comparator, &comparison, f)?;
        self.emit_typed_array_method_abrupt_exit(&comparison, output, exit, f);
        comparison.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Le);
        self.open_frame(ControlFrameKind::If, f);
        l.load(f);
        index.store(f);
        self.emit_increment_local(l, 1, f);
        f.instruction(&Instruction::Else);
        r.load(f);
        index.store(f);
        self.emit_increment_local(r, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        l.load(f);
        middle.load(f);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        l.load(f);
        index.store(f);
        self.emit_increment_local(l, 1, f);
        f.instruction(&Instruction::Else);
        r.load(f);
        index.store(f);
        self.emit_increment_local(r, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor32.store(f);
        stored.replace(
            s.array_type::<ValueArray>()
                .read(&list, cursor32, s, f)
                .reference(),
            f,
        );
        destination.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor32.store(f);
        s.array_type::<ValueArray>().write(
            &scratch,
            cursor32,
            GcOperand::reference(&stored, s),
            s,
            f,
        );
        self.emit_increment_local(destination, 1, f);
        self.emit_branch_to_target(merge_again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        end.load(f);
        start.store(f);
        self.emit_branch_to_target(run_again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        swap.replace(list.load(s, f), f);
        list.replace(scratch.load(s, f), f);
        scratch.replace(swap.load(s, f), f);
        width.load(f);
        f.instruction(&Instruction::I64Const(2));
        f.instruction(&Instruction::I64Mul);
        width.store(f);
        self.emit_branch_to_target(pass_again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        swap.clear(f);
        scratch.clear(f);
        stored.clear(f);
        comparison.clear(f);
        right.clear(f);
        left.clear(f);
        value.clear(f);
        for local in [destination, r, l, end, middle, start, width, index] {
            s.release_i64_local(local, f);
        }
        s.release_i32_local(cursor32, f);
        s.release_i32_local(count32, f);
        Ok(list)
    }

    fn emit_typed_array_with_value(
        &mut self,
        receiver: &TypedArrayMethodReceiver,
        actual: I64Local,
        converted: &ValueLocals,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let input = s.reserve_value_local(f);
        let number = s.reserve_i64_local(f);
        let valid = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        self.emit_value_to_number_payload(&input, pending, f)?;
        self.emit_typed_array_method_abrupt_exit(pending, output, exit, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            number,
            f,
        );
        number.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Lt);
        f.instruction(&Instruction::If(BlockType::Result(ValType::F64)));
        receiver.length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        number.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Add);
        f.instruction(&Instruction::Else);
        number.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.store(f);
        self.emit_builtin_arg_to_value(1, &input, f);
        f.instruction(&Instruction::I32Const(0));
        for kind in [
            crate::module::TypedArrayElementKind::BigInt64,
            crate::module::TypedArrayElementKind::BigUint64,
        ] {
            receiver.kind.load(f);
            f.instruction(&Instruction::I32Const(kind.encode()));
            f.instruction(&Instruction::I32Eq);
            f.instruction(&Instruction::I32Or);
        }
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_bigint_locals(&input, BigIntNumberPolicy::RejectNumber, pending, f)?;
        f.instruction(&Instruction::Else);
        self.emit_value_to_number_payload(&input, pending, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_typed_array_method_abrupt_exit(pending, output, exit, f);
        converted.copy_from(pending.value(), f);
        // The original length resolves negative indices; validity is observed
        // after both coercions, against the current backing buffer.
        self.emit_typed_array_valid_integer_index_i32(&receiver.object, number, actual, valid, f)?;
        valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_range_error(
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_WITH_INDEX_OUT_OF_RANGE,
            pending,
            f,
        )?;
        output.copy_from(pending, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(valid, f);
        s.release_i64_local(number, f);
        input.clear(f);
        Ok(())
    }

    fn emit_typed_array_copy_method(
        &mut self,
        method: TypedArrayCopyMethod,
        source: &TypedArrayMethodReceiver,
        other: &ValueLocals,
        index: I64Local,
        from: I64Local,
        actual: I64Local,
        value: &ValueLocals,
        result_value: &ValueLocals,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        if matches!(method, TypedArrayCopyMethod::With) {
            self.emit_typed_array_with_value(source, actual, other, pending, output, exit, f)?;
        }
        self.emit_typed_array_create_same_type(source.kind, source.length, pending, f)?;
        self.emit_typed_array_method_abrupt_exit(pending, output, exit, f);
        result_value.copy_from(pending.value(), f);
        let target = s
            .reserve_gc_local(f)
            .initialize(result_value.cast_reference::<TypedArrayObject>(s, f), f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        source.length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        match method {
            TypedArrayCopyMethod::Reversed => {
                source.length.load(f);
                index.load(f);
                f.instruction(&Instruction::I64Sub);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Sub);
                from.store(f);
                self.emit_typed_array_element_read_from_locals(&source.object, from, value, f)?;
            }
            TypedArrayCopyMethod::With => {
                index.load(f);
                actual.load(f);
                f.instruction(&Instruction::I64Eq);
                self.open_frame(ControlFrameKind::If, f);
                value.copy_from(other, f);
                f.instruction(&Instruction::Else);
                self.emit_typed_array_element_read_from_locals(&source.object, index, value, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        self.emit_typed_array_element_write_from_locals(&target, index, value, pending, f)?;
        self.emit_typed_array_method_abrupt_exit(pending, output, exit, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        output.set_normal(result_value, f);
        target.clear(f);
        Ok(())
    }
    fn emit_typed_array_sort_method(
        &mut self,
        method: TypedArraySortMethod,
        source: &TypedArrayMethodReceiver,
        receiver: &ValueLocals,
        comparator: &ValueLocals,
        index: I64Local,
        cursor: I32Local,
        value: &ValueLocals,
        result_value: &ValueLocals,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        value.set_undefined(f);
        let target = s
            .reserve_gc_local(f)
            .initialize(source.object.load(s, f), f);
        match method {
            TypedArraySortMethod::InPlace => result_value.copy_from(receiver, f),
            TypedArraySortMethod::Copy => {
                self.emit_typed_array_create_same_type(source.kind, source.length, pending, f)?;
                self.emit_typed_array_method_abrupt_exit(pending, output, exit, f);
                result_value.copy_from(pending.value(), f);
                target.replace(result_value.cast_reference::<TypedArrayObject>(s, f), f);
            }
        }
        let list = self.emit_typed_array_sort_values(source, comparator, output, exit, f)?;
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(value, f), f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        source.length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        stored.replace(
            s.array_type::<ValueArray>()
                .read(&list, cursor, s, f)
                .reference(),
            f,
        );
        self.emit_stored_value_to_locals(&stored, value, f);
        self.emit_typed_array_element_write_from_locals(&target, index, value, pending, f)?;
        self.emit_typed_array_method_abrupt_exit(pending, output, exit, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        output.set_normal(result_value, f);
        stored.clear(f);
        list.clear(f);
        target.clear(f);
        Ok(())
    }

    pub(super) fn emit_typed_array_native_method(
        &mut self,
        method: TypedArrayNativeMethod,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let comparator = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let other = s.reserve_value_local(f);
        let result_value = s.reserve_value_local(f);
        let output = s.reserve_completion(f);
        let pending = s.reserve_completion(f);
        let index = s.reserve_i64_local(f);
        let from = s.reserve_i64_local(f);
        let actual = s.reserve_i64_local(f);
        let cursor = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_builtin_arg_to_value(0, &comparator, f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        if matches!(
            method,
            TypedArrayNativeMethod::Sort | TypedArrayNativeMethod::ToSorted
        ) {
            self.emit_typed_array_sort_comparator_admission(
                &comparator,
                &pending,
                &output,
                exit,
                f,
            )?;
        }
        let source =
            self.emit_typed_array_method_receiver(&method, &receiver, &pending, &output, exit, f)?;
        match method {
            TypedArrayNativeMethod::Reverse => {
                f.instruction(&Instruction::I64Const(0));
                index.store(f);
                let done = self.open_frame(ControlFrameKind::Block, f);
                let again = self.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                source.length.load(f);
                f.instruction(&Instruction::I64Const(2));
                f.instruction(&Instruction::I64DivU);
                f.instruction(&Instruction::I64GeU);
                self.emit_branch_if_to_target(done, f);
                source.length.load(f);
                index.load(f);
                f.instruction(&Instruction::I64Sub);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Sub);
                from.store(f);
                self.emit_typed_array_element_read_from_locals(&source.object, index, &value, f)?;
                self.emit_typed_array_element_read_from_locals(&source.object, from, &other, f)?;
                self.emit_typed_array_element_write_from_locals(
                    &source.object,
                    index,
                    &other,
                    &pending,
                    f,
                )?;
                self.emit_typed_array_method_abrupt_exit(&pending, &output, exit, f);
                self.emit_typed_array_element_write_from_locals(
                    &source.object,
                    from,
                    &value,
                    &pending,
                    f,
                )?;
                self.emit_typed_array_method_abrupt_exit(&pending, &output, exit, f);
                self.emit_increment_local(index, 1, f);
                self.emit_branch_to_target(again, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                output.set_normal(&receiver, f);
            }
            TypedArrayNativeMethod::CopyWithin => {
                self.emit_typed_array_copy_within_method(&source, &pending, &output, exit, f)?;
                output.set_normal(&receiver, f);
            }
            TypedArrayNativeMethod::ToReversed => self.emit_typed_array_copy_method(
                TypedArrayCopyMethod::Reversed,
                &source,
                &other,
                index,
                from,
                actual,
                &value,
                &result_value,
                &pending,
                &output,
                exit,
                f,
            )?,
            TypedArrayNativeMethod::With => self.emit_typed_array_copy_method(
                TypedArrayCopyMethod::With,
                &source,
                &other,
                index,
                from,
                actual,
                &value,
                &result_value,
                &pending,
                &output,
                exit,
                f,
            )?,
            TypedArrayNativeMethod::Sort => self.emit_typed_array_sort_method(
                TypedArraySortMethod::InPlace,
                &source,
                &receiver,
                &comparator,
                index,
                cursor,
                &value,
                &result_value,
                &pending,
                &output,
                exit,
                f,
            )?,
            TypedArrayNativeMethod::ToSorted => self.emit_typed_array_sort_method(
                TypedArraySortMethod::Copy,
                &source,
                &receiver,
                &comparator,
                index,
                cursor,
                &value,
                &result_value,
                &pending,
                &output,
                exit,
                f,
            )?,
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        source.clear(s, f);
        s.release_i32_local(cursor, f);
        for local in [actual, from, index] {
            s.release_i64_local(local, f);
        }
        pending.clear(f);
        output.clear(f);
        result_value.clear(f);
        other.clear(f);
        value.clear(f);
        comparator.clear(f);
        receiver.clear(f);
        Ok(())
    }
}
