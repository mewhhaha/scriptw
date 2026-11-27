//! Closed frame fields and consuming container completion replace linear stacks.
use super::*;

/// Object keys may repeat. A parser-owned array ordinal is fresh because the
/// private frame increments it exactly once after each completed element.
/// Keep the distinction at the insertion boundary rather than accepting a flag
/// that could silently disable duplicate replacement for an object property.
pub(super) enum JsonParseChildKey<'a> {
    ObjectKey(&'a GcLocal<StringValue>),
    FreshArrayIndex(I64Local),
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_json_parse_frame(
        &mut self,
        value: &ValueLocals,
        state: JsonParseFrameState,
        parent: &GcLocal<JsonParseFrame, Nullable>,
        f: &mut Function,
    ) -> GcLocal<JsonParseFrame> {
        let s = self.runtime_schema();
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(value, f), f);
        let frame = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonParseFrame>().construct(
                (
                    GcOperand::reference(&stored, s),
                    GcOperand::constant(state),
                    GcOperand::null(s),
                    GcOperand::i64(0),
                    GcOperand::null(s),
                    GcOperand::reference(parent, s),
                ),
                f,
            ),
            f,
        );
        stored.clear(f);
        frame
    }
    pub(super) fn emit_json_parse_state(
        &self,
        frame: &GcLocal<JsonParseFrame>,
        state: JsonParseFrameState,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.struct_type::<JsonParseFrame>()
            .field(JsonParseFrameSchema::STATE)
            .write(frame, GcOperand::constant(state), s, f);
    }
    pub(super) fn emit_json_parse_record(
        &self,
        value: &ValueLocals,
        source: &GcLocal<StringValue, Nullable>,
        children: &GcLocal<JsonParseChild, Nullable>,
        f: &mut Function,
    ) -> GcLocal<JsonParseRecord> {
        let s = self.runtime_schema();
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(value, f), f);
        let record = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonParseRecord>().construct(
                (
                    GcOperand::reference(&stored, s),
                    GcOperand::reference(source, s),
                    GcOperand::reference(children, s),
                ),
                f,
            ),
            f,
        );
        stored.clear(f);
        record
    }
    /// Updating the original key's record makes duplicate-key source metadata
    /// agree with CreateDataProperty's last value without exposing metadata to JS.
    pub(super) fn emit_json_parse_child(
        &mut self,
        frame: &GcLocal<JsonParseFrame>,
        key: JsonParseChildKey<'_>,
        record: &GcLocal<JsonParseRecord>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let key = match key {
            JsonParseChildKey::ObjectKey(key) => key,
            JsonParseChildKey::FreshArrayIndex(index) => {
                let key = self.emit_json_index_key(index, f)?;
                self.emit_json_parse_prepend_child(frame, &key, record, f);
                key.clear(f);
                return Ok(());
            }
        };
        let s = self.runtime_schema();
        let found = s.reserve_i32_local(f);
        json_i32(found, 0, f);
        let head = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonParseFrame>()
                .field(JsonParseFrameSchema::CHILDREN)
                .read(frame, s, f)
                .reference(),
            f,
        );
        let cursor = s.reserve_gc_local(f).initialize(head.load(s, f), f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let child = s
            .reserve_gc_local(f)
            .initialize(cursor.load(s, f).require_non_null(f), f);
        let child_key = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonParseChild>()
                .field(JsonParseChildSchema::KEY)
                .read(&child, s, f)
                .reference(),
            f,
        );
        self.emit_string_payload_equality_i32(key, &child_key, f);
        self.open_frame(ControlFrameKind::If, f);
        s.struct_type::<JsonParseChild>()
            .field(JsonParseChildSchema::RECORD)
            .write(&child, GcOperand::reference(record, s), s, f);
        json_i32(found, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.replace(
            s.struct_type::<JsonParseChild>()
                .field(JsonParseChildSchema::NEXT)
                .read(&child, s, f)
                .reference(),
            f,
        );
        child_key.clear(f);
        child.clear(f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_parse_prepend_child(frame, key, record, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.clear(f);
        head.clear(f);
        s.release_i32_local(found, f);
        Ok(())
    }

    fn emit_json_parse_prepend_child(
        &self,
        frame: &GcLocal<JsonParseFrame>,
        key: &GcLocal<StringValue>,
        record: &GcLocal<JsonParseRecord>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let head = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonParseFrame>()
                .field(JsonParseFrameSchema::CHILDREN)
                .read(frame, s, f)
                .reference(),
            f,
        );
        let child = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonParseChild>().construct(
                (
                    GcOperand::reference(key, s),
                    GcOperand::reference(record, s),
                    GcOperand::reference(&head, s),
                ),
                f,
            ),
            f,
        );
        s.struct_type::<JsonParseFrame>()
            .field(JsonParseFrameSchema::CHILDREN)
            .write(frame, GcOperand::nullable_reference(&child, s), s, f);
        child.clear(f);
        head.clear(f);
    }
}

