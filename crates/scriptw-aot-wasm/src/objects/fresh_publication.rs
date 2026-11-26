//! First data properties of a fresh plain object, published in one call.
use super::property_storage::{FreshDictionaryCursor, PreparedOrdinaryPropertyKey};
use super::*;
use crate::gc_types::{IntrinsicPropertyValueCache, NumberBitsI64Array, PooledStringTable};

/// The three data attributes of one fresh row, packed as the emitter writes
/// them into the row's flag word.
#[derive(Clone, Copy)]
pub(crate) struct FreshDataAttributes {
    pub(crate) writable: bool,
    pub(crate) enumerable: bool,
    pub(crate) configurable: bool,
}

impl FreshDataAttributes {
    const WRITABLE: i32 = 1;
    const ENUMERABLE: i32 = 2;
    const CONFIGURABLE: i32 = 4;

    pub(crate) fn word(self) -> i32 {
        (if self.writable { Self::WRITABLE } else { 0 })
            | (if self.enumerable { Self::ENUMERABLE } else { 0 })
            | (if self.configurable {
                Self::CONFIGURABLE
            } else {
                0
            })
    }
}

impl FunctionBuilder<'_> {
    /// Publishes `keys[i]` with `values[i]` and the attributes in `flags[i]`,
    /// in index order, into an object that has no property yet. The emitter
    /// that fills the arrays proves the keys distinct, so no row probes for a
    /// previous entry; the dictionary is sized once for every row. A storage
    /// that is not a fresh dictionary, or arrays of unequal length, trap.
    pub(crate) fn compile_fresh_data_properties_publish_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self
            .begin_helper_body(crate::runtime_helpers::RuntimeHelperId::FreshDataPropertiesPublish);
        let p = self
            .helper_parameters::<crate::runtime_helpers::FreshDataPropertiesPublishParameters>(
                &mut f,
            );
        let s = self.runtime_schema();
        let count = s.reserve_i32_local(&mut f);
        s.array_type::<PooledStringTable>()
            .length(&p.keys, s, &mut f);
        count.store(&mut f);
        s.array_type::<IntrinsicPropertyValueCache>()
            .length(&p.values, s, &mut f);
        count.load(&mut f);
        f.instruction(&Instruction::I32Ne);
        s.array_type::<OrdinaryPropertyIndex>()
            .length(&p.flags, s, &mut f);
        count.load(&mut f);
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32Or);
        s.array_type::<NumberBitsI64Array>()
            .length(&p.hashes, s, &mut f);
        count.load(&mut f);
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32Or);
        // The ordered capacity below is a power of two of at most 2^30.
        count.load(&mut f);
        f.instruction(&Instruction::I32Const(1 << 30));
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        let base = s.reserve_gc_local(&mut f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&p.object, s, &mut f)
                .reference(),
            &mut f,
        );
        // A plain header is born with an empty dictionary; anything else traps.
        let storage = s.reserve_gc_local(&mut f).initialize(
            base.load(s, &mut f)
                .cast_subtype::<OrdinaryPropertyStorage>(s, &mut f),
            &mut f,
        );
        base.clear(&mut f);
        let ordered = s.reserve_i32_local(&mut f);
        let buckets_capacity = s.reserve_i32_local(&mut f);
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&storage, s, &mut f)
            .store(ordered, &mut f);
        ordered.load(&mut f);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        // Ordered capacity max(4, 2^ceil(log2(count))) holds every row; the
        // index keeps twice as many buckets, as compaction leaves it.
        count.load(&mut f);
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32LeU);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Const(32));
        count.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Clz);
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::End);
        ordered.store(&mut f);
        ordered.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        buckets_capacity.store(&mut f);
        let bucket_mask = s.reserve_i32_local(&mut f);
        buckets_capacity.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        bucket_mask.store(&mut f);
        let table = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<PropertyTable>()
                .filled(GcOperand::null(s), ordered, &mut f),
            &mut f,
        );
        let buckets = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<OrdinaryPropertyIndex>().filled(
                GcOperand::i32(0),
                buckets_capacity,
                &mut f,
            ),
            &mut f,
        );
        s.field(OrdinaryPropertyStorageSchema::ENTRIES).write(
            &storage,
            GcOperand::reference(&table, s),
            s,
            &mut f,
        );
        s.field(OrdinaryPropertyStorageSchema::INDEX).write(
            &storage,
            GcOperand::reference(&buckets, s),
            s,
            &mut f,
        );
        let absent = s
            .reserve_gc_local(&mut f)
            .initialize(s.stored_undefined(&mut f), &mut f);
        let row = s.reserve_i32_local(&mut f);
        let word = s.reserve_i32_local(&mut f);
        let writable = s.reserve_i32_local(&mut f);
        let enumerable = s.reserve_i32_local(&mut f);
        let configurable = s.reserve_i32_local(&mut f);
        let hash = s.reserve_i64_local(&mut f);
        let dummy_key_str = s
            .reserve_gc_local(&mut f)
            .initialize(self.emit_interned_string_reference("", &mut f)?, &mut f);
        let dummy_key = PropertyKeyLocals::from_string(s, &dummy_key_str, &mut f);
        dummy_key_str.clear(&mut f);
        row.set_constant(0, &mut f);
        let done = self.open_frame(ControlFrameKind::Block, &mut f);
        let next = self.open_frame(ControlFrameKind::Loop, &mut f);
        row.load(&mut f);
        count.load(&mut f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, &mut f);
        let maybe_key = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<PooledStringTable>()
                .read(&p.keys, row, s, &mut f)
                .reference(),
            &mut f,
        );
        let text = s
            .reserve_gc_local(&mut f)
            .initialize(maybe_key.load(s, &mut f).require_non_null(&mut f), &mut f);
        maybe_key.clear(&mut f);
        let maybe_value = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<IntrinsicPropertyValueCache>()
                .read(&p.values, row, s, &mut f)
                .reference(),
            &mut f,
        );
        let value = s
            .reserve_gc_local(&mut f)
            .initialize(maybe_value.load(s, &mut f).require_non_null(&mut f), &mut f);
        maybe_value.clear(&mut f);
        s.array_type::<OrdinaryPropertyIndex>()
            .read(&p.flags, row, s, &mut f)
            .store(word, &mut f);
        word.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32And);
        writable.store(&mut f);
        word.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32And);
        enumerable.store(&mut f);
        word.load(&mut f);
        f.instruction(&Instruction::I32Const(2));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32And);
        configurable.store(&mut f);
        let descriptor = s.reserve_gc_local(&mut f).initialize(
            s.struct_type::<PropertyDescriptor>().construct(
                (
                    GcOperand::data_descriptor_flags(writable, enumerable, configurable),
                    GcOperand::reference(&value, s),
                    GcOperand::reference(&absent, s),
                    GcOperand::reference(&absent, s),
                ),
                &mut f,
            ),
            &mut f,
        );
        value.clear(&mut f);
        // All fresh keys are non-null Strings with compile-time precomputed
        // property key hashes; delegate PropertyEntry creation to
        // emit_ordinary_insert_in_dictionary.
        s.array_type::<NumberBitsI64Array>()
            .read(&p.hashes, row, s, &mut f)
            .store_i64(hash, &mut f);
        let prepared = PreparedOrdinaryPropertyKey {
            key: &dummy_key,
            hash,
        };
        let cursor = FreshDictionaryCursor {
            table: &table,
            buckets: &buckets,
            bucket_mask,
            position: row,
            string_key: &text,
        };
        self.emit_ordinary_insert_in_dictionary(
            &storage,
            &prepared,
            &descriptor,
            Some(&cursor),
            &mut f,
        );
        descriptor.clear(&mut f);
        text.clear(&mut f);
        row.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        row.store(&mut f);
        self.emit_branch_to_target(next, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.field(OrdinaryPropertyStorageSchema::LENGTH).write(
            &storage,
            GcOperand::i32_local(count),
            s,
            &mut f,
        );
        dummy_key.clear(&mut f);
        s.release_i64_local(hash, &mut f);
        s.release_i32_local(configurable, &mut f);
        s.release_i32_local(enumerable, &mut f);
        s.release_i32_local(writable, &mut f);
        s.release_i32_local(word, &mut f);
        s.release_i32_local(row, &mut f);
        absent.clear(&mut f);
        buckets.clear(&mut f);
        table.clear(&mut f);
        s.release_i32_local(bucket_mask, &mut f);
        s.release_i32_local(buckets_capacity, &mut f);
        s.release_i32_local(ordered, &mut f);
        storage.clear(&mut f);
        s.release_i32_local(count, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
