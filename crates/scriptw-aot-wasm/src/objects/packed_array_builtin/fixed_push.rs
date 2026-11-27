//! Fixed native entries borrow the same packed append admission/publication
//! authority as ordinary Array.push, with rooted values in place of an argv.
use super::*;
use crate::gc_types::{
    GcI32Constant, I32Local, Nullable, NumberBitsI64Array, NumberPresenceU64Array,
    PackedValueTable, StoredValue,
};
use crate::runtime_helpers::{
    ArrayPackedPush1Parameters, ArrayPackedPush2Parameters, ArrayPackedPush3Parameters,
};

impl FunctionBuilder<'_> {
    fn emit_fixed_packed_array_push(
        &mut self,
        receiver: &ValueLocals,
        arguments: &[&ValueLocals],
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        debug_assert!(!arguments.is_empty());
        let s = self.runtime_schema();
        // The closed result protocol is Normal(Undefined) on a pure decline,
        // Normal(Number) on publication, and Throw for any future abrupt route.
        result.initialize(f);
        let decline = self.open_frame(ControlFrameKind::Block, f);
        let array = s.reserve_non_null_gc_local(f).initialize(
            receiver.sealed_reference_or_branch::<ArrayObject>(
                s,
                self.local_branch_label(decline),
                f,
            ),
            f,
        );
        let captured_length = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::LENGTH)
            .read(&array, s, f)
            .store_i64(captured_length, f);
        let length = s.reserve_i64_local(f);
        captured_length.load(f);
        f.instruction(&Instruction::I64Const(arguments.len() as i64));
        f.instruction(&Instruction::I64Add);
        length.store(f);
        let mode = s.reserve_i32_local(f);
        s.field(ArrayObjectSchema::MODE)
            .read(&array, s, f)
            .store(mode, f);
        mode.load(f);
        f.instruction(&Instruction::I32Const(
            ArrayIndexedStorageMode::PackedValues.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        mode.load(f);
        f.instruction(&Instruction::I32Const(
            ArrayIndexedStorageMode::PackedNumbers.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        length.load(f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        f.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(decline, f);
        s.field(ArrayObjectSchema::LENGTH_WRITABLE)
            .read(&array, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        let header = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, f)
                .reference(),
            f,
        );
        s.field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(&header, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        // Every canonical Array index is below its length. Thus COUNT ==
        // length proves a contiguous prefix without an O(length) rescan, and
        // guarantees every appended slot >= captured_length is currently empty.
        s.field(ArrayObjectSchema::COUNT).read(&array, s, f);
        captured_length.load(f);
        f.instruction(&Instruction::I64Ne);
        self.emit_branch_if_to_target(decline, f);
        // Check the common untouched %Array.prototype% protector directly from
        // the already-loaded header before falling back to the full walk.
        let proto_ok = self.open_frame(ControlFrameKind::Block, f);
        let proto_walk = self.open_frame(ControlFrameKind::Block, f);
        let prototype = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, s, f)
                .reference(),
            f,
        );
        self.emit_array_prototype_chain_without_indices_or_branch(&prototype, proto_walk, f);
        prototype.clear(f);
        self.emit_branch_to_target(proto_ok, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let permitted = s.reserve_i32_local(f);
        permitted.set_constant(0, f);
        self.emit_packed_push_prototype_walk(&array, captured_length, length, permitted, f)?;
        permitted.load(f);
        f.instruction(&Instruction::I32Eqz);
        s.release_i32_local(permitted, f);
        self.emit_branch_if_to_target(decline, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        header.clear(f);
        // No user code runs after admission and before publication.
        // Fast path: append directly within existing packed capacity without
        // invoking ArrayIndexedPublishDefaultData or checking empty slots.
        let appended = self.open_frame(ControlFrameKind::Block, f);
        let commit = self.open_frame(ControlFrameKind::Block, f);
        mode.load(f);
        f.instruction(&Instruction::I32Const(
            ArrayIndexedStorageMode::PackedValues.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        s.release_i32_local(mode, f);
        self.open_frame(ControlFrameKind::If, f);
        let values = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::VALUES)
                .read(&array, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        length.load(f);
        s.array_type::<PackedValueTable>().length(&values, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LeU);
        self.open_frame(ControlFrameKind::If, f);
        let slot = s.reserve_i32_local(f);
        captured_length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        let stored = s
            .reserve_gc_local::<StoredValue, Nullable>(f)
            .initialize_null(s, f);
        for (offset, value) in arguments.iter().enumerate() {
            if offset > 0 {
                slot.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Add);
                slot.store(f);
            }
            value.tag().load(f);
            f.instruction(&Instruction::I32Const(
                WasmRuntimeValueTag::Undefined as i32,
            ));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            stored.replace(s.stored_undefined(f).nullable(), f);
            f.instruction(&Instruction::Else);
            stored.replace(
                s.struct_type::<StoredValue>()
                    .from_value(value, f)
                    .nullable(),
                f,
            );
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            s.array_type::<PackedValueTable>().write(
                &values,
                slot,
                GcOperand::reference(&stored, s),
                s,
                f,
            );
        }
        stored.clear(f);
        s.release_i32_local(slot, f);
        self.emit_branch_to_target(commit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        values.clear(f);
        f.instruction(&Instruction::Else);
        // PackedNumbers within existing capacity when every argument is a Number.
        for (i, value) in arguments.iter().enumerate() {
            value.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
            f.instruction(&Instruction::I32Eq);
            if i > 0 {
                f.instruction(&Instruction::I32And);
            }
        }
        self.open_frame(ControlFrameKind::If, f);
        let numbers = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::NUMBERS)
                .read(&array, s, f)
                .reference(),
            f,
        );
        length.load(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LeU);
        self.open_frame(ControlFrameKind::If, f);
        let presence = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::NUMBER_PRESENT)
                .read(&array, s, f)
                .reference(),
            f,
        );
        let first = s.reserve_i32_local(f);
        captured_length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        first.store(f);
        let slot = s.reserve_i32_local(f);
        first.load(f);
        slot.store(f);
        for (offset, value) in arguments.iter().enumerate() {
            if offset > 0 {
                slot.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Add);
                slot.store(f);
            }
            s.array_type::<NumberBitsI64Array>().write(
                &numbers,
                slot,
                GcOperand::i64_local(value.scalar()),
                s,
                f,
            );
        }
        self.emit_appended_number_presence(&presence, first, arguments.len() as i32, f);
        s.release_i32_local(slot, f);
        s.release_i32_local(first, f);
        presence.clear(f);
        self.emit_branch_to_target(commit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        numbers.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let index = s.reserve_i64_local(f);
        captured_length.load(f);
        index.store(f);
        for value in arguments {
            self.emit_array_indexed_publish_default_data(&array, index, value, f)?;
            self.emit_increment_local(index, 1, f);
            s.field(ArrayObjectSchema::LENGTH)
                .write(&array, GcOperand::i64_local(index), s, f);
        }
        s.release_i64_local(index, f);
        self.emit_branch_to_target(appended, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.field(ArrayObjectSchema::COUNT)
            .write(&array, GcOperand::i64_local(length), s, f);
        s.field(ArrayObjectSchema::LENGTH)
            .write(&array, GcOperand::i64_local(length), s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        length.store(f);
        result.value().set_number(length, f);
        result.set_normal(result.value(), f);
        s.release_i64_local(length, f);
        s.release_i64_local(captured_length, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        Ok(())
    }

    /// Publish one contiguous append in one bitmap update, with a second
    /// update only when the append crosses the 64-bit word boundary.
    fn emit_appended_number_presence(
        &mut self,
        presence: &GcLocal<NumberPresenceU64Array>,
        first: I32Local,
        count: i32,
        f: &mut Function,
    ) {
        debug_assert!((1..=3).contains(&count));
        let s = self.runtime_schema();
        let words = s.array_type::<NumberPresenceU64Array>();
        let word = s.reserve_i32_local(f);
        first.load(f);
        f.instruction(&Instruction::I32Const(6));
        f.instruction(&Instruction::I32ShrU);
        word.store(f);
        let mask = (1_i64 << count) - 1;
        let bits = s.reserve_i64_local(f);
        words.read(presence, word, s, f);
        f.instruction(&Instruction::I64Const(mask));
        first.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        // i64.shl masks the count modulo the bitmap word width.
        f.instruction(&Instruction::I64Shl);
        f.instruction(&Instruction::I64Or);
        bits.store(f);
        words.write(presence, word, GcOperand::i64_local(bits), s, f);

        if count > 1 {
            let spill_shift = s.reserve_i32_local(f);
            f.instruction(&Instruction::I32Const(64));
            first.load(f);
            f.instruction(&Instruction::I32Const(63));
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::I32Sub);
            spill_shift.store(f);
            spill_shift.load(f);
            f.instruction(&Instruction::I32Const(count));
            f.instruction(&Instruction::I32LtU);
            self.open_frame(ControlFrameKind::If, f);
            word.load(f);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Add);
            word.store(f);
            words.read(presence, word, s, f);
            f.instruction(&Instruction::I64Const(mask));
            spill_shift.load(f);
            f.instruction(&Instruction::I64ExtendI32U);
            f.instruction(&Instruction::I64ShrU);
            f.instruction(&Instruction::I64Or);
            bits.store(f);
            words.write(presence, word, GcOperand::i64_local(bits), s, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            s.release_i32_local(spill_shift, f);
        }
        s.release_i64_local(bits, f);
        s.release_i32_local(word, f);
    }

    pub(crate) fn compile_array_packed_push1_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::ArrayPackedPush1);
        let p = self.helper_parameters::<ArrayPackedPush1Parameters>(&mut f);
        let result = self.runtime_schema().reserve_completion(&mut f);
        self.emit_fixed_packed_array_push(&p.receiver, &[&p.first], &result, &mut f)?;
        result.emit(&mut f);
        result.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    pub(crate) fn compile_array_packed_push2_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::ArrayPackedPush2);
        let p = self.helper_parameters::<ArrayPackedPush2Parameters>(&mut f);
        let result = self.runtime_schema().reserve_completion(&mut f);
        self.emit_fixed_packed_array_push(&p.receiver, &[&p.first, &p.second], &result, &mut f)?;
        result.emit(&mut f);
        result.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    pub(crate) fn compile_array_packed_push3_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::ArrayPackedPush3);
        let p = self.helper_parameters::<ArrayPackedPush3Parameters>(&mut f);
        let result = self.runtime_schema().reserve_completion(&mut f);
        self.emit_fixed_packed_array_push(
            &p.receiver,
            &[&p.first, &p.second, &p.third],
            &result,
            &mut f,
        )?;
        result.emit(&mut f);
        result.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
