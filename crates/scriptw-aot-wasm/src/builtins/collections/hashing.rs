use super::*;
impl FunctionBuilder<'_> {
    // Equal primitive keys have equal content hashes. Object/Symbol identity
    // stays in a mutable GC field and never becomes a reference address.
    pub(super) fn emit_collection_key_hash(
        &mut self,
        key: &ValueLocals,
        hash: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        key.scalar().load(f);
        hash.store(f);
        key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let text = s
            .reserve_gc_local(f)
            .initialize(key.cast_reference::<StringValue>(s, f), f);
        self.emit_string_key_hash(&text, hash, f);
        text.clear(f);
        f.instruction(&Instruction::Else);
        key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        hash.load(f);
        f.instruction(&Instruction::I64Const(i64::MAX));
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(0));
        hash.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Ne);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(0x7ff8_0000_0000_0000));
        hash.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        self.emit_is_heap_object_like_tag_i32(key.tag(), f);
        self.open_frame(ControlFrameKind::If, f);
        let header = s
            .reserve_gc_local(f)
            .initialize(self.emit_object_header_projection(key, f), f);
        s.struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::KEY_HASH_ID)
            .read(&header, s, f)
            .store_i64(hash, f);
        hash.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.emit_next_collection_key_hash_id(hash, f);
        s.struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::KEY_HASH_ID)
            .write(&header, GcOperand::i64_local(hash), s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        header.clear(f);
        f.instruction(&Instruction::Else);
        key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let symbol = s
            .reserve_gc_local(f)
            .initialize(key.cast_reference::<SymbolValue>(s, f), f);
        self.emit_symbol_key_hash(&symbol, hash, f);
        symbol.clear(f);
        f.instruction(&Instruction::Else);
        key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::BigInt.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let bigint = s
            .reserve_gc_local(f)
            .initialize(key.cast_reference::<BigIntValue>(s, f), f);
        let limbs = s.reserve_gc_local(f).initialize(
            s.struct_type::<BigIntValue>()
                .field(BigIntValueSchema::LIMBS)
                .read(&bigint, s, f)
                .reference(),
            f,
        );
        let index = s.reserve_i32_local(f);
        let count = s.reserve_i32_local(f);
        let negative = s.reserve_i32_local(f);
        let limb = s.reserve_i64_local(f);
        s.struct_type::<BigIntValue>()
            .field(BigIntValueSchema::NEGATIVE)
            .read(&bigint, s, f)
            .store(negative, f);
        negative.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        hash.store(f);
        s.array_type::<BigIntLimbArray>().length(&limbs, s, f);
        count.store(f);
        f.instruction(&Instruction::I32Const(0));
        index.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        s.array_type::<BigIntLimbArray>()
            .read(&limbs, index, s, f)
            .store_i64(limb, f);
        hash.load(f);
        limb.load(f);
        f.instruction(&Instruction::I64Xor);
        f.instruction(&Instruction::I64Const(1099511628211));
        f.instruction(&Instruction::I64Mul);
        hash.store(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(limb, f);
        s.release_i32_local(negative, f);
        s.release_i32_local(count, f);
        s.release_i32_local(index, f);
        limbs.clear(f);
        bigint.clear(f);
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
        self.emit_key_hash_tag_mix(key, hash, f);
    }
    pub(super) fn emit_collection_hash_bucket(
        &self,
        table: &GcLocal<CollectionHashTable>,
        hash: I64Local,
        index: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        // Every CollectionHashTable is allocated with a power-of-two bucket
        // count (4 initially, rounded up to a power of two on rehash).
        hash.load(f);
        f.instruction(&Instruction::I32WrapI64);
        s.array_type::<CollectionHashTable>().length(table, s, f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32And);
        index.store(f);
    }
}
