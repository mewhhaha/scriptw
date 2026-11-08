//! Entry-owned roots and scalar aliases; no source callback crosses this borrow.
use super::*;

struct CallRoot {
    callable: GcLocal<FunctionObject, Nullable>,
    selected: I32Local,
}
struct SlotRoot {
    cell: Option<GcLocal<BindingCell, Nullable>>,
    alias: I32Local,
    dirty: I32Local,
    number: F64Local,
}
pub(in super::super) struct CapturedCallRoots {
    calls: Vec<CallRoot>,
    slots: Vec<SlotRoot>,
}

fn capture_cell(
    callable: &GcLocal<FunctionObject, Nullable>,
    capture: &CapturedBindingIr,
    builder: &mut FunctionBuilder<'_>,
    function: &mut Function,
) -> GcLocal<BindingCell> {
    let s = builder.runtime_schema();
    let context = s.reserve_gc_local(function).initialize(
        s.field(FunctionObjectSchema::CONTEXT)
            .read(callable, s, function)
            .reference(),
        function,
    );
    let environment = s.reserve_gc_local(function).initialize(
        s.field(FunctionContextSchema::LEXICAL_ENVIRONMENT)
            .read(&context, s, function)
            .reference(),
        function,
    );
    for _ in 0..capture.hops {
        environment.replace(
            s.field(EnvironmentSchema::PARENT)
                .read(&environment, s, function)
                .reference(),
            function,
        );
    }
    let cell = builder.emit_environment_cell_local(&environment, capture.slot, function);
    environment.clear(function);
    context.clear(function);
    cell
}