/// Large objects and deeper nesting spill to the ordinary definition path.
/// A frame's object index is the aligned scratch base plus its property count,
/// or -1 after it has fallen back. Array frames keep their usual array index.
pub(super) const JSON_OBJECT_FIELDS: i32 = 4;
pub(super) const JSON_OBJECT_FRAME_STRIDE: i32 = 8;
const JSON_OBJECT_SCRATCH_CAPACITY: i32 = JSON_OBJECT_FRAME_STRIDE * 4;

pub(super) struct JsonParseObjectScratch {
    pub(super) top: I32Local,
    tags: GcLocal<CompactTagArray, Nullable>,
    scalars: GcLocal<CompactScalarArray, Nullable>,
    references: GcLocal<CompactReferenceArray, Nullable>,
}

/// A window at the top of the scratch stack. It holds [values, keys] in the
/// reference backing, keeping all references rooted across nested parsing.
pub(super) struct JsonParseObjectWindow {
    base: I32Local,
    pub(super) count: I32Local,
    tags: GcLocal<CompactTagArray>,
    scalars: GcLocal<CompactScalarArray>,
    references: GcLocal<CompactReferenceArray>,
}

impl JsonParseObjectScratch {
    pub(super) fn open(s: &RuntimeSchema, f: &mut Function) -> Self {
        let top = s.reserve_i32_local(f);
        top.set_constant(0, f);
        Self {
            top,
            tags: s.reserve_gc_local(f).initialize_null(s, f),
            scalars: s.reserve_gc_local(f).initialize_null(s, f),
            references: s.reserve_gc_local(f).initialize_null(s, f),
        }
    }

    pub(super) fn open_frame(
        &self,
        frame: &GcLocal<JsonParseFrame>,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        let index = s.reserve_i64_local(f);
        self.top.load(f);
        f.instruction(&Instruction::I32Const(JSON_OBJECT_SCRATCH_CAPACITY));
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.top.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        index.store(f);
        self.top.load(f);
        f.instruction(&Instruction::I32Const(JSON_OBJECT_FRAME_STRIDE));
        f.instruction(&Instruction::I32Add);
        self.top.store(f);
        f.instruction(&Instruction::Else);
        index.set_constant(-1, f);
        f.instruction(&Instruction::End);
        s.field(JsonParseFrameSchema::INDEX)
            .write(frame, GcOperand::i64_local(index), s, f);
        s.release_i64_local(index, f);
    }

    pub(super) fn emit_allocate_if_needed(
        &self,
        builder: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) {
        let s = builder.runtime_schema();
        self.references.load(s, f).is_null(f);
        builder.open_frame(ControlFrameKind::If, f);
        let capacity = s.reserve_i32_local(f);
        capacity.set_constant(JSON_OBJECT_SCRATCH_CAPACITY, f);
        self.tags.replace(
            s.array_type::<CompactTagArray>()
                .filled(GcOperand::i32(0), capacity, f)
                .nullable(),
            f,
        );
        self.scalars.replace(
            s.array_type::<CompactScalarArray>()
                .filled(GcOperand::i64(0), capacity, f)
                .nullable(),
            f,
        );
        self.references.replace(
            s.array_type::<CompactReferenceArray>()
                .filled(GcOperand::null_eq(), capacity, f)
                .nullable(),
            f,
        );
        s.release_i32_local(capacity, f);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    pub(super) fn window(
        &self,
        index: I64Local,
        s: &RuntimeSchema,
        f: &mut Function,
    ) -> JsonParseObjectWindow {
        let base = s.reserve_i32_local(f);
        let count = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::I32Const(!(JSON_OBJECT_FRAME_STRIDE - 1)));
        f.instruction(&Instruction::I32And);
        base.store(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::I32Const(JSON_OBJECT_FRAME_STRIDE - 1));
        f.instruction(&Instruction::I32And);
        count.store(f);
        JsonParseObjectWindow {
            base,
            count,
            tags: s
                .reserve_gc_local(f)
                .initialize(self.tags.load(s, f).require_non_null(f), f),
            scalars: s
                .reserve_gc_local(f)
                .initialize(self.scalars.load(s, f).require_non_null(f), f),
            references: s
                .reserve_gc_local(f)
                .initialize(self.references.load(s, f).require_non_null(f), f),
        }
    }

