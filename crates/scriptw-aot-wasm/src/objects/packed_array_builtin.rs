//! Builtin kernels borrow the canonical packed backing only after the ordinary
//! receiver/length work. No JavaScript runs inside the validated borrow.
use super::*;
use crate::gc_types::{
    ArrayIndexStorage, ArrayIndexStorageSchema, ArrayObject, ArrayObjectSchema, JavaScriptObject,
    ValueArray,
};

/// Available only inside the concrete-Array, captured-length and packed-mode
/// guard. Packed modes contain default writable/enumerable/configurable data.
pub(crate) struct PackedArraySnapshot {
    array: GcLocal<ArrayObject>,
    storage: GcLocal<ArrayIndexStorage>,
}

impl PackedArraySnapshot {
    pub(crate) fn read(
        &self,
        builder: &mut FunctionBuilder<'_>,
        index: I64Local,
        value: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) {
        found.set_constant(0, f);
        builder.emit_packed_array_value(&self.storage, index, value, found, f);
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_with_packed_array_snapshot(
        &mut self,
        receiver: &ValueLocals,
        length: I64Local,
        f: &mut Function,
        consume: impl FnOnce(
            &mut Self,
            &PackedArraySnapshot,
            ControlTarget,
            &mut Function,
        ) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<ArrayObject>(s, f), f);
        let storage = self.emit_array_index_storage(&array, f);
        let snapshot = PackedArraySnapshot { array, storage };
        let decline = self.open_frame(ControlFrameKind::Block, f);
        s.field(ArrayObjectSchema::LENGTH)
            .read(&snapshot.array, s, f);
        length.load(f);
        f.instruction(&Instruction::I64Ne);
        self.emit_branch_if_to_target(decline, f);
        s.array_storage_is_packed(&snapshot.storage, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        consume(self, &snapshot, decline, f)?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        snapshot.storage.clear(f);
        snapshot.array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    /// Bounded and conservative: Array prototypes must have no indexed own
    /// properties; ordinary prototypes must have no descriptor for any append
    /// key. In particular mutable intrinsic prototypes receive the same checks.
    /// Other exotics, proxies and longer chains decline without invoking traps.
    fn emit_packed_push_prototypes_permit_append(
        &mut self,
        receiver: &GcLocal<ArrayObject>,
        start: I64Local,
        end: I64Local,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let permitted = s.reserve_i32_local(f);
        permitted.set_constant(0, f);
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize(
                s.field(ArrayObjectSchema::OBJECT)
                    .read(receiver, s, f)
                    .reference()
                    .nullable(),
                f,
            );
        let current = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROTOTYPE)
                    .read(&header, s, f)
                    .reference(),
                f,
            );
        let depth = s.reserve_i32_local(f);
        depth.set_constant(0, f);
        let index = s.reserve_i64_local(f);
        let acceptable = s.reserve_i32_local(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        current.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        permitted.set_constant(1, f);
        self.emit_branch_to_target(finished, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        depth.load(f);
        f.instruction(&Instruction::I32Const(8));
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(finished, f);
        current.is_subtype::<ArrayObject>(s, f);
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(current.load(s, f).cast_subtype::<ArrayObject>(s, f), f);
        let storage = self.emit_array_index_storage(&array, f);
        s.field(ArrayIndexStorageSchema::COUNT)
            .read(&storage, s, f)
            .store_i64(index, f);
        index.load(f);
        f.instruction(&Instruction::I64Eqz);
        acceptable.store(f);
        header.replace(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, f)
                .reference()
                .nullable(),
            f,
        );
        storage.clear(f);
        array.clear(f);
        acceptable.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(finished, f);
        f.instruction(&Instruction::Else);
        current.is_subtype::<OrdinaryObject>(s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(finished, f);
        header.replace(
            current
                .load(s, f)
                .cast_subtype::<OrdinaryObject>(s, f)
                .nullable(),
            f,
        );
        start.load(f);
        index.store(f);
        let checked = self.open_frame(ControlFrameKind::Block, f);
        let check = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        end.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(checked, f);
        let negative = s.reserve_i32_local(f);
        negative.set_constant(0, f);
        let text = s
            .call_helper(
                crate::runtime_helpers::IntegerDecimalStringArguments::new(index, negative),
                self.runtime_helper_base()?,
                f,
            )
            .bind(s, s.reserve_gc_local(f), f);
        let key = PropertyKeyLocals::from_string(s, &text, f);
        let ordinary = s
            .reserve_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        let descriptor = self.emit_ordinary_own_descriptor_reference(&ordinary, &key, f)?;
        descriptor.load(s, f).is_null(f);
        acceptable.store(f);
        descriptor.clear(f);
        ordinary.clear(f);
        key.clear(f);
        text.clear(f);
        s.release_i32_local(negative, f);
        acceptable.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(finished, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(check, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        current.replace(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, s, f)
                .reference(),
            f,
        );
        depth.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        depth.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        header.clear(f);
        current.clear(f);
        s.release_i32_local(acceptable, f);
        s.release_i64_local(index, f);
        s.release_i32_local(depth, f);
        Ok(permitted)
    }

    pub(crate) fn emit_try_packed_array_push(
        &mut self,
        receiver: &ValueLocals,
        captured_length: I64Local,
        arguments: &GcLocal<ValueArray>,
        count: I32Local,
        length: I64Local,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        self.emit_with_packed_array_snapshot(
            receiver,
            captured_length,
            f,
            |builder, packed, decline, f| {
                // Zero arguments must still perform ordinary strict Set(length).
                count.load(f);
                f.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(decline, f);
                length.load(f);
                f.instruction(&Instruction::I64Const(u32::MAX as i64));
                f.instruction(&Instruction::I64GtU);
                builder.emit_branch_if_to_target(decline, f);
                s.field(ArrayObjectSchema::LENGTH_WRITABLE)
                    .read(&packed.array, s, f);
                f.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(decline, f);
                let header = s.reserve_gc_local(f).initialize(
                    s.field(ArrayObjectSchema::OBJECT)
                        .read(&packed.array, s, f)
                        .reference(),
                    f,
                );
                let extensible = s.reserve_i32_local(f);
                s.field(OrdinaryObjectSchema::EXTENSIBLE)
                    .read(&header, s, f)
                    .store(extensible, f);
                header.clear(f);
                extensible.load(f);
                f.instruction(&Instruction::I32Eqz);
                s.release_i32_local(extensible, f);
                builder.emit_branch_if_to_target(decline, f);
                // Every canonical Array index is below its length. Thus COUNT ==
                // length proves a contiguous prefix without an O(length) rescan.
                s.field(ArrayIndexStorageSchema::COUNT)
                    .read(&packed.storage, s, f);
                captured_length.load(f);
                f.instruction(&Instruction::I64Ne);
                builder.emit_branch_if_to_target(decline, f);
                let permitted = builder.emit_packed_push_prototypes_permit_append(
                    &packed.array,
                    captured_length,
                    length,
                    f,
                )?;
                permitted.load(f);
                f.instruction(&Instruction::I32Eqz);
                s.release_i32_local(permitted, f);
                builder.emit_branch_if_to_target(decline, f);
                // All semantic guards have completed before the first publication.
                // This is the existing sole indexed growth/publication authority.
                let position = s.reserve_i32_local(f);
                position.set_constant(0, f);
                let index = s.reserve_i64_local(f);
                captured_length.load(f);
                index.store(f);
                let value = s.reserve_value_local(f);
                let finished = builder.open_frame(ControlFrameKind::Block, f);
                let next = builder.open_frame(ControlFrameKind::Loop, f);
                position.load(f);
                count.load(f);
                f.instruction(&Instruction::I32GeU);
                builder.emit_branch_if_to_target(finished, f);
                builder.emit_argument_vector_entry_to_value(arguments, position, &value, f);
                builder.emit_array_indexed_publish_default_data(&packed.array, index, &value, f)?;
                builder.emit_increment_local(index, 1, f);
                s.field(ArrayObjectSchema::LENGTH).write(
                    &packed.array,
                    GcOperand::i64_local(index),
                    s,
                    f,
                );
                position.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Add);
                position.store(f);
                builder.emit_branch_to_target(next, f);
                builder.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                value.clear(f);
                s.release_i64_local(index, f);
                s.release_i32_local(position, f);
                handled.set_constant(1, f);
                Ok(())
            },
        )?;
        Ok(handled)
    }
}
