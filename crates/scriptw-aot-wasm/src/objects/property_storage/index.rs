//! The auxiliary key index never owns observable property order.
use super::*;

/// Fresh queries retain the empty-index shortcut. Publication prepares once.
#[derive(Clone, Copy)]
enum OrdinaryPropertyQuery<'key> {
    Fresh(&'key PropertyKeyLocals),
    Prepared(&'key PreparedOrdinaryPropertyKey<'key>),
    Capture(&'key PreparedOrdinaryPropertyKey<'key>, I32Local),
}

impl OrdinaryPropertyQuery<'_> {
    fn key(&self) -> &PropertyKeyLocals {
        match self {
            Self::Fresh(key) => key,
            Self::Prepared(prepared) | Self::Capture(prepared, _) => prepared.key,
        }
    }
}

impl FunctionBuilder<'_> {
    pub(in crate::objects) fn emit_ordinary_property_entry(
        &self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyEntry, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let entry = schema.helper_reference_on_stack(schema.call_helper(
            crate::runtime_helpers::OrdinaryPropertyFindArguments::new(object, key),
            self.runtime_helper_base()?,
            function,
        ));
        Ok(schema
            .reserve_gc_local(function)
            .initialize(entry, function))
    }

    pub(crate) fn compile_ordinary_property_find_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::OrdinaryPropertyFind);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::OrdinaryPropertyFindParameters>(
                &mut function,
            );
        let entry = self.emit_ordinary_property_find_inner(
            &parameters.object,
            OrdinaryPropertyQuery::Fresh(&parameters.key),
            &mut function,
        )?;
        entry.load(self.runtime_schema(), &mut function);
        entry.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(super) fn emit_prepared_ordinary_property_entry(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        prepared: &PreparedOrdinaryPropertyKey<'_>,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyEntry, Nullable>, EmitError> {
        self.emit_ordinary_property_find_inner(
            object,
            OrdinaryPropertyQuery::Prepared(prepared),
            function,
        )
    }

    pub(super) fn emit_captured_ordinary_property_entry(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        prepared: &PreparedOrdinaryPropertyKey<'_>,
        hash_ready: I32Local,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyEntry, Nullable>, EmitError> {
        self.emit_ordinary_property_find_inner(
            object,
            OrdinaryPropertyQuery::Capture(prepared, hash_ready),
            function,
        )
    }

    fn emit_ordinary_property_find_inner(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        query: OrdinaryPropertyQuery<'_>,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyEntry, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let key = query.key();
        let storage = self.emit_dictionary_property_storage(object, function);
        let entries = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(&storage, schema, function)
                .reference(),
            function,
        );
        let buckets = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryPropertyStorageSchema::INDEX)
                .read(&storage, schema, function)
                .reference(),
            function,
        );
        let output = schema
            .reserve_gc_local::<PropertyEntry, Nullable>(function)
            .initialize_null(schema, function);
        let capacity = schema.reserve_i32_local(function);
        let bucket = schema.reserve_i32_local(function);
        let ordinal = schema.reserve_i32_local(function);
        let position = schema.reserve_i32_local(function);
        let hash = match query {
            OrdinaryPropertyQuery::Fresh(_) => schema.reserve_i64_local(function),
            OrdinaryPropertyQuery::Prepared(prepared)
            | OrdinaryPropertyQuery::Capture(prepared, _) => prepared.hash,
        };
        let actual_hash = schema.reserve_i64_local(function);
        schema
            .array_type::<OrdinaryPropertyIndex>()
            .length(&buckets, schema, function);
        capacity.store(function);
        capacity.load(function);
        self.open_frame(ControlFrameKind::If, function);
        match query {
            OrdinaryPropertyQuery::Fresh(_) => self.emit_property_key_hash(key, hash, function),
            OrdinaryPropertyQuery::Capture(_, hash_ready) => {
                self.emit_property_key_hash(key, hash, function);
                function.instruction(&Instruction::I32Const(1));
                hash_ready.store(function);
            }
            OrdinaryPropertyQuery::Prepared(_) => {}
        }
        Self::emit_ordinary_property_hash_bucket(hash, capacity, bucket, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let repeat = self.open_frame(ControlFrameKind::Loop, function);
        schema
            .array_type::<OrdinaryPropertyIndex>()
            .read(&buckets, bucket, schema, function)
            .store(ordinal, function);
        ordinal.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, function);
        ordinal.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        position.store(function);
        let candidate = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<PropertyTable>()
                .read(&entries, position, schema, function)
                .reference(),
            function,
        );
        candidate.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let entry = schema.reserve_gc_local(function).initialize(
            candidate.load(schema, function).require_non_null(function),
            function,
        );
        schema
            .field(PropertyEntrySchema::HASH)
            .read(&entry, schema, function)
            .store_i64(actual_hash, function);
        hash.load(function);
        actual_hash.load(function);
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .field(PropertyEntrySchema::KEY)
                .read(&entry, schema, function)
                .reference(),
            function,
        );
        let actual = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &actual, schema, function);
        self.emit_tagged_payload_same_value_i32(&actual, key.value(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        output.replace(entry.load(schema, function).nullable(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        actual.clear(function);
        stored.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        entry.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        candidate.clear(function);
        output.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, function);
        Self::emit_ordinary_property_next_bucket(bucket, capacity, function);
        self.emit_branch_to_target(repeat, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(actual_hash, function);
        if matches!(query, OrdinaryPropertyQuery::Fresh(_)) {
            schema.release_i64_local(hash, function);
        }
        schema.release_i32_local(position, function);
        schema.release_i32_local(ordinal, function);
        schema.release_i32_local(bucket, function);
        schema.release_i32_local(capacity, function);
        buckets.clear(function);
        entries.clear(function);
        storage.clear(function);
        Ok(output)
    }

    fn emit_ordinary_property_hash_bucket(
        hash: I64Local,
        capacity: I32Local,
        output: I32Local,
        function: &mut Function,
    ) {
        hash.load(function);
        function.instruction(&Instruction::I32WrapI64);
        capacity.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        function.instruction(&Instruction::I32And);
        output.store(function);
    }

    fn emit_ordinary_property_next_bucket(
        bucket: I32Local,
        capacity: I32Local,
        function: &mut Function,
    ) {
        bucket.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        capacity.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        function.instruction(&Instruction::I32And);
        bucket.store(function);
    }

    /// Every bucket is an insertion position plus one; zero ends a probe.
    /// A position whose ordered entry is null is a reusable tombstone.
    pub(super) fn emit_ordinary_property_index_publish(
        &mut self,
        buckets: &GcLocal<OrdinaryPropertyIndex>,
        entries: &GcLocal<PropertyTable>,
        hash: I64Local,
        position: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let capacity = schema.reserve_i32_local(function);
        let bucket = schema.reserve_i32_local(function);
        let ordinal = schema.reserve_i32_local(function);
        let old_position = schema.reserve_i32_local(function);
        let empty = schema.reserve_i32_local(function);
        schema
            .array_type::<OrdinaryPropertyIndex>()
            .length(buckets, schema, function);
        capacity.store(function);
        Self::emit_ordinary_property_hash_bucket(hash, capacity, bucket, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let repeat = self.open_frame(ControlFrameKind::Loop, function);
        schema
            .array_type::<OrdinaryPropertyIndex>()
            .read(buckets, bucket, schema, function)
            .store(ordinal, function);
        ordinal.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        empty.store(function);
        function.instruction(&Instruction::Else);
        ordinal.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        old_position.store(function);
        let old = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<PropertyTable>()
                .read(entries, old_position, schema, function)
                .reference(),
            function,
        );
        old.load(schema, function).is_null(function);
        empty.store(function);
        old.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        empty.load(function);
        self.open_frame(ControlFrameKind::If, function);
        position.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        ordinal.store(function);
        schema.array_type::<OrdinaryPropertyIndex>().write(
            buckets,
            bucket,
            GcOperand::i32_local(ordinal),
            schema,
            function,
        );
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Self::emit_ordinary_property_next_bucket(bucket, capacity, function);
        self.emit_branch_to_target(repeat, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(empty, function);
        schema.release_i32_local(old_position, function);
        schema.release_i32_local(ordinal, function);
        schema.release_i32_local(bucket, function);
        schema.release_i32_local(capacity, function);
    }
}
