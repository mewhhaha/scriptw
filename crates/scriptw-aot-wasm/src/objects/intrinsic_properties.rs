//! Complete intrinsic property recipes share immutable R metadata. Observation
//! caches identity in the defining Realm; mutation publishes an ordinary table.
use super::property_storage::PreparedOrdinaryPropertyKey;
use super::*;
use crate::gc_types::*;
use crate::runtime_helpers::*;
mod installation;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_install_intrinsic_property_layout(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        id: crate::intrinsics::IntrinsicPropertyLayoutId,
        realm: &GcLocal<RealmRecord>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let span = self.strings.intrinsic_property_layout_span(id)?;
        installation::emit_intrinsic_layout_storage(span, object, realm, self.runtime_schema(), f)
    }

    pub(super) fn emit_intrinsic_property_row(
        &self,
        storage: &GcLocal<IntrinsicPropertyStorage>,
        index: I32Local,
        f: &mut Function,
    ) -> RuntimeHelperPropertyLayoutRow {
        RuntimeHelperPropertyLayoutRow::from_storage(storage, index, self.runtime_schema(), f)
    }

    pub(crate) fn compile_intrinsic_property_find_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::IntrinsicPropertyFind);
        let p = self.helper_parameters::<IntrinsicPropertyFindParameters>(&mut f);
        let s = self.runtime_schema();
        let count = s.reserve_i32_local(&mut f);
        s.field(IntrinsicPropertyStorageSchema::COUNT)
            .read(&p.storage, s, &mut f)
            .store(count, &mut f);
        let index = s.reserve_i32_local(&mut f);
        let result = s.reserve_i32_local(&mut f);
        index.set_constant(0, &mut f);
        result.set_constant(-1, &mut f);
        let hash = s.reserve_i64_local(&mut f);
        hash.set_constant(0, &mut f);
        let is_string = s.reserve_i32_local(&mut f);
        let bucket = s.reserve_i32_local(&mut f);
        let index_address = s.reserve_i32_local(&mut f);
        let index_mask = s.reserve_i32_local(&mut f);
        s.field(IntrinsicPropertyStorageSchema::INDEX_ADDRESS)
            .read(&p.storage, s, &mut f)
            .store(index_address, &mut f);
        s.field(IntrinsicPropertyStorageSchema::INDEX_MASK)
            .read(&p.storage, s, &mut f)
            .store(index_mask, &mut f);
        p.key.value().tag().load(&mut f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Eq);
        is_string.store(&mut f);
        is_string.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        let text = s.reserve_gc_local(&mut f).initialize(
            p.key.value().cast_reference::<StringValue>(s, &mut f),
            &mut f,
        );
        self.emit_string_key_hash(&text, hash, &mut f);
        text.clear(&mut f);
        hash.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        index_mask.load(&mut f);
        f.instruction(&Instruction::I32And);
        bucket.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let exit = self.open_frame(ControlFrameKind::Block, &mut f);
        let repeat = self.open_frame(ControlFrameKind::Loop, &mut f);
        is_string.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        index_address.load(&mut f);
        bucket.load(&mut f);
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        index.store(&mut f);
        index.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, &mut f);
        index.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        index.store(&mut f);
        f.instruction(&Instruction::Else);
        index.load(&mut f);
        count.load(&mut f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let row = self.emit_intrinsic_property_row(&p.storage, index, &mut f);
        // String hashes are immutable content hashes. Symbol identity hashes
        // are assigned lazily and must never be encoded in static metadata.
        row.key().is_symbol().load(&mut f);
        hash.load(&mut f);
        row.raw_key_hash().load(&mut f);
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, &mut f);
        let actual = PropertyKeyLocals::from_intrinsic_installation_parameter(
            row.key(),
            s,
            self.runtime_helper_base()?,
            &mut f,
        );
        self.emit_tagged_payload_same_value_i32(actual.value(), p.key.value(), &mut f)?;
        self.open_frame(ControlFrameKind::If, &mut f);
        index.load(&mut f);
        result.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        actual.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        row.clear(s, &mut f);
        result.load(&mut f);
        f.instruction(&Instruction::I32Const(-1));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(exit, &mut f);
        is_string.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        bucket.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index_mask.load(&mut f);
        f.instruction(&Instruction::I32And);
        bucket.store(&mut f);
        f.instruction(&Instruction::Else);
        index.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(repeat, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        result.load(&mut f);
        s.release_i32_local(index_mask, &mut f);
        s.release_i32_local(index_address, &mut f);
        s.release_i32_local(bucket, &mut f);
        s.release_i32_local(is_string, &mut f);
        s.release_i64_local(hash, &mut f);
        s.release_i32_local(result, &mut f);
        s.release_i32_local(index, &mut f);
        s.release_i32_local(count, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    pub(crate) fn compile_intrinsic_property_value_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::IntrinsicPropertyValue);
        let p = self.helper_parameters::<IntrinsicPropertyValueParameters>(&mut f);
        let s = self.runtime_schema();
        let row = self.emit_intrinsic_property_row(&p.storage, p.index, &mut f);
        let group = s.reserve_i32_local(&mut f);
        row.group().load(&mut f);
        p.setter.load(&mut f);
        f.instruction(&Instruction::I32Add);
        group.store(&mut f);
        let cache = s.reserve_gc_local(&mut f).initialize(
            s.field(IntrinsicPropertyStorageSchema::CACHE)
                .read(&p.storage, s, &mut f)
                .reference(),
            &mut f,
        );
        cache.load(s, &mut f).is_null(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        let count = s.reserve_i32_local(&mut f);
        s.field(IntrinsicPropertyStorageSchema::GROUP_COUNT)
            .read(&p.storage, s, &mut f)
            .store(count, &mut f);
        let allocated = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<IntrinsicPropertyValueCache>()
                .filled(GcOperand::null(s), count, &mut f),
            &mut f,
        );
        s.field(IntrinsicPropertyStorageSchema::CACHE).write(
            &p.storage,
            GcOperand::nullable_reference(&allocated, s),
            s,
            &mut f,
        );
        cache.replace(allocated.load(s, &mut f).nullable(), &mut f);
        allocated.clear(&mut f);
        s.release_i32_local(count, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let cache_ref = s
            .reserve_gc_local(&mut f)
            .initialize(cache.load(s, &mut f).require_non_null(&mut f), &mut f);
        cache.clear(&mut f);
        let value = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<IntrinsicPropertyValueCache>()
                .read(&cache_ref, group, s, &mut f)
                .reference(),
            &mut f,
        );
        value.load(s, &mut f).is_null(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        let realm = s.reserve_gc_local(&mut f).initialize(
            s.field(IntrinsicPropertyStorageSchema::REALM)
                .read(&p.storage, s, &mut f)
                .reference(),
            &mut f,
        );
        p.setter.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        let setter = self.emit_intrinsic_layout_recipe_value(&row, &realm, true, &mut f)?;
        value.replace(setter.load(s, &mut f).nullable(), &mut f);
        setter.clear(&mut f);
        f.instruction(&Instruction::Else);
        let getter = self.emit_intrinsic_layout_recipe_value(&row, &realm, false, &mut f)?;
        value.replace(getter.load(s, &mut f).nullable(), &mut f);
        getter.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.array_type::<IntrinsicPropertyValueCache>().write(
            &cache_ref,
            group,
            GcOperand::reference(&value, s),
            s,
            &mut f,
        );
        realm.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.load(s, &mut f).require_non_null(&mut f);
        value.clear(&mut f);
        cache_ref.clear(&mut f);
        s.release_i32_local(group, &mut f);
        row.clear(s, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    pub(super) fn emit_intrinsic_property_value(
        &self,
        storage: &GcLocal<IntrinsicPropertyStorage>,
        index: I32Local,
        setter: bool,
        f: &mut Function,
    ) -> Result<GcLocal<PropertyValue>, EmitError> {
        let s = self.runtime_schema();
        let part = s.reserve_i32_local(f);
        part.set_constant(i32::from(setter), f);
        let result = s.reserve_gc_local(f).initialize(
            s.helper_reference_on_stack(s.call_helper(
                IntrinsicPropertyValueArguments::new(storage, index, part),
                self.runtime_helper_base()?,
                f,
            )),
            f,
        );
        s.release_i32_local(part, f);
        Ok(result)
    }

    pub(super) fn emit_intrinsic_own_descriptor(
        &mut self,
        storage: &GcLocal<IntrinsicPropertyStorage>,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor, Nullable>, EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i32_local(f);
        s.call_helper(
            IntrinsicPropertyFindArguments::new(storage, key),
            self.runtime_helper_base()?,
            f,
        )
        .store(index, f);
        let output = s
            .reserve_gc_local::<PropertyDescriptor, Nullable>(f)
            .initialize_null(s, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        let descriptor = self.emit_intrinsic_property_descriptor(storage, index, f)?;
        output.replace(descriptor.load(s, f).nullable(), f);
        descriptor.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(index, f);
        Ok(output)
    }

    pub(super) fn emit_intrinsic_own_data_value(
        &mut self,
        storage: &GcLocal<IntrinsicPropertyStorage>,
        key: &PropertyKeyLocals,
        output: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i32_local(f);
        s.call_helper(
            IntrinsicPropertyFindArguments::new(storage, key),
            self.runtime_helper_base()?,
            f,
        )
        .store(index, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        let row = self.emit_intrinsic_property_row(storage, index, f);
        row.flags().load(f);
        f.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let recipe = self.emit_intrinsic_property_value(storage, index, false, f)?;
        self.emit_property_value_into(&recipe, output, f);
        found.set_constant(1, f);
        recipe.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        row.clear(s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(index, f);
        Ok(())
    }

    pub(super) fn emit_intrinsic_property_descriptor(
        &mut self,
        storage: &GcLocal<IntrinsicPropertyStorage>,
        index: I32Local,
        f: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor>, EmitError> {
        let s = self.runtime_schema();
        let row = self.emit_intrinsic_property_row(storage, index, f);
        let absent = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        let data = s
            .reserve_gc_local(f)
            .initialize(absent.load(s, f).upcast::<PropertyValue>(), f);
        let getter = s.reserve_gc_local(f).initialize(absent.load(s, f), f);
        let setter = s.reserve_gc_local(f).initialize(absent.load(s, f), f);
        row.flags().load(f);
        f.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        for (is_setter, output) in [(false, &getter), (true, &setter)] {
            let recipe = self.emit_intrinsic_property_value(storage, index, is_setter, f)?;
            let resolved = s.helper_reference_on_stack(s.call_helper(
                PropertyValueResolveArguments::new(&recipe),
                self.runtime_helper_base()?,
                f,
            ));
            output.replace(resolved, f);
            recipe.clear(f);
        }
        f.instruction(&Instruction::Else);
        let value = self.emit_intrinsic_property_value(storage, index, false, f)?;
        data.replace(value.load(s, f), f);
        value.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let descriptor = s.reserve_gc_local(f).initialize(
            s.struct_type::<PropertyDescriptor>().construct(
                (
                    GcOperand::intrinsic_layout_flags(&row),
                    GcOperand::reference(&data, s),
                    GcOperand::reference(&getter, s),
                    GcOperand::reference(&setter, s),
                ),
                f,
            ),
            f,
        );
        setter.clear(f);
        getter.clear(f);
        data.clear(f);
        absent.clear(f);
        row.clear(s, f);
        Ok(descriptor)
    }

    pub(super) fn emit_promote_intrinsic_properties(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        storage: &GcLocal<IntrinsicPropertyStorage>,
        f: &mut Function,
    ) -> Result<GcLocal<OrdinaryPropertyStorage>, EmitError> {
        let s = self.runtime_schema();
        let entries = s
            .reserve_gc_local(f)
            .initialize(s.empty_property_table(f), f);
        let buckets = s
            .reserve_gc_local(f)
            .initialize(s.empty_property_index(f), f);
        let dictionary = s.reserve_gc_local(f).initialize(
            s.struct_type::<OrdinaryPropertyStorage>().construct(
                (
                    GcOperand::reference(&entries, s),
                    GcOperand::i32(0),
                    GcOperand::reference(&buckets, s),
                ),
                f,
            ),
            f,
        );
        let count = s.reserve_i32_local(f);
        let index = s.reserve_i32_local(f);
        s.field(IntrinsicPropertyStorageSchema::COUNT)
            .read(storage, s, f)
            .store(count, f);
        index.set_constant(0, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        let row = self.emit_intrinsic_property_row(storage, index, f);
        let key = PropertyKeyLocals::from_intrinsic_installation_parameter(
            row.key(),
            s,
            self.runtime_helper_base()?,
            f,
        );
        let descriptor = self.emit_intrinsic_property_descriptor(storage, index, f)?;
        let hash = s.reserve_i64_local(f);
        self.emit_property_key_hash(&key, hash, f);
        self.emit_ordinary_insert_in_dictionary(
            &dictionary,
            &PreparedOrdinaryPropertyKey { key: &key, hash },
            &descriptor,
            f,
        );
        s.release_i64_local(hash, f);
        descriptor.clear(f);
        key.clear(f);
        row.clear(s, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // No callbacks occur during conversion. Commit the complete graph once.
        s.field(OrdinaryObjectSchema::PROPERTIES).write(
            object,
            GcOperand::subtype_reference(&dictionary, s),
            s,
            f,
        );
        s.release_i32_local(index, f);
        s.release_i32_local(count, f);
        buckets.clear(f);
        entries.clear(f);
        Ok(dictionary)
    }
}