    pub(super) fn release(
        &self,
        window: JsonParseObjectWindow,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        window.base.load(f);
        self.top.store(f);
        let at = s.reserve_i32_local(f);
        // Clear both halves, so sibling frames and later array elements
        // do not retain the keys or values from a popped window.
        for offset in 0..JSON_OBJECT_FRAME_STRIDE {
            window.base.load(f);
            f.instruction(&Instruction::I32Const(offset));
            f.instruction(&Instruction::I32Add);
            at.store(f);
            s.array_type::<CompactReferenceArray>().write(
                &window.references,
                at,
                GcOperand::null_eq(),
                s,
                f,
            );
        }
        s.release_i32_local(at, f);
        window.references.clear(f);
        window.scalars.clear(f);
        window.tags.clear(f);
        s.release_i32_local(window.count, f);
        s.release_i32_local(window.base, f);
    }

    pub(super) fn close(self, s: &RuntimeSchema, f: &mut Function) {
        self.references.clear(f);
        self.scalars.clear(f);
        self.tags.clear(f);
        s.release_i32_local(self.top, f);
    }
}

impl JsonParseObjectWindow {
    fn slot(&self, ordinal: I32Local, key: bool, s: &RuntimeSchema, f: &mut Function) -> I32Local {
        let slot = s.reserve_i32_local(f);
        self.base.load(f);
        ordinal.load(f);
        f.instruction(&Instruction::I32Add);
        if key {
            f.instruction(&Instruction::I32Const(JSON_OBJECT_FIELDS));
            f.instruction(&Instruction::I32Add);
        }
        slot.store(f);
        slot
    }

    pub(super) fn append(
        &self,
        key: &GcLocal<StringValue>,
        value: &ValueLocals,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        let slot = self.slot(self.count, false, s, f);
        s.array_type::<CompactTagArray>().write(
            &self.tags,
            slot,
            GcOperand::i32_local(value.tag()),
            s,
            f,
        );
        s.array_type::<CompactScalarArray>().write(
            &self.scalars,
            slot,
            GcOperand::i64_local(value.scalar()),
            s,
            f,
        );
        s.array_type::<CompactReferenceArray>().write(
            &self.references,
            slot,
            GcOperand::eq_reference(value.reference()),
            s,
            f,
        );
        s.release_i32_local(slot, f);
        let slot = self.slot(self.count, true, s, f);
        let name = s.reserve_value_local(f);
        name.set_reference(key, s, f);
        s.array_type::<CompactReferenceArray>().write(
            &self.references,
            slot,
            GcOperand::eq_reference(name.reference()),
            s,
            f,
        );
        name.clear(f);
        s.release_i32_local(slot, f);
    }

    pub(super) fn key(
        &self,
        ordinal: I32Local,
        s: &RuntimeSchema,
        f: &mut Function,
    ) -> GcLocal<StringValue> {
        let slot = self.slot(ordinal, true, s, f);
        let value = s.reserve_value_local(f);
        s.array_type::<CompactReferenceArray>()
            .read(&self.references, slot, s, f)
            .store_eq(value.reference(), f);
        let key = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<StringValue>(s, f), f);
        value.clear(f);
        s.release_i32_local(slot, f);
        key
    }

    pub(super) fn read(
        &self,
        ordinal: I32Local,
        value: &ValueLocals,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        let slot = self.slot(ordinal, false, s, f);
        s.array_type::<CompactTagArray>()
            .read(&self.tags, slot, s, f)
            .store(value.tag(), f);
        s.array_type::<CompactScalarArray>()
            .read(&self.scalars, slot, s, f)
            .store_i64(value.scalar(), f);
        s.array_type::<CompactReferenceArray>()
            .read(&self.references, slot, s, f)
            .store_eq(value.reference(), f);
        s.release_i32_local(slot, f);
    }
}