impl CapturedCalls {
    pub(in super::super) fn guard(
        &self,
        builder: &mut FunctionBuilder<'_>,
        accepted: I32Local,
        function: &mut Function,
    ) -> Result<CapturedCallRoots, EmitError> {
        let s = builder.runtime_schema();
        let roots = CapturedCallRoots {
            calls: self
                .calls
                .iter()
                .map(|_| {
                    let selected = s.reserve_i32_local(function);
                    selected.set_constant(-1, function);
                    CallRoot {
                        callable: s.reserve_gc_local(function).initialize_null(s, function),
                        selected,
                    }
                })
                .collect(),
            slots: self
                .slots
                .iter()
                .enumerate()
                .map(|(index, slot)| {
                    let alias = s.reserve_i32_local(function);
                    alias.set_constant(index as i32, function);
                    let dirty = s.reserve_i32_local(function);
                    dirty.set_constant(0, function);
                    let number = s.reserve_f64_local(function);
                    function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
                    number.store(function);
                    let cell = match slot {
                        Slot::Temporary => None,
                        Slot::Capture { .. } => {
                            Some(s.reserve_gc_local(function).initialize_null(s, function))
                        }
                    };
                    SlotRoot {
                        cell,
                        alias,
                        dirty,
                        number,
                    }
                })
                .collect(),
        };
        accepted.load(function);
        builder.open_frame(ControlFrameKind::If, function);
        accepted.set_constant(0, function);
        let decline = builder.open_frame(ControlFrameKind::Block, function);
        s.emit_source_activation_admission_test_for_depth(self.depth, function);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        let value = s.reserve_value_local(function);
        for (index, call) in self.calls.iter().enumerate() {
            match &call.target {
                Target::Input(source) => source.load(&value, builder, decline, function),
                Target::CapturedArray {
                    parent,
                    capture,
                    index,
                } => {
                    let cell =
                        capture_cell(&roots.calls[*parent].callable, capture, builder, function);
                    let initialized = builder.emit_read_environment_cell(&cell, &value, function);
                    initialized.load(function);
                    s.release_i32_local(initialized, function);
                    cell.clear(function);
                    function.instruction(&Instruction::I32Eqz);
                    builder.emit_branch_if_to_target(decline, function);
                    value.tag().load(function);
                    function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Array.tag()));
                    function.instruction(&Instruction::I32Ne);
                    builder.emit_branch_if_to_target(decline, function);
                    value.reference().load(function);
                    function.instruction(&Instruction::RefTestNonNull(
                        s.reference_type::<ArrayObject>(GcNullability::NonNullable)
                            .heap_type,
                    ));
                    function.instruction(&Instruction::I32Eqz);
                    builder.emit_branch_if_to_target(decline, function);
                    let array = s
                        .reserve_gc_local(function)
                        .initialize(value.cast_reference::<ArrayObject>(s, function), function);
                    s.field(ArrayObjectSchema::MODE).read(&array, s, function);
                    function.instruction(&Instruction::I32Const(
                        ArrayIndexedStorageMode::PackedValues.encode(),
                    ));
                    function.instruction(&Instruction::I32Ne);
                    builder.emit_branch_if_to_target(decline, function);
                    s.field(ArrayObjectSchema::COUNT).read(&array, s, function);
                    s.field(ArrayObjectSchema::LENGTH).read(&array, s, function);
                    function.instruction(&Instruction::I64Ne);
                    builder.emit_branch_if_to_target(decline, function);
                    s.field(ArrayObjectSchema::LENGTH).read(&array, s, function);
                    function.instruction(&Instruction::I64Const(i64::from(*index)));
                    function.instruction(&Instruction::I64LeU);
                    builder.emit_branch_if_to_target(decline, function);
                    let values = s.reserve_gc_local(function).initialize(
                        s.field(ArrayObjectSchema::VALUES)
                            .read(&array, s, function)
                            .reference(),
                        function,
                    );
                    values.load(s, function).is_null(function);
                    builder.emit_branch_if_to_target(decline, function);
                    s.array_type::<PackedValueTable>()
                        .length(&values, s, function);
                    function.instruction(&Instruction::I32Const(*index as i32));
                    function.instruction(&Instruction::I32LeU);
                    builder.emit_branch_if_to_target(decline, function);
                    let slot = s.reserve_i32_local(function);
                    slot.set_constant(*index as i32, function);
                    let stored = s.reserve_gc_local(function).initialize(
                        s.array_type::<PackedValueTable>()
                            .read(&values, slot, s, function)
                            .reference(),
                        function,
                    );
                    s.release_i32_local(slot, function);
                    stored.load(s, function).is_null(function);
                    builder.emit_branch_if_to_target(decline, function);
                    let present = s.reserve_gc_local(function).initialize(
                        stored.load(s, function).require_non_null(function),
                        function,
                    );
                    s.struct_type::<StoredValue>()
                        .read_into(&present, &value, s, function);
                    present.clear(function);
                    stored.clear(function);
                    values.clear(function);
                    array.clear(function);
                }
            }
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
            function.instruction(&Instruction::I32Ne);
            builder.emit_branch_if_to_target(decline, function);
            value.reference().load(function);
            function.instruction(&Instruction::RefTestNonNull(
                s.reference_type::<FunctionObject>(GcNullability::NonNullable)
                    .heap_type,
            ));
            function.instruction(&Instruction::I32Eqz);
            builder.emit_branch_if_to_target(decline, function);
            let callable = &roots.calls[index].callable;
            callable.replace(
                value
                    .cast_reference::<FunctionObject>(s, function)
                    .nullable(),
                function,
            );
            let code = s.reserve_gc_local(function).initialize(
                s.field(FunctionObjectSchema::CODE)
                    .read(callable, s, function)
                    .reference(),
                function,
            );
            s.field(ExecutableCodeSchema::KIND).read(&code, s, function);
            function.instruction(&Instruction::I32Const(
                ExecutableCodeKind::JavaScript.encode(),
            ));
            function.instruction(&Instruction::I32Ne);
            builder.emit_branch_if_to_target(decline, function);
            let context = s.reserve_gc_local(function).initialize(
                s.field(FunctionObjectSchema::CONTEXT)
                    .read(callable, s, function)
                    .reference(),
                function,
            );
            s.field(FunctionContextSchema::DEFINING_PROGRAM)
                .read(&context, s, function)
                .reference();
            builder.current_program().reference(s, function);
            function.instruction(&Instruction::RefEq);
            function.instruction(&Instruction::I32Eqz);
            builder.emit_branch_if_to_target(decline, function);
            for (case_index, case) in call.cases.iter().enumerate() {
                s.field(ExecutableCodeSchema::SOURCE_ID)
                    .read(&code, s, function);
                function.instruction(&Instruction::I64Const(case.source_id));
                function.instruction(&Instruction::I64Eq);
                builder.open_frame(ControlFrameKind::If, function);
                for slot_index in &case.captures {
                    let Slot::Capture {
                        binding, written, ..
                    } = &self.slots[*slot_index]
                    else {
                        unreachable!()
                    };
                    let cell = capture_cell(callable, binding, builder, function);
                    let root = roots.slots[*slot_index]
                        .cell
                        .as_ref()
                        .expect("captured slot");
                    root.replace(cell.load(s, function).nullable(), function);
                    s.field(BindingCellSchema::TARGET)
                        .read(&cell, s, function)
                        .reference()
                        .is_null(function);
                    s.field(BindingCellSchema::INITIALIZED)
                        .read(&cell, s, function);
                    function.instruction(&Instruction::I32And);
                    if *written {
                        s.field(BindingCellSchema::MUTABLE).read(&cell, s, function);
                        function.instruction(&Instruction::I32And);
                    }
                    function.instruction(&Instruction::I32Eqz);
                    builder.emit_branch_if_to_target(decline, function);
                    s.struct_type::<BindingCell>()
                        .read_value(&cell, &value, s, function);
                    value.tag().load(function);
                    function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
                    function.instruction(&Instruction::I32Ne);
                    builder.emit_branch_if_to_target(decline, function);
                    value.scalar().load(function);
                    function.instruction(&Instruction::F64ReinterpretI64);
                    roots.slots[*slot_index].number.store(function);
                    cell.clear(function);
                }
                roots.calls[index]
                    .selected
                    .set_constant(case_index as i32, function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            context.clear(function);
            code.clear(function);
            roots.calls[index].selected.load(function);
            function.instruction(&Instruction::I32Const(0));
            function.instruction(&Instruction::I32LtS);
            builder.emit_branch_if_to_target(decline, function);
        }
        for (index, slot) in roots.slots.iter().enumerate() {
            let Some(cell) = &slot.cell else {
                continue;
            };
            cell.load(s, function).is_null(function);
            function.instruction(&Instruction::I32Eqz);
            builder.open_frame(ControlFrameKind::If, function);
            for previous in &roots.slots[..index] {
                let Some(previous_cell) = &previous.cell else {
                    continue;
                };
                cell.load(s, function);
                previous_cell.load(s, function);
                function.instruction(&Instruction::RefEq);
                builder.open_frame(ControlFrameKind::If, function);
                previous.alias.load(function);
                slot.alias.store(function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            builder.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        accepted.set_constant(1, function);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        value.clear(function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        accepted.load(function);
        function.instruction(&Instruction::I32Eqz);
        builder.open_frame(ControlFrameKind::If, function);
        roots.clear_roots(builder, function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(roots)
    }
}

impl CapturedCallRoots {
    pub(in super::super) fn selected(&self, call: usize, function: &mut Function) {
        self.calls[call].selected.load(function);
    }
    pub(in super::super) fn load(&self, slot: usize, function: &mut Function) {
        self.slots[slot].number.load(function);
    }
    pub(in super::super) fn store(
        &self,
        index: usize,
        plan: &CapturedCalls,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let slot = &self.slots[index];
        slot.number.store(function);
        if !matches!(plan.slots[index], Slot::Capture { .. }) {
            return;
        }
        slot.dirty.set_constant(1, function);
        for (other_index, other) in self.slots.iter().enumerate() {
            if other_index == index || other.cell.is_none() {
                continue;
            }
            slot.alias.load(function);
            other.alias.load(function);
            function.instruction(&Instruction::I32Eq);
            builder.open_frame(ControlFrameKind::If, function);
            slot.number.load(function);
            other.number.store(function);
            other.dirty.set_constant(1, function);
            builder.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
    }
    pub(in super::super) fn publish(
        &self,
        _plan: &CapturedCalls,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let s = builder.runtime_schema();
        let value = s.reserve_value_local(function);
        let bits = s.reserve_i64_local(function);
        for (index, slot) in self.slots.iter().enumerate() {
            let Some(cell) = &slot.cell else {
                continue;
            };
            slot.alias.load(function);
            function.instruction(&Instruction::I32Const(index as i32));
            function.instruction(&Instruction::I32Eq);
            slot.dirty.load(function);
            function.instruction(&Instruction::I32And);
            builder.open_frame(ControlFrameKind::If, function);
            slot.number.load(function);
            function.instruction(&Instruction::I64ReinterpretF64);
            bits.store(function);
            value.set_number(bits, function);
            s.struct_type::<BindingCell>()
                .write_value(cell, &value, s, function);
            builder.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        s.release_i64_local(bits, function);
        value.clear(function);
    }
    fn clear_roots(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        for call in &self.calls {
            call.callable.set_null(builder.runtime_schema(), function);
        }
        for slot in &self.slots {
            if let Some(cell) = &slot.cell {
                cell.set_null(builder.runtime_schema(), function);
            }
        }
    }
    pub(in super::super) fn release(self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        for call in self.calls {
            call.callable.clear(function);
            s.release_i32_local(call.selected, function);
        }
        for slot in self.slots {
            if let Some(cell) = slot.cell {
                cell.clear(function);
            }
            s.release_i32_local(slot.alias, function);
            s.release_i32_local(slot.dirty, function);
            s.release_f64_local(slot.number, function);
        }
    }
}
