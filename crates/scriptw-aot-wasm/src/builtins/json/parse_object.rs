//! Private JSON records can publish compact storage in one write.
use super::parse_frame_state::{
    JSON_OBJECT_FIELDS, JSON_OBJECT_FRAME_STRIDE, JsonParseObjectScratch, JsonParseObjectWindow,
};
use super::*;

impl FunctionBuilder<'_> {
    /// A fresh extensible ordinary object inheriting %Object.prototype%,
    /// born on the shared empty compact storage like `{}`.
    pub(super) fn emit_json_fresh_object(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        f: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let s = self.runtime_schema();
        let prototype = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &prototype,
            f,
        );
        let object = s.reserve_gc_local(f).initialize(
            self.emit_alloc_compact_object_with_prototype(&prototype, f)?,
            f,
        );
        prototype.clear(f);
        Ok(object)
    }

    /// CreateDataProperty(container, key, value) for the object the parser
    /// created with `emit_json_fresh_object` and still owns privately. Its
    /// [[DefineOwnProperty]] is the ordinary one, whose first step for this
    /// complete default data descriptor is the compact kernel: a present key
    /// keeps its slot (a duplicate overwrites in place, at its first
    /// position) and an absent non-index String key moves to the canonical
    /// child shape. The kernel declines without mutation otherwise (an array
    /// index, a shape bound, a dictionary), and the general definition then
    /// promotes to the dictionary that orders integer keys first. Every key
    /// is a String, so "__proto__" is an own data key like any other.
    pub(super) fn emit_json_object_member(
        &mut self,
        container: &ValueLocals,
        key: &GcLocal<StringValue>,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let object = s
            .reserve_gc_local(f)
            .initialize(container.cast_reference::<OrdinaryObject>(s, f), f);
        let property = PropertyKeyLocals::from_string(s, key, f);
        let handled = s.reserve_i32_local(f);
        s.call_helper(
            crate::runtime_helpers::CompactDataDefineArguments::new(&object, &property, value),
            self.runtime_helper_base()?,
            f,
        )
        .store(handled, f);
        property.clear(f);
        object.clear(f);
        handled.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_define(container, key, value, false, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(handled, f);
        Ok(())
    }
    /// Attempts to follow the existing canonical default-data transitions
    /// using the buffered keys alone. A miss is handled by the usual
    /// definition path, which can establish the missing transitions.
    fn emit_json_private_object_shape(
        &mut self,
        window: &JsonParseObjectWindow,
        f: &mut Function,
    ) -> Result<GcLocal<CompactDataShape, Nullable>, EmitError> {
        let s = self.runtime_schema();
        let root = s
            .reserve_gc_local(f)
            .initialize(s.empty_compact_storage(f), f);
        let shape = s.reserve_gc_local(f).initialize(
            s.field(ArrayCompactDataStorageSchema::SHAPE)
                .read(&root, s, f)
                .reference(),
            f,
        );
        root.clear(f);
        let result = s
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize_null(s, f);
        let item = s.reserve_i32_local(f);
        let offset = s.reserve_i32_local(f);
        let length = s.reserve_i32_local(f);
        let hash = s.reserve_i64_local(f);
        let proto_name = self.emit_json_string("__proto__", f)?;
        let proto_hash = s.reserve_i64_local(f);
        self.emit_string_key_hash(&proto_name, proto_hash, f);
        proto_name.clear(f);
        item.set_constant(0, f);
        let declined = self.open_frame(ControlFrameKind::Block, f);
        let complete = self.open_frame(ControlFrameKind::Block, f);
        let visit = self.open_frame(ControlFrameKind::Loop, f);
        item.load(f);
        window.count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(complete, f);
        let name = window.key(item, s, f);
        self.emit_string_key_hash(&name, hash, f);
        // The existing __proto__ definition is an ordinary own data property.
        // Keep that spelling on its existing path even if a transition exists.
        proto_hash.load(f);
        hash.load(f);
        f.instruction(&Instruction::I64Eq);
        self.emit_branch_if_to_target(declined, f);
        let table_opt = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::TRANSITIONS)
                .read(&shape, s, f)
                .reference(),
            f,
        );
        table_opt.load(s, f).is_null(f);
        self.emit_branch_if_to_target(declined, f);
        let table = s
            .reserve_gc_local(f)
            .initialize(table_opt.load(s, f).require_non_null(f), f);
        table_opt.clear(f);
        s.array_type::<CompactShapeTable>().length(&table, s, f);
        length.store(f);
        offset.set_constant(0, f);
        let found = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        offset.load(f);
        length.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(declined, f);
        let child_opt = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeTable>()
                .read(&table, offset, s, f)
                .reference(),
            f,
        );
        child_opt.load(s, f).is_null(f);
        self.emit_branch_if_to_target(declined, f);
        let child = s
            .reserve_gc_local(f)
            .initialize(child_opt.load(s, f).require_non_null(f), f);
        child_opt.clear(f);
        let keys = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::KEYS)
                .read(&child, s, f)
                .reference(),
            f,
        );
        let candidate = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeKeys>()
                .read(&keys, item, s, f)
                .reference(),
            f,
        );
        keys.clear(f);
        s.field(StringValueSchema::RAW_KEY_HASH)
            .read(&candidate, s, f);
        hash.load(f);
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_string_payload_equality_i32(&name, &candidate, f);
        self.open_frame(ControlFrameKind::If, f);
        shape.replace(child.load(s, f), f);
        self.emit_branch_to_target(found, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        candidate.clear(f);
        child.clear(f);
        offset.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        offset.store(f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        table.clear(f);
        name.clear(f);
        item.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        item.store(f);
        self.emit_branch_to_target(visit, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        result.replace(shape.load(s, f).nullable(), f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(proto_hash, f);
        s.release_i64_local(hash, f);
        s.release_i32_local(length, f);
        s.release_i32_local(offset, f);
        s.release_i32_local(item, f);
        shape.clear(f);
        Ok(result)
    }

    /// Builds every slot in the final inline record before publishing storage.
    fn emit_json_publish_private_object(
        &mut self,
        container: &ValueLocals,
        shape: &GcLocal<CompactDataShape>,
        window: &JsonParseObjectWindow,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let fields: [ValueLocals; JSON_OBJECT_FIELDS as usize] =
            std::array::from_fn(|_| s.reserve_value_local(f));
        let ordinal = s.reserve_i32_local(f);
        let packed = s.reserve_i32_local(f);
        packed.set_constant(0, f);
        for (slot, value) in fields.iter().enumerate() {
            ordinal.set_constant(slot as i32, f);
            value.set_undefined(f);
            ordinal.load(f);
            window.count.load(f);
            f.instruction(&Instruction::I32LtU);
            self.open_frame(ControlFrameKind::If, f);
            window.read(ordinal, value, s, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            packed.load(f);
            value.tag().load(f);
            f.instruction(&Instruction::I32Const((8 * slot) as i32));
            f.instruction(&Instruction::I32Shl);
            f.instruction(&Instruction::I32Or);
            packed.store(f);
        }
        let [first, second, third, fourth] = &fields;
        let published = s.reserve_gc_local(f).initialize(
            s.struct_type::<InlineCompactDataStorage>()
                .construct(
                    (
                        GcOperand::reference(shape, s),
                        GcOperand::i32_local(packed),
                        GcOperand::i64_local(first.scalar()),
                        GcOperand::i64_local(second.scalar()),
                        GcOperand::i64_local(third.scalar()),
                        GcOperand::i64_local(fourth.scalar()),
                        GcOperand::eq_reference(first.reference()),
                        GcOperand::eq_reference(second.reference()),
                        GcOperand::eq_reference(third.reference()),
                        GcOperand::eq_reference(fourth.reference()),
                    ),
                    f,
                )
                .upcast::<CompactDataStorage>(),
            f,
        );
        let object = s
            .reserve_gc_local(f)
            .initialize(container.cast_reference::<OrdinaryObject>(s, f), f);
        s.field(OrdinaryObjectSchema::PROPERTIES).write(
            &object,
            GcOperand::subtype_reference(&published, s),
            s,
            f,
        );
        object.clear(f);
        published.clear(f);
        for value in fields {
            value.clear(f);
        }
        s.release_i32_local(packed, f);
        s.release_i32_local(ordinal, f);
    }

    pub(super) fn emit_json_deliver_object_member(
        &mut self,
        scratch: &JsonParseObjectScratch,
        frame: &GcLocal<JsonParseFrame>,
        container: &ValueLocals,
        key: &GcLocal<StringValue>,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i64_local(f);
        s.field(JsonParseFrameSchema::INDEX)
            .read(frame, s, f)
            .store_i64(index, f);
        let defined = self.open_frame(ControlFrameKind::Block, f);
        let existing = self.open_frame(ControlFrameKind::Block, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        self.emit_branch_if_to_target(existing, f);
        scratch.emit_allocate_if_needed(self, f);
        let window = scratch.window(index, s, f);
        window.count.load(f);
        f.instruction(&Instruction::I32Const(JSON_OBJECT_FIELDS));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        window.append(key, value, s, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        s.field(JsonParseFrameSchema::INDEX)
            .write(frame, GcOperand::i64_local(index), s, f);
        self.emit_branch_to_target(defined, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // A larger object uses the general path for this and subsequent keys.
        self.emit_json_replay_private_object(container, &window, f)?;
        scratch.release(window, s, f);
        s.field(JsonParseFrameSchema::INDEX)
            .write(frame, GcOperand::i64(-1), s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_json_object_member(container, key, value, f)?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(index, f);
        Ok(())
    }

    pub(super) fn emit_json_complete_object_members(
        &mut self,
        scratch: &JsonParseObjectScratch,
        frame: &GcLocal<JsonParseFrame>,
        container: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i64_local(f);
        s.field(JsonParseFrameSchema::INDEX)
            .read(frame, s, f)
            .store_i64(index, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        self.emit_branch_if_to_target(done, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::I32Const(JSON_OBJECT_FRAME_STRIDE - 1));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        // Empty objects keep the shared empty storage and need no scratch arrays.
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        scratch.top.store(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let window = scratch.window(index, s, f);
        let shape = self.emit_json_private_object_shape(&window, f)?;
        shape.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_replay_private_object(container, &window, f)?;
        f.instruction(&Instruction::Else);
        let known = s
            .reserve_gc_local(f)
            .initialize(shape.load(s, f).require_non_null(f), f);
        self.emit_json_publish_private_object(container, &known, &window, f);
        known.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        shape.clear(f);
        scratch.release(window, s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(index, f);
        Ok(())
    }

    fn emit_json_replay_private_object(
        &mut self,
        container: &ValueLocals,
        window: &JsonParseObjectWindow,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let ordinal = s.reserve_i32_local(f);
        ordinal.set_constant(0, f);
        let value = s.reserve_value_local(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        ordinal.load(f);
        window.count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, f);
        let key = window.key(ordinal, s, f);
        window.read(ordinal, &value, s, f);
        self.emit_json_object_member(container, &key, &value, f)?;
        key.clear(f);
        ordinal.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        ordinal.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.clear(f);
        s.release_i32_local(ordinal, f);
        Ok(())
    }
}
