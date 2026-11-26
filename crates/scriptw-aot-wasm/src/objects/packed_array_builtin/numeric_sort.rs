//! Packed Number sorting retains the original unboxed bit patterns.
use super::*;
use crate::gc_types::{BinaryNumericCallback, FunctionLocal, GcIndex};

impl FunctionBuilder<'_> {
    pub(crate) fn emit_packed_number_monotonic_sort_or_fall_through(
        &mut self,
        receiver: &ValueLocals,
        length: I64Local,
        compare: &FunctionLocal<BinaryNumericCallback>,
        handled: I32Local,
        completed: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let decline = self.open_frame(ControlFrameKind::Block, f);
        length.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64LeU);
        self.emit_branch_if_to_target(decline, f);
        let numbers =
            self.emit_dense_packed_number_sort_buffer_or_branch(receiver, length, decline, f);
        s.emit_binary_numeric_subtraction_admitted(compare, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);

        let extent = s.reserve_i32_local(f);
        length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        extent.store(f);
        self.emit_scan_packed_number_sort(&numbers, extent, decline, f);
        s.release_i32_local(extent, f);
        numbers.clear(f);
        handled.set_constant(1, f);
        self.emit_branch_to_target(completed, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }

    fn emit_dense_packed_number_sort_buffer_or_branch(
        &mut self,
        receiver: &ValueLocals,
        length: I64Local,
        decline: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<NumberBitsI64Array> {
        let s = self.runtime_schema();
        let array = s.reserve_gc_local(f).initialize(
            receiver.sealed_reference_or_branch::<ArrayObject>(
                s,
                self.local_branch_label(decline),
                f,
            ),
            f,
        );
        // Equal occupancy and length prove every index is an own writable
        // data property. No read or write can consult the prototype chain.
        s.array_storage_mode_is(&array, ArrayIndexedStorageMode::PackedNumbers, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        for field in [ArrayObjectSchema::LENGTH, ArrayObjectSchema::COUNT] {
            s.field(field).read(&array, s, f);
            length.load(f);
            f.instruction(&Instruction::I64Ne);
            self.emit_branch_if_to_target(decline, f);
        }
        let numbers = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::NUMBERS)
                .read(&array, s, f)
                .reference(),
            f,
        );
        array.clear(f);
        numbers
    }

    /// A run already in order keeps all ties in place. Only a strictly
    /// descending run can reverse. NaNs decline before any live write.
    fn emit_scan_packed_number_sort(
        &mut self,
        numbers: &GcLocal<NumberBitsI64Array>,
        extent: I32Local,
        decline: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let array = s.array_type::<NumberBitsI64Array>();
        let index = s.reserve_i32_local(f);
        let paired_end = s.reserve_i32_local(f);
        let previous = s.reserve_f64_local(f);
        let current = s.reserve_f64_local(f);
        array.read(numbers, GcIndex::Constant(0), s, f);
        f.instruction(&Instruction::F64ReinterpretI64);
        previous.store(f);
        array.read(numbers, GcIndex::Constant(1), s, f);
        f.instruction(&Instruction::F64ReinterpretI64);
        current.store(f);
        index.set_constant(2, f);
        extent.load(f);
        f.instruction(&Instruction::I32Const(-2));
        f.instruction(&Instruction::I32And);
        paired_end.store(f);
        previous.load(f);
        current.load(f);
        f.instruction(&Instruction::F64Gt);
        self.open_frame(ControlFrameKind::If, f);
        for (descending, order) in [(true, Instruction::F64Gt), (false, Instruction::F64Le)] {
            if !descending {
                f.instruction(&Instruction::Else);
                // The first pair also needs an ordered comparison: the
                // failed descending test could have seen a NaN.
                previous.load(f);
                current.load(f);
                f.instruction(&Instruction::F64Le);
                f.instruction(&Instruction::I32Eqz);
                self.emit_branch_if_to_target(decline, f);
            }
            current.load(f);
            previous.store(f);
            let scan_step = |f: &mut Function| {
                array.read(numbers, index, s, f);
                f.instruction(&Instruction::F64ReinterpretI64);
                current.store(f);
                previous.load(f);
                current.load(f);
                f.instruction(&order);
                current.load(f);
                previous.store(f);
                index.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Add);
                index.store(f);
            };
            index.load(f);
            paired_end.load(f);
            f.instruction(&Instruction::I32LtU);
            self.open_frame(ControlFrameKind::If, f);
            let scan = self.open_frame(ControlFrameKind::Loop, f);
            scan_step(f);
            scan_step(f);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(decline, f);
            index.load(f);
            paired_end.load(f);
            f.instruction(&Instruction::I32LtU);
            self.emit_branch_if_to_target(scan, f);
            self.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            index.load(f);
            extent.load(f);
            f.instruction(&Instruction::I32LtU);
            self.open_frame(ControlFrameKind::If, f);
            scan_step(f);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(decline, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            if descending {
                self.emit_reverse_number_sort_prefix(numbers, extent, f);
            }
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_f64_local(current, f);
        s.release_f64_local(previous, f);
        s.release_i32_local(paired_end, f);
        s.release_i32_local(index, f);
    }

    /// Reverse a run after its caller has established it has at least two elements.
    pub(crate) fn emit_reverse_number_sort_prefix(
        &mut self,
        numbers: &GcLocal<NumberBitsI64Array>,
        extent: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let array = s.array_type::<NumberBitsI64Array>();
        let left = s.reserve_i32_local(f);
        let right = s.reserve_i32_local(f);
        let left_bits = s.reserve_i64_local(f);
        let right_bits = s.reserve_i64_local(f);
        left.set_constant(0, f);
        extent.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        right.store(f);
        let reverse = self.open_frame(ControlFrameKind::Loop, f);
        array.read(numbers, left, s, f).store_i64(left_bits, f);
        array.read(numbers, right, s, f).store_i64(right_bits, f);
        array.write(numbers, left, GcOperand::i64_local(right_bits), s, f);
        array.write(numbers, right, GcOperand::i64_local(left_bits), s, f);
        for (slot, delta) in [(left, 1), (right, -1)] {
            slot.load(f);
            f.instruction(&Instruction::I32Const(delta));
            f.instruction(&Instruction::I32Add);
            slot.store(f);
        }
        left.load(f);
        right.load(f);
        f.instruction(&Instruction::I32LtU);
        self.emit_branch_if_to_target(reverse, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        s.release_i64_local(right_bits, f);
        s.release_i64_local(left_bits, f);
        s.release_i32_local(right, f);
        s.release_i32_local(left, f);
    }

    pub(crate) fn emit_try_packed_number_sort_snapshot(
        &mut self,
        receiver: &ValueLocals,
        length: I64Local,
        f: &mut Function,
    ) -> GcLocal<NumberBitsI64Array, Nullable> {
        let s = self.runtime_schema();
        let result = s
            .reserve_gc_local::<NumberBitsI64Array, Nullable>(f)
            .initialize_null(s, f);
        let decline = self.open_frame(ControlFrameKind::Block, f);
        let numbers =
            self.emit_dense_packed_number_sort_buffer_or_branch(receiver, length, decline, f);
        let extent = s.reserve_i32_local(f);
        length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        extent.store(f);
        let backing = s.array_type::<NumberBitsI64Array>();
        let list = s
            .reserve_gc_local(f)
            .initialize(backing.filled(GcOperand::i64(0), extent, f), f);
        backing.copy_prefix_from(&list, &numbers, extent, s, f);
        result.replace(list.load(s, f).nullable(), f);
        list.clear(f);
        numbers.clear(f);
        s.release_i32_local(extent, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        result
    }

    /// Revalidate live own writable data before one non-observable bulk copy.
    /// Flags, occupancy, length and capacity remain owned by canonical storage.
    pub(crate) fn emit_try_packed_number_sort_writeback(
        &mut self,
        receiver: &ValueLocals,
        length: I64Local,
        list: &GcLocal<NumberBitsI64Array>,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        let decline = self.open_frame(ControlFrameKind::Block, f);
        let numbers =
            self.emit_dense_packed_number_sort_buffer_or_branch(receiver, length, decline, f);
        let extent = s.reserve_i32_local(f);
        length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        extent.store(f);
        s.array_type::<NumberBitsI64Array>()
            .copy_prefix_from(&numbers, list, extent, s, f);
        numbers.clear(f);
        s.release_i32_local(extent, f);
        handled.set_constant(1, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        handled
    }
}
