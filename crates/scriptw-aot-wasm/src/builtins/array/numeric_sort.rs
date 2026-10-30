//! Pure numeric callbacks consume an independent unboxed snapshot. The live
//! receiver is borrowed only during snapshot and revalidated at publication.
use super::*;
use crate::gc_types::{BinaryNumericCallback, FunctionLocal, NumberBitsI64Array};

impl FunctionBuilder<'_> {
    pub(super) fn emit_try_array_private_number_sort(
        &mut self,
        receiver: &ValueLocals,
        target: &ValueLocals,
        length: I64Local,
        comparator: &ValueLocals,
        pending: &CompletionLocals,
        output: ArraySortOutput,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        let entry = self.emit_binary_numeric_sort_entry(comparator, f)?;
        entry.load(f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let snapshot = self.emit_try_packed_number_sort_snapshot(receiver, length, f)?;
        snapshot.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let callable = s
            .reserve_function_local(f)
            .initialize(entry.load(f).require_non_null(f), f);
        let list = s
            .reserve_gc_local(f)
            .initialize(snapshot.load(s, f).require_non_null(f), f);
        let list =
            self.emit_array_private_number_sort(list, length, &callable, comparator, pending, f)?;
        callable.clear(f);
        let written = self.open_frame(ControlFrameKind::Block, f);
        if matches!(output, ArraySortOutput::Receiver) {
            let done = self.emit_try_packed_number_sort_writeback(target, length, &list, f)?;
            done.load(f);
            s.release_i32_local(done, f);
            self.emit_branch_if_to_target(written, f);
        }
        let fresh = match output {
            ArraySortOutput::Receiver => None,
            ArraySortOutput::Copy => Some(
                s.reserve_gc_local(f)
                    .initialize(target.cast_reference::<ArrayObject>(s, f), f),
            ),
        };
        let index = s.reserve_i64_local(f);
        let slot = s.reserve_i32_local(f);
        let value = s.reserve_value_local(f);
        index.set_constant(0, f);
        value.set_number(index, f);
        let end = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(end, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<NumberBitsI64Array>()
            .read(&list, slot, s, f)
            .store_i64(value.scalar(), f);
        match output {
            ArraySortOutput::Receiver => {
                self.emit_array_native_set_index(target, index, &value, pending, f)?
            }
            ArraySortOutput::Copy => self.emit_array_indexed_publish_default_data(
                fresh.as_ref().expect("unexposed copy"),
                index,
                &value,
                f,
            )?,
        }
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.clear(f);
        if let Some(fresh) = fresh {
            fresh.clear(f);
        }
        s.release_i32_local(slot, f);
        s.release_i64_local(index, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        handled.set_constant(1, f);
        list.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        snapshot.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        entry.clear(f);
        Ok(handled)
    }

    fn emit_number_sort_greater(
        &mut self,
        entry: &FunctionLocal<BinaryNumericCallback>,
        comparator: &ValueLocals,
        left: &ValueLocals,
        right: &ValueLocals,
        greater: I32Local,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let admitted = s.reserve_i32_local(f);
        let result = s.reserve_f64_local(f);
        s.call_binary_numeric_reference(entry, left, right, admitted, result, f);
        admitted.load(f);
        self.open_frame(ControlFrameKind::If, f);
        result.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Gt);
        greater.store(f);
        f.instruction(&Instruction::Else);
        // The scalar thunk retains the source-depth limit. Its refusal must
        // follow the original call/error path, never silently skip a callback.
        let l = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(left, f), f);
        let r = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(right, f), f);
        let argv = self.emit_argument_vector_from_snapshots([&l, &r], f);
        let undefined = s.reserve_value_local(f);
        let numeric = s.reserve_value_local(f);
        undefined.set_undefined(f);
        self.emit_function_or_proxy_call_with_argv(comparator, &undefined, &argv, pending, f)?;
        self.emit_array_native_propagate(pending, f);
        numeric.copy_from(pending.value(), f);
        self.emit_value_to_number_payload(&numeric, pending, f)?;
        self.emit_array_native_propagate(pending, f);
        pending.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Gt);
        greater.store(f);
        numeric.clear(f);
        undefined.clear(f);
        argv.clear(f);
        r.clear(f);
        l.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_f64_local(result, f);
        s.release_i32_local(admitted, f);
        Ok(())
    }

    fn emit_array_private_number_sort(
        &mut self,
        list: GcLocal<NumberBitsI64Array>,
        count: I64Local,
        entry: &FunctionLocal<BinaryNumericCallback>,
        comparator: &ValueLocals,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<GcLocal<NumberBitsI64Array>, EmitError> {
        let s = self.runtime_schema();
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        let greater = s.reserve_i32_local(f);
        let direction = s.reserve_i32_local(f);
        let slot = s.reserve_i32_local(f);
        let width = s.reserve_i64_local(f);
        let start = s.reserve_i64_local(f);
        let middle = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        let l = s.reserve_i64_local(f);
        let r = s.reserve_i64_local(f);
        let chosen = s.reserve_i64_local(f);
        let destination = s.reserve_i64_local(f);
        let bits = s.reserve_i64_local(f);
        let left = s.reserve_value_local(f);
        let right = s.reserve_value_local(f);
        bits.set_constant(0, f);
        left.set_number(bits, f);
        right.set_number(bits, f);
        slot.set_constant(0, f);
        s.array_type::<NumberBitsI64Array>()
            .read(&list, slot, s, f)
            .store_i64(left.scalar(), f);
        let sorted = self.open_frame(ControlFrameKind::Block, f);
        direction.set_constant(-1, f);
        r.set_constant(1, f);
        let unordered = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        r.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        direction.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        // Only strictly descending runs reverse; equality/NaN must keep order.
        l.set_constant(0, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        r.store(f);
        let reversed = self.open_frame(ControlFrameKind::Block, f);
        let reverse = self.open_frame(ControlFrameKind::Loop, f);
        l.load(f);
        r.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(reversed, f);
        l.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<NumberBitsI64Array>()
            .read(&list, slot, s, f)
            .store_i64(bits, f);
        r.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<NumberBitsI64Array>()
            .read(&list, slot, s, f)
            .store_i64(right.scalar(), f);
        s.array_type::<NumberBitsI64Array>()
            .write(&list, slot, GcOperand::i64_local(bits), s, f);
        l.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<NumberBitsI64Array>().write(
            &list,
            slot,
            GcOperand::i64_local(right.scalar()),
            s,
            f,
        );
        self.emit_increment_local(l, 1, f);
        self.emit_increment_local(r, -1, f);
        self.emit_branch_to_target(reverse, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(sorted, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        r.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<NumberBitsI64Array>()
            .read(&list, slot, s, f)
            .store_i64(right.scalar(), f);
        self.emit_number_sort_greater(entry, comparator, &left, &right, greater, pending, f)?;
        direction.load(f);
        f.instruction(&Instruction::I32Const(-1));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        greater.load(f);
        direction.store(f);
        f.instruction(&Instruction::Else);
        greater.load(f);
        direction.load(f);
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(unordered, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        right.scalar().load(f);
        left.scalar().store(f);
        self.emit_increment_local(r, 1, f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);

        count.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        let scratch = s.reserve_gc_local(f).initialize(
            s.array_type::<NumberBitsI64Array>()
                .filled(GcOperand::i64_local(bits), slot, f),
            f,
        );
        let swap = s.reserve_gc_local(f).initialize(list.load(s, f), f);
        width.set_constant(1, f);
        let passes_done = self.open_frame(ControlFrameKind::Block, f);
        let pass = self.open_frame(ControlFrameKind::Loop, f);
        width.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(passes_done, f);
        start.set_constant(0, f);
        let runs_done = self.open_frame(ControlFrameKind::Block, f);
        let run = self.open_frame(ControlFrameKind::Loop, f);
        start.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(runs_done, f);
        start.load(f);
        width.load(f);
        f.instruction(&Instruction::I64Add);
        middle.store(f);
        self.emit_array_sort_min_count(middle, count, f);
        middle.load(f);
        width.load(f);
        f.instruction(&Instruction::I64Add);
        end.store(f);
        self.emit_array_sort_min_count(end, count, f);
        start.load(f);
        l.store(f);
        start.load(f);
        destination.store(f);
        middle.load(f);
        r.store(f);
        let merged = self.open_frame(ControlFrameKind::Block, f);
        let merge = self.open_frame(ControlFrameKind::Loop, f);
        destination.load(f);
        end.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(merged, f);
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
        slot.store(f);
        s.array_type::<NumberBitsI64Array>()
            .read(&list, slot, s, f)
            .store_i64(left.scalar(), f);
        r.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<NumberBitsI64Array>()
            .read(&list, slot, s, f)
            .store_i64(right.scalar(), f);
        self.emit_number_sort_greater(entry, comparator, &left, &right, greater, pending, f)?;
        greater.load(f);
        self.open_frame(ControlFrameKind::If, f);
        r.load(f);
        chosen.store(f);
        self.emit_increment_local(r, 1, f);
        f.instruction(&Instruction::Else);
        l.load(f);
        chosen.store(f);
        self.emit_increment_local(l, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        l.load(f);
        middle.load(f);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        l.load(f);
        chosen.store(f);
        self.emit_increment_local(l, 1, f);
        f.instruction(&Instruction::Else);
        r.load(f);
        chosen.store(f);
        self.emit_increment_local(r, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        chosen.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<NumberBitsI64Array>()
            .read(&list, slot, s, f)
            .store_i64(bits, f);
        destination.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<NumberBitsI64Array>().write(
            &scratch,
            slot,
            GcOperand::i64_local(bits),
            s,
            f,
        );
        self.emit_increment_local(destination, 1, f);
        self.emit_branch_to_target(merge, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        end.load(f);
        start.store(f);
        self.emit_branch_to_target(run, f);
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
        self.emit_branch_to_target(pass, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        swap.clear(f);
        scratch.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        right.clear(f);
        left.clear(f);
        for local in [bits, destination, chosen, r, l, end, middle, start, width] {
            s.release_i64_local(local, f);
        }
        for local in [slot, direction, greater] {
            s.release_i32_local(local, f);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(list)
    }
}
