//! One live output cursor for packed Arrays and compact ordinary objects.
//! The fast loop calls no JavaScript. At an accessor, hole, changed compact
//! shape, or unsupported child, it publishes its cursor and completes just
//! that member. The continuation retains the fetched value or the exact
//! unread key, the parent's original extent, and the active ancestor path.
use super::*;
use crate::builtins::json::stringify::JSON_INITIAL_PATH_CAPACITY;

struct JsonPrototypeWitnesses {
    object: GcLocal<OrdinaryObject, Nullable>,
    array: GcLocal<ArrayObject, Nullable>,
}

struct JsonContainerSnapshot {
    // Null denotes an Array. Object shapes preserve the original key order
    // even if a descendant subsequently changes the object's properties.
    shape: GcLocal<CompactDataShape, Nullable>,
    storage: GcLocal<CompactDataStorage, Nullable>,
    array_values: GcLocal<PackedValueTable, Nullable>,
    extent: I32Local,
}

impl JsonContainerSnapshot {
    fn new(s: &RuntimeSchema, f: &mut Function) -> Self {
        Self {
            shape: s.reserve_gc_local(f).initialize_null(s, f),
            storage: s.reserve_gc_local(f).initialize_null(s, f),
            array_values: s.reserve_gc_local(f).initialize_null(s, f),
            extent: s.reserve_i32_local(f),
        }
    }

    fn copy_from(&self, other: &Self, s: &RuntimeSchema, f: &mut Function) {
        self.shape.replace(other.shape.load(s, f), f);
        self.storage.replace(other.storage.load(s, f), f);
        self.array_values.replace(other.array_values.load(s, f), f);
        other.extent.load(f);
        self.extent.store(f);
    }

