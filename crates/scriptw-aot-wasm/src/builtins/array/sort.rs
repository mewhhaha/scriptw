//! Sort a private snapshot without allocating another value wrapper per move.
//! Stable merging selects the left element whenever the comparison is equal,
//! including comparator NaN. All source reads precede this private operation;
//! receiver writeback still belongs to the ordinary Array algorithm.
use super::*;
use crate::gc_types::{StoredValue, ValueArray};

impl FunctionBuilder<'_> {
    pub(super) fn emit_array_private_values_sort(
        &mut self,
        list: GcLocal<ValueArray>,
        count: I64Local,
        comparator: &ValueLocals,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<GcLocal<ValueArray>, EmitError> {
        let s = self.runtime_schema();
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        let greater = s.reserve_i32_local(f);
        let cursor = s.reserve_i32_local(f);
        let width = s.reserve_i64_local(f);
        let start = s.reserve_i64_local(f);
        let middle = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        let l = s.reserve_i64_local(f);
        let r = s.reserve_i64_local(f);
        let selected = s.reserve_i64_local(f);
        let destination = s.reserve_i64_local(f);
        cursor.set_constant(0, f);
        // Every list entry is an immutable snapshot. Private spare slots may
        // share the first snapshot until overwritten, without additional boxes.
        let stored = s.reserve_gc_local::<StoredValue, _>(f).initialize(
            s.array_type::<ValueArray>()
                .read(&list, cursor, s, f)
                .reference(),
            f,
        );
        let left = s.reserve_gc_local(f).initialize(stored.load(s, f), f);
        let right = s.reserve_gc_local(f).initialize(stored.load(s, f), f);
        let sorted = self.open_frame(ControlFrameKind::Block, f);
        // Retain the linear best case without scratch allocation. A failed
        // adjacent ordering check falls through to stable O(n log n) merging.
        // The spec chooses no comparator call sequence; actual source reads
        // and receiver writes stay outside this private sorting phase.
        r.set_constant(1, f);
        let unordered = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        r.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(sorted, f);
        r.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        stored.replace(
            s.array_type::<ValueArray>()
                .read(&list, cursor, s, f)
                .reference(),
            f,
        );
        right.replace(stored.load(s, f), f);
        self.emit_array_sort_greater(&left, &right, comparator, greater, pending, f)?;
        greater.load(f);
        self.emit_branch_if_to_target(unordered, f);
        left.replace(right.load(s, f), f);
        self.emit_increment_local(r, 1, f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        count.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let scratch = s.reserve_gc_local(f).initialize(
            s.array_type::<ValueArray>()
                .filled(GcOperand::reference(&stored, s), cursor, f),
            f,
        );
        let swap = s.reserve_gc_local(f).initialize(list.load(s, f), f);
        width.set_constant(1, f);
        let pass_exit = self.open_frame(ControlFrameKind::Block, f);
        let pass_again = self.open_frame(ControlFrameKind::Loop, f);
        width.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(pass_exit, f);
        start.set_constant(0, f);
        let run_exit = self.open_frame(ControlFrameKind::Block, f);
        let run_again = self.open_frame(ControlFrameKind::Loop, f);
        start.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(run_exit, f);
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
        cursor.store(f);
        stored.replace(
            s.array_type::<ValueArray>()
                .read(&list, cursor, s, f)
                .reference(),
            f,
        );
        left.replace(stored.load(s, f), f);
        r.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        stored.replace(
            s.array_type::<ValueArray>()
                .read(&list, cursor, s, f)
                .reference(),
            f,
        );
        right.replace(stored.load(s, f), f);
        self.emit_array_sort_greater(&left, &right, comparator, greater, pending, f)?;
        greater.load(f);
        self.open_frame(ControlFrameKind::If, f);
        r.load(f);
        selected.store(f);
        self.emit_increment_local(r, 1, f);
        f.instruction(&Instruction::Else);
        l.load(f);
        selected.store(f);
        self.emit_increment_local(l, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        l.load(f);
        middle.load(f);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        l.load(f);
        selected.store(f);
        self.emit_increment_local(l, 1, f);
        f.instruction(&Instruction::Else);
        r.load(f);
        selected.store(f);
        self.emit_increment_local(r, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        selected.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        stored.replace(
            s.array_type::<ValueArray>()
                .read(&list, cursor, s, f)
                .reference(),
            f,
        );
        destination.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        s.array_type::<ValueArray>().write(
            &scratch,
            cursor,
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
        // count is the length of a Wasm32 GC array. Wide loop arithmetic keeps
        // the final doubled width and run endpoints from wrapping at 2^32.
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
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        stored.clear(f);
        right.clear(f);
        left.clear(f);
        s.release_i32_local(cursor, f);
        s.release_i32_local(greater, f);
        for local in [destination, selected, r, l, end, middle, start, width] {
            s.release_i64_local(local, f);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(list)
    }

    pub(super) fn emit_array_sort_min_count(
        &self,
        value: I64Local,
        count: I64Local,
        f: &mut Function,
    ) {
        value.load(f);
        count.load(f);
        value.load(f);
        count.load(f);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::Select);
        value.store(f);
    }
}
