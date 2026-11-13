//! Property and collection keys share content and identity hashing.
use super::*;
use crate::gc_types::{
    CodeUnitArray, GcLocal, GcOperand, I64Local, StringValue, StringValueSchema, SymbolValue,
    SymbolValueSchema,
};

impl FunctionBuilder<'_> {
    pub(crate) fn emit_property_key_hash(
        &mut self,
        key: &PropertyKeyLocals,
        hash: I64Local,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        key.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let text = schema
            .reserve_gc_local(f)
            .initialize(key.value().cast_reference::<StringValue>(schema, f), f);
        self.emit_string_key_hash(&text, hash, f);
        text.clear(f);
        f.instruction(&Instruction::Else);
        let symbol = schema
            .reserve_gc_local(f)
            .initialize(key.value().cast_reference::<SymbolValue>(schema, f), f);
        self.emit_symbol_key_hash(&symbol, hash, f);
        symbol.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_key_hash_tag_mix(key.value(), hash, f);
    }

    pub(crate) fn emit_string_key_hash(
        &mut self,
        text: &GcLocal<StringValue>,
        hash: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        // Published strings have immutable contents. Cache the raw hash only:
        // property and collection callers still apply their own tag mixing.
        s.field(StringValueSchema::RAW_KEY_HASH)
            .read(text, s, f)
            .store_i64(hash, f);
        hash.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(text, s, f)
                .reference(),
            f,
        );
        let index = s.reserve_i32_local(f);
        let count = s.reserve_i32_local(f);
        let unit = s.reserve_i32_local(f);
        s.array_type::<CodeUnitArray>().length(&units, s, f);
        count.store(f);
        f.instruction(&Instruction::I32Const(0));
        index.store(f);
        f.instruction(&Instruction::I64Const(-3750763034362895579));
        hash.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        s.array_type::<CodeUnitArray>()
            .read(&units, index, s, f)
            .store(unit, f);
        hash.load(f);
        unit.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Xor);
        f.instruction(&Instruction::I64Const(1099511628211));
        f.instruction(&Instruction::I64Mul);
        hash.store(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.field(StringValueSchema::RAW_KEY_HASH)
            .write(text, GcOperand::i64_local(hash), s, f);
        s.release_i32_local(unit, f);
        s.release_i32_local(count, f);
        s.release_i32_local(index, f);
        units.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    pub(crate) fn emit_symbol_key_hash(
        &mut self,
        symbol: &GcLocal<SymbolValue>,
        hash: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.field(SymbolValueSchema::KEY_HASH_ID)
            .read(symbol, s, f)
            .store_i64(hash, f);
        hash.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.emit_next_collection_key_hash_id(hash, f);
        s.field(SymbolValueSchema::KEY_HASH_ID)
            .write(symbol, GcOperand::i64_local(hash), s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    pub(crate) fn emit_key_hash_tag_mix(
        &self,
        key: &ValueLocals,
        hash: I64Local,
        f: &mut Function,
    ) {
        hash.load(f);
        key.tag().load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Xor);
        hash.store(f);
        Self::emit_key_hash_finish(hash, f);
    }

    /// The same mix for a key whose tag is known.
    pub(crate) fn emit_key_hash_constant_tag_mix(
        &self,
        tag: WasmRuntimeValueTag,
        hash: I64Local,
        f: &mut Function,
    ) {
        hash.load(f);
        f.instruction(&Instruction::I64Const(i64::from(tag.tag())));
        f.instruction(&Instruction::I64Xor);
        hash.store(f);
        Self::emit_key_hash_finish(hash, f);
    }

    fn emit_key_hash_finish(hash: I64Local, f: &mut Function) {
        for (shift, multiplier) in [(33, -49064778989728563i64), (33, -4265267296055464877i64)] {
            hash.load(f);
            hash.load(f);
            f.instruction(&Instruction::I64Const(shift));
            f.instruction(&Instruction::I64ShrU);
            f.instruction(&Instruction::I64Xor);
            f.instruction(&Instruction::I64Const(multiplier));
            f.instruction(&Instruction::I64Mul);
            hash.store(f);
        }
        hash.load(f);
        hash.load(f);
        f.instruction(&Instruction::I64Const(33));
        f.instruction(&Instruction::I64ShrU);
        f.instruction(&Instruction::I64Xor);
        hash.store(f);
    }
}