    fn clear(self, s: &RuntimeSchema, f: &mut Function) {
        self.shape.clear(f);
        self.storage.clear(f);
        self.array_values.clear(f);
        s.release_i32_local(self.extent, f);
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_json_plain_traversal_or_branch(
        &mut self,
        value: &ValueLocals,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        decline: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.emit_json_tag_test(value, WasmRuntimeValueTag::Array, f);
        self.emit_json_tag_test(value, WasmRuntimeValueTag::Object, f);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        indent.load(f);
        f.instruction(&Instruction::I64Const(i64::from(
            JSON_INITIAL_PATH_CAPACITY,
        )));
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(decline, f);
        let prototypes = JsonPrototypeWitnesses {
            object: s.reserve_gc_local(f).initialize_null(s, f),
            array: s.reserve_gc_local(f).initialize_null(s, f),
        };
        self.emit_json_refresh_prototype_witnesses(context, &prototypes, f);
        let current = JsonContainerSnapshot::new(s, f);
        self.emit_json_capture_container(value, &prototypes, &current, decline, f);
        let candidate = JsonContainerSnapshot::new(s, f);
        let output_owner = s.reserve_gc_local(f).initialize(
            s.field(JsonStringifyContextSchema::OUTPUT)
                .read(context, s, f)
                .reference(),
            f,
        );
        let path = s.reserve_gc_local(f).initialize(
            s.field(JsonStringifyContextSchema::STACK)
                .read(context, s, f)
                .reference(),
            f,
        );
        let start_depth = s.reserve_i32_local(f);
        let depth = s.reserve_i32_local(f);
        indent.load(f);
        f.instruction(&Instruction::I32WrapI64);
        start_depth.store(f);
        start_depth.load(f);
        depth.store(f);
        // The guard above confines the walk to the context's preallocated
        // prefix. There is enough room to publish this entry without growth.
        self.emit_json_cycle_check(value, context, &path, depth, f)?;
        s.array_type::<JsonStringifyStack>().write(
            &path,
            depth,
            GcOperand::eq_reference(value.reference()),
            s,
            f,
        );
        let continuations = s
            .reserve_gc_local::<NumberBitsI64Array, Nullable>(f)
            .initialize_null(s, f);
        let shapes = s
            .reserve_gc_local::<CompactShapeTable, Nullable>(f)
            .initialize_null(s, f);
        let saved = s.reserve_i64_local(f);
        let container = s.reserve_value_local(f);
        container.copy_from(value, f);
        let entry = s.reserve_value_local(f);
        let key = s.reserve_value_local(f);
        let result = s.reserve_value_local(f);
        let index = s.reserve_i32_local(f);
        index.set_constant(0, f);
        let child_depth = s.reserve_i32_local(f);
        let emitted = s.reserve_i32_local(f);
        emitted.set_constant(0, f);
        let opening = s.reserve_i32_local(f);
        opening.set_constant(1, f);
        let complete = s.reserve_i32_local(f);
        complete.set_constant(0, f);
        let needs_get = s.reserve_i32_local(f);
        let fetched = s.reserve_i32_local(f);
        fetched.set_constant(0, f);
        let mark = s.reserve_i32_local(f);

        let done = self.open_frame(ControlFrameKind::Block, f);
        let resume = self.open_frame(ControlFrameKind::Loop, f);
        let output = StringAccumulatorCursor::open(&output_owner, s, f);
        let suspended = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        opening.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_container_delimiter(&current, &output_owner, &output, b'[', b'{', f);
        opening.set_constant(0, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        index.load(f);
        current.extent.load(f);
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_container_delimiter(&current, &output_owner, &output, b']', b'}', f);
        complete.set_constant(1, f);
        depth.load(f);
        start_depth.load(f);
        f.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(suspended, f);
        complete.set_constant(0, f);
        depth.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        depth.store(f);
        s.array_type::<JsonStringifyStack>()
            .read(&path, depth, s, f)
            .store_eq(container.reference(), f);
        current.shape.replace(
            s.array_type::<CompactShapeTable>()
                .read(&shapes, depth, s, f)
                .reference(),
            f,
        );
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Array as i32));
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Object as i32));
        current.shape.load(s, f).is_null(f);
        f.instruction(&Instruction::Select);
        container.tag().store(f);
        self.emit_json_refresh_container_storage(&container, &current, f);
        s.array_type::<NumberBitsI64Array>()
            .read(&continuations, depth, s, f)
            .store_i64(saved, f);
        saved.load(f);
        f.instruction(&Instruction::I32WrapI64);
        index.store(f);
        saved.load(f);
        f.instruction(&Instruction::I64Const(32));
        f.instruction(&Instruction::I64ShrU);
        f.instruction(&Instruction::I32WrapI64);
        current.extent.store(f);
        // The accepted child just appended a value for this member.
        emitted.set_constant(1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        fetched.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        needs_get.set_constant(1, f);
        self.emit_json_traversal_member(&container, &current, index, &key, &entry, suspended, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        fetched.set_constant(0, f);
        needs_get.set_constant(0, f);

        let advance = self.open_frame(ControlFrameKind::Block, f);
        self.emit_json_tag_test(&entry, WasmRuntimeValueTag::Undefined, f);
        self.emit_json_tag_test(&entry, WasmRuntimeValueTag::Symbol, f);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        current.shape.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_array_separator(emitted, &output_owner, &output, f);
        output.push_literal(&output_owner, "null", s, f);
        emitted.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(advance, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        output.length().load(f);
        mark.store(f);
        current.shape.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_array_separator(emitted, &output_owner, &output, f);
        f.instruction(&Instruction::Else);
        let name = s
            .reserve_gc_local(f)
            .initialize(key.cast_reference::<StringValue>(s, f), f);
        self.emit_json_quote_into_cursor(&output_owner, &output, &name, Some(emitted), f);
        name.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_serializable_primitive_i32(&entry, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_serialize_primitive_into_cursor(&entry, &output_owner, &output, f)?;
        emitted.set_constant(1, f);
        self.emit_branch_to_target(advance, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        // The ordinary serializer owns paths beyond the initial capacity.
        // The hot scan therefore compares at most seven active ancestors.
        Self::emit_json_next_i32(depth, child_depth, f);
        child_depth.load(f);
        f.instruction(&Instruction::I32Const(JSON_INITIAL_PATH_CAPACITY));
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(suspended, f);
        self.emit_json_capture_container(&entry, &prototypes, &candidate, suspended, f);
        self.emit_json_cycle_check(&entry, context, &path, child_depth, f)?;

        continuations.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let count = s.reserve_i32_local(f);
        count.set_constant(JSON_INITIAL_PATH_CAPACITY, f);
        continuations.replace(
            s.array_type::<NumberBitsI64Array>()
                .filled(GcOperand::i64(0), count, f)
                .nullable(),
            f,
        );
        shapes.replace(
            s.array_type::<CompactShapeTable>()
                .filled(GcOperand::null(s), count, f)
                .nullable(),
            f,
        );
        s.release_i32_local(count, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.array_type::<CompactShapeTable>().write(
            &shapes,
            depth,
            GcOperand::reference(&current.shape, s),
            s,
            f,
        );
        current.extent.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Const(32));
        f.instruction(&Instruction::I64Shl);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Or);
        saved.store(f);
        s.array_type::<NumberBitsI64Array>().write(
            &continuations,
            depth,
            GcOperand::i64_local(saved),
            s,
            f,
        );
        s.array_type::<JsonStringifyStack>().write(
            &path,
            child_depth,
            GcOperand::eq_reference(entry.reference()),
            s,
            f,
        );
        child_depth.load(f);
        depth.store(f);
        current.copy_from(&candidate, s, f);
        container.copy_from(&entry, f);
        index.set_constant(0, f);
        emitted.set_constant(0, f);
        opening.set_constant(1, f);
        self.emit_branch_to_target(next, f);

        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        Self::emit_json_next_i32(index, index, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        output.close(&output_owner, s, f);

        complete.load(f);
        self.emit_branch_if_to_target(done, f);
        current.shape.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        Self::emit_json_traversal_index_key(index, &key, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        needs_get.load(f);
        self.open_frame(ControlFrameKind::If, f);
        // The key is qualified by the original extent or shape. A Get can
        // execute arbitrary JS; consume its result before any further Get.
        let name = s
            .reserve_gc_local::<StringValue, Nullable>(f)
            .initialize_null(s, f);
        current.shape.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        name.replace(self.emit_json_index_string(key.scalar(), f)?.nullable(), f);
        f.instruction(&Instruction::Else);
        name.replace(key.cast_reference::<StringValue>(s, f).nullable(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let name_for_get = s
            .reserve_gc_local(f)
            .initialize(name.load(s, f).require_non_null(f), f);
        self.emit_json_get(&container, &name_for_get, &entry, f)?;
        name_for_get.clear(f);
        name.clear(f);
        fetched.set_constant(1, f);
        f.instruction(&Instruction::Else);
        let nested_indent = s.reserve_i64_local(f);
        Self::emit_json_next_i32(depth, child_depth, f);
        child_depth.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        nested_indent.store(f);
        let already_fetched = s.reserve_i32_local(f);
        already_fetched.set_constant(1, f);
        self.emit_json_stringify_call(
            &entry,
            &key,
            already_fetched,
            context,
            nested_indent,
            &result,
            f,
        )?;
        s.release_i32_local(already_fetched, f);
        s.release_i64_local(nested_indent, f);
        self.emit_json_tag_test(&result, WasmRuntimeValueTag::Undefined, f);
        self.open_frame(ControlFrameKind::If, f);
        let output = StringAccumulatorCursor::open(&output_owner, s, f);
        current.shape.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        output.push_literal(&output_owner, "null", s, f);
        emitted.set_constant(1, f);
        f.instruction(&Instruction::Else);
        output.truncate(mark, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        output.close(&output_owner, s, f);
        f.instruction(&Instruction::Else);
        emitted.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Self::emit_json_next_i32(index, index, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // A general continuation may have grown the shared path. Either
        // opaque edge may have changed the intrinsics or their rows.
        path.replace(
            s.field(JsonStringifyContextSchema::STACK)
                .read(context, s, f)
                .reference(),
            f,
        );
        self.emit_json_refresh_prototype_witnesses(context, &prototypes, f);
        self.emit_json_refresh_container_storage(&container, &current, f);
        self.emit_branch_to_target(resume, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);

        for local in [
            mark,
            fetched,
            needs_get,
            complete,
            opening,
            emitted,
            child_depth,
            depth,
            start_depth,
            index,
        ] {
            s.release_i32_local(local, f);
        }
        for value in [result, key, entry, container] {
            value.clear(f);
        }
        s.release_i64_local(saved, f);
        shapes.clear(f);
        continuations.clear(f);
        path.clear(f);
        output_owner.clear(f);
        candidate.clear(s, f);
        current.clear(s, f);
        prototypes.array.clear(f);
        prototypes.object.clear(f);
        Ok(())
    }

    fn emit_json_refresh_prototype_witnesses(
        &mut self,
        context: &GcLocal<JsonStringifyContext>,
        witnesses: &JsonPrototypeWitnesses,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        witnesses.object.set_null(s, f);
        witnesses.array.set_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let realm = s.reserve_gc_local(f).initialize(
            s.field(JsonStringifyContextSchema::REALM)
                .read(context, s, f)
                .reference(),
            f,
        );
        let protectors = s.reserve_gc_local(f).initialize(
            s.field(RealmRecordSchema::PROTECTORS)
                .read(&realm, s, f)
                .reference()
                .non_null_or_branch(self.local_branch_label(done), f),
            f,
        );
        realm.clear(f);
        let object = s.reserve_gc_local(f).initialize(
            s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
                .read(&protectors, s, f)
                .reference(),
            f,
        );
        s.field(OrdinaryObjectSchema::PROPERTIES)
            .read(&object, s, f);
        s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE_ROWS)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, f);
        witnesses.object.replace(object.load(s, f).nullable(), f);
        let array = s.reserve_gc_local(f).initialize(
            s.field(RealmProtectorsSchema::ARRAY_PROTOTYPE)
                .read(&protectors, s, f)
                .reference(),
            f,
        );
        let header = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, f)
                .reference(),
            f,
        );
        s.field(OrdinaryObjectSchema::PROPERTIES)
            .read(&header, s, f);
        s.field(RealmProtectorsSchema::ARRAY_PROTOTYPE_ROWS)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        s.field(OrdinaryObjectSchema::PROTOTYPE).read(&header, s, f);
        object.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, f);
        witnesses.array.replace(array.load(s, f).nullable(), f);
        header.clear(f);
        array.clear(f);
        object.clear(f);
        protectors.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }

    fn emit_json_capture_container(
        &mut self,
        value: &ValueLocals,
        prototypes: &JsonPrototypeWitnesses,
        snapshot: &JsonContainerSnapshot,
        decline: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let accepted = self.open_frame(ControlFrameKind::Block, f);
        let ordinary = self.open_frame(ControlFrameKind::Block, f);
        let array = s.reserve_gc_local(f).initialize(
            value.sealed_reference_or_branch::<ArrayObject>(
                s,
                self.local_branch_label(ordinary),
                f,
            ),
            f,
        );
        s.array_storage_is_packed(&array, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        let length = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::LENGTH)
            .read(&array, s, f)
            .store_i64(length, f);
        length.load(f);
        f.instruction(&Instruction::I64Const(i64::from(i32::MAX)));
        f.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(decline, f);
        let header = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, f)
                .reference(),
            f,
        );
        s.field(OrdinaryObjectSchema::PROTOTYPE).read(&header, s, f);
        prototypes.array.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        let properties = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&header, s, f)
                .reference(),
            f,
        );
        let own_proved = self.open_frame(ControlFrameKind::Block, f);
        let compact = self.open_frame(ControlFrameKind::Block, f);
        let dictionary = s.reserve_gc_local(f).initialize(
            properties.sealed_subtype_or_branch::<OrdinaryPropertyStorage>(
                s,
                self.local_branch_label(compact),
                f,
            ),
            f,
        );
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&dictionary, s, f);
        self.emit_branch_if_to_target(decline, f);
        dictionary.clear(f);
        self.emit_branch_to_target(own_proved, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let storage = self.emit_json_compact_storage_or_branch(&properties, decline, f);
        let shape = s.reserve_gc_local(f).initialize(
            s.field(CompactDataStorageSchema::SHAPE)
                .read(&storage, s, f)
                .reference(),
            f,
        );
        s.field(CompactDataShapeSchema::LACKS_TO_JSON)
            .read(&shape, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        shape.clear(f);
        storage.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        properties.clear(f);
        header.clear(f);
        length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        snapshot.extent.store(f);
        s.release_i64_local(length, f);
        snapshot.shape.set_null(s, f);
        snapshot.storage.set_null(s, f);
        self.emit_json_capture_array_values(&array, snapshot, f);
        array.clear(f);
        self.emit_branch_to_target(accepted, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);

        let header = s.reserve_gc_local(f).initialize(
            value.sealed_reference_or_branch::<OrdinaryObject>(
                s,
                self.local_branch_label(decline),
                f,
            ),
            f,
        );
        s.field(OrdinaryObjectSchema::PROTOTYPE).read(&header, s, f);
        prototypes.object.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        let properties = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&header, s, f)
                .reference(),
            f,
        );
        let storage = self.emit_json_compact_storage_or_branch(&properties, decline, f);
        let shape = s.reserve_gc_local(f).initialize(
            s.field(CompactDataStorageSchema::SHAPE)
                .read(&storage, s, f)
                .reference(),
            f,
        );
        s.field(CompactDataShapeSchema::LACKS_TO_JSON)
            .read(&shape, s, f);
        s.field(CompactDataShapeSchema::OWN_KEYS_ORDERED)
            .read(&shape, s, f);
        f.instruction(&Instruction::I32And);
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read(&shape, s, f);
        let attributes = crate::heap::StoredPropertyAttributes::Data {
            writable: true,
            enumerable: true,
            configurable: true,
        }
        .descriptor_word()
        .as_i32();
        f.instruction(&Instruction::I32Const(attributes));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        let keys = s
            .field(CompactDataShapeSchema::KEYS)
            .read(&shape, s, f)
            .reference();
        s.array_type::<CompactShapeKeys>().length_top(keys, f);
        snapshot.extent.store(f);
        snapshot.storage.replace(storage.load(s, f).nullable(), f);
        snapshot.shape.replace(shape.load(s, f).nullable(), f);
        snapshot.array_values.set_null(s, f);
        shape.clear(f);
        storage.clear(f);
        properties.clear(f);
        header.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }

    fn emit_json_compact_storage_or_branch(
        &mut self,
        properties: &GcLocal<ObjectPropertyStorage>,
        decline: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<CompactDataStorage> {
        let s = self.runtime_schema();
        properties.is_subtype::<CompactDataStorage>(s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(decline, f);
        s.reserve_gc_local(f).initialize(
            properties
                .load(s, f)
                .cast_subtype::<CompactDataStorage>(s, f),
            f,
        )
    }

    fn emit_json_capture_array_values(
        &mut self,
        array: &GcLocal<ArrayObject>,
        current: &JsonContainerSnapshot,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        current.array_values.set_null(s, f);
        s.array_storage_mode_is(array, ArrayIndexedStorageMode::PackedValues, f);
        self.open_frame(ControlFrameKind::If, f);
        current.array_values.replace(
            s.field(ArrayObjectSchema::VALUES)
                .read(array, s, f)
                .reference(),
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    fn emit_json_refresh_container_storage(
        &mut self,
        container: &ValueLocals,
        current: &JsonContainerSnapshot,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        current.storage.set_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        current.shape.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(container.cast_reference::<ArrayObject>(s, f), f);
        self.emit_json_capture_array_values(&array, current, f);
        array.clear(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        current.array_values.set_null(s, f);
        let header = s
            .reserve_gc_local(f)
            .initialize(container.cast_reference::<OrdinaryObject>(s, f), f);
        let properties = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&header, s, f)
                .reference(),
            f,
        );
        let storage = self.emit_json_compact_storage_or_branch(&properties, done, f);
        s.field(CompactDataStorageSchema::SHAPE)
            .read(&storage, s, f);
        current.shape.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, f);
        current.storage.replace(storage.load(s, f).nullable(), f);
        storage.clear(f);
        properties.clear(f);
        header.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }

    fn emit_json_traversal_member(
        &mut self,
        container: &ValueLocals,
        snapshot: &JsonContainerSnapshot,
        index: I32Local,
        key: &ValueLocals,
        entry: &ValueLocals,
        needs_get: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        snapshot.shape.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let read = self.open_frame(ControlFrameKind::Block, f);
        let probe = self.open_frame(ControlFrameKind::Block, f);
        let values = s.reserve_gc_local(f).initialize(
            snapshot
                .array_values
                .non_null_or_branch(self.local_branch_label(probe), f),
            f,
        );
        index.load(f);
        s.array_type::<PackedValueTable>().length(&values, s, f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(needs_get, f);
        let element = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>()
                .read(&values, index, s, f)
                .reference()
                .non_null_or_branch(self.local_branch_label(needs_get), f),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&element, entry, s, f);
        element.clear(f);
        values.clear(f);
        self.emit_branch_to_target(read, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let array = s
            .reserve_gc_local(f)
            .initialize(container.cast_reference::<ArrayObject>(s, f), f);
        let index64 = s.reserve_i64_local(f);
        index.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        index64.store(f);
        let found = s.reserve_i32_local(f);
        self.emit_array_own_indexed_value(&array, index64, entry, found, f);
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(needs_get, f);
        s.release_i32_local(found, f);
        s.release_i64_local(index64, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        let shape = s
            .reserve_gc_local(f)
            .initialize(snapshot.shape.load(s, f).require_non_null(f), f);
        let keys = s
            .field(CompactDataShapeSchema::KEYS)
            .read(&shape, s, f)
            .reference();
        let name = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeKeys>()
                .read_top(keys, index, f)
                .reference(),
            f,
        );
        key.set_reference(&name, s, f);
        name.clear(f);
        shape.clear(f);
        let header = s
            .reserve_gc_local(f)
            .initialize(container.cast_reference::<OrdinaryObject>(s, f), f);
        let storage = s.reserve_gc_local(f).initialize(
            snapshot
                .storage
                .non_null_or_branch(self.local_branch_label(needs_get), f),
            f,
        );
        self.emit_plain_data_header_value_or_branch(&header, &storage, index, entry, needs_get, f);
        storage.clear(f);
        header.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    fn emit_json_container_delimiter(
        &mut self,
        snapshot: &JsonContainerSnapshot,
        owner: &GcLocal<StringAccumulator>,
        output: &StringAccumulatorCursor,
        array: u8,
        object: u8,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let unit = s.reserve_i32_local(f);
        unit.set_constant(1, f);
        output.reserve(owner, unit, s, f);
        f.instruction(&Instruction::I32Const(i32::from(array)));
        f.instruction(&Instruction::I32Const(i32::from(object)));
        snapshot.shape.load(s, f).is_null(f);
        f.instruction(&Instruction::Select);
        unit.store(f);
        output.push(unit, s, f);
        s.release_i32_local(unit, f);
    }

    fn emit_json_array_separator(
        &mut self,
        emitted: I32Local,
        accumulator: &GcLocal<StringAccumulator>,
        output: &StringAccumulatorCursor,
        f: &mut Function,
    ) {
        emitted.load(f);
        self.open_frame(ControlFrameKind::If, f);
        output.push_literal(accumulator, ",", self.runtime_schema(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    fn emit_json_traversal_index_key(index: I32Local, key: &ValueLocals, f: &mut Function) {
        index.load(f);
        f.instruction(&Instruction::F64ConvertI32U);
        f.instruction(&Instruction::I64ReinterpretF64);
        key.scalar().store(f);
        key.set_number(key.scalar(), f);
    }

    fn emit_json_next_i32(from: I32Local, to: I32Local, f: &mut Function) {
        from.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        to.store(f);
    }
}
