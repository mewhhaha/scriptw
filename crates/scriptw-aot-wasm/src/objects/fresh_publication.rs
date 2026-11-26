//! Direct publication of data properties into a fresh ordinary dictionary.
use super::property_storage::{DictionaryInsertKey, FreshDictionaryCursor};
use super::*;

impl FunctionBuilder<'_> {
    /// The compiler proves the keys distinct and supplies the insertion
    /// positions and buckets. Only the first row supplies a capacity. There
    /// are no observable operations before the last row commits the length.
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
        p.capacity.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&p.storage, s, &mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let table = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<PropertyTable>()
                .filled(GcOperand::null(s), p.capacity, &mut f),
            &mut f,
        );
        let bucket_capacity = s.reserve_i32_local(&mut f);
        p.capacity.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        bucket_capacity.store(&mut f);
        let buckets = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<OrdinaryPropertyIndex>()
                .filled(GcOperand::i32(0), bucket_capacity, &mut f),
            &mut f,
        );
        s.field(OrdinaryPropertyStorageSchema::ENTRIES).write(
            &p.storage,
            GcOperand::reference(&table, s),
            s,
            &mut f,
        );
        s.field(OrdinaryPropertyStorageSchema::INDEX).write(
            &p.storage,
            GcOperand::reference(&buckets, s),
            s,
            &mut f,
        );
        buckets.clear(&mut f);
        table.clear(&mut f);
        s.release_i32_local(bucket_capacity, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        let table = s.reserve_gc_local(&mut f).initialize(
            s.field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(&p.storage, s, &mut f)
                .reference(),
            &mut f,
        );
        let buckets = s.reserve_gc_local(&mut f).initialize(
            s.field(OrdinaryPropertyStorageSchema::INDEX)
                .read(&p.storage, s, &mut f)
                .reference(),
            &mut f,
        );
        self.emit_ordinary_insert_in_dictionary(
            &p.storage,
            p.hash,
            DictionaryInsertKey::Fresh(FreshDictionaryCursor {
                table: &table,
                buckets: &buckets,
                bucket: p.bucket,
                position: p.position,
                string_key: &p.key,
            }),
            &p.descriptor,
            &mut f,
        );
        p.final_length.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        s.field(OrdinaryPropertyStorageSchema::LENGTH).write(
            &p.storage,
            GcOperand::i32_local(p.final_length),
            s,
            &mut f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        buckets.clear(&mut f);
        table.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
