//! A second canonical dense table supplies primitive keys for one full period.
use super::super::dense_array::{ReadKey, RegionInputSource};
use super::*;

#[derive(PartialEq, Eq)]
pub(in crate::control_flow::scalar_number_loop) struct CyclicReadKey {
    array: RegionInputSource,
    mask: u32,
}

impl CyclicReadKey {
    pub(in crate::control_flow::scalar_number_loop) fn new(
        builder: &FunctionBuilder<'_>,
        expression: &TypedExpr,
        counter: LocalBindingId,
    ) -> Option<Self> {
        let read = PropertyReadView::of(expression)?;
        let array = RegionInputSource::of(builder, read.target)?;
        let ReadKey::Computed(index) = read.key else {
            return None;
        };
        let ExprIr::BitwiseNumeric {
            op: BitwiseBinaryOp::And,
            lhs,
            rhs,
        } = &index.expr
        else {
            return None;
        };
        if dense_array::private_local(builder, lhs) != Some(counter) || array.is_local(counter) {
            return None;
        }
        let ExprIr::Number(bits) = rhs.expr else {
            return None;
        };
        let mask = super::super::chunked::bounded_counter_literal(bits)?;
        if mask >= 1 << 20 || !(mask + 1).is_power_of_two() {
            return None;
        }
        Some(Self { array, mask })
    }

    pub(in crate::control_flow::scalar_number_loop) fn period(&self) -> u32 {
        self.mask + 1
    }
    pub(in crate::control_flow::scalar_number_loop) fn is_local_receiver(
        &self,
        id: LocalBindingId,
    ) -> bool {
        self.array.is_local(id)
    }
}

pub(super) struct CyclicKeyBorrow {
    values: GcLocal<PackedValueTable, Nullable>,
    mask: u32,
}

impl CyclicKeyBorrow {
    pub(super) fn new(
        plan: &CyclicReadKey,
        builder: &FunctionBuilder<'_>,
        f: &mut Function,
    ) -> Self {
        let s = builder.runtime_schema();
        Self {
            values: s.reserve_gc_local(f).initialize_null(s, f),
            mask: plan.mask,
        }
    }

    pub(super) fn prepare(
        &self,
        plan: &CyclicReadKey,
        builder: &mut FunctionBuilder<'_>,
        done: ControlTarget,
        f: &mut Function,
    ) {
        let s = builder.runtime_schema();
        let source = s.reserve_value_local(f);
        source.set_undefined(f);
        let array = s
            .reserve_gc_local::<ArrayObject, Nullable>(f)
            .initialize_null(s, f);
        let length = s.reserve_i64_local(f);
        let ready = s.reserve_i32_local(f);
        ready.set_constant(0, f);
        let declined = builder.open_frame(ControlFrameKind::Block, f);
        plan.array.load(&source, builder, declined, f);
        source.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Array.tag()));
        f.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(declined, f);
        source.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(declined, f);
        array.replace(source.cast_reference::<ArrayObject>(s, f).nullable(), f);
        s.field(ArrayObjectSchema::MODE).read(&array, s, f);
        f.instruction(&Instruction::I32Const(
            ArrayIndexedStorageMode::PackedValues.encode(),
        ));
        f.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(declined, f);
        s.field(ArrayObjectSchema::LENGTH)
            .read(&array, s, f)
            .store_i64(length, f);
        length.load(f);
        f.instruction(&Instruction::I64Const(i64::from(self.mask)));
        f.instruction(&Instruction::I64LeU);
        builder.emit_branch_if_to_target(declined, f);
        length.load(f);
        f.instruction(&Instruction::I64Const(1 << 20));
        f.instruction(&Instruction::I64GtU);
        builder.emit_branch_if_to_target(declined, f);
        s.field(ArrayObjectSchema::COUNT).read(&array, s, f);
        length.load(f);
        f.instruction(&Instruction::I64Ne);
        builder.emit_branch_if_to_target(declined, f);
        self.values.replace(
            s.field(ArrayObjectSchema::VALUES)
                .read(&array, s, f)
                .reference(),
            f,
        );
        self.values.load(s, f).is_null(f);
        builder.emit_branch_if_to_target(declined, f);
        s.array_type::<PackedValueTable>()
            .length(&self.values, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.load(f);
        f.instruction(&Instruction::I64LtU);
        builder.emit_branch_if_to_target(declined, f);
        ready.set_constant(1, f);
        builder.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        array.clear(f);
        source.clear(f);
        s.release_i64_local(length, f);
        ready.load(f);
        s.release_i32_local(ready, f);
        f.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(done, f);
    }

    pub(super) fn read(
        &self,
        slot: I32Local,
        key: &ValueLocals,
        builder: &mut FunctionBuilder<'_>,
        declined: ControlTarget,
        f: &mut Function,
    ) {
        let s = builder.runtime_schema();
        let index = s.reserve_i32_local(f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(self.mask as i32));
        f.instruction(&Instruction::I32And);
        index.store(f);
        let stored = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>()
                .read(&self.values, index, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        s.release_i32_local(index, f);
        // Canonical dense mode/count/length prove the selected current own entry.
        s.struct_type::<StoredValue>().read_into(&stored, key, s, f);
        stored.clear(f);
        key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(declined, f);
        key.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<StringValue>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(declined, f);
    }

    pub(super) fn clear_roots(&self, builder: &FunctionBuilder<'_>, f: &mut Function) {
        self.values.set_null(builder.runtime_schema(), f);
    }
    pub(super) fn release(self, f: &mut Function) {
        self.values.clear(f);
    }
}
