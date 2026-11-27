//! An iterative UTF-16 parser builds fresh containers directly; only a reviver
//! also receives the completed private records.
use super::parse_cursor::JsonCursor;
use super::parse_frame_state::{JsonParseChildKey, JsonParseObjectScratch};
use super::*;

/// Where a completed value goes: `value`, flagged by `delivered`, and with a
/// reviver also its record.
struct JsonParseDelivery<'a> {
    current: &'a GcLocal<JsonParseFrame, Nullable>,
    record: &'a GcLocal<JsonParseRecord, Nullable>,
    value: &'a ValueLocals,
    delivered: I32Local,
    capture_source: I32Local,
}
impl FunctionBuilder<'_> {
    /// Parses `text` into `value`. Without a reviver the parser keeps only
    /// its frames: each completed value is delivered in `value` itself. With
    /// one it also returns the root's private record, whose source and child
    /// records the reviver alone reads.
    fn emit_json_parse_text(
        &mut self,
        text: &GcLocal<StringValue>,
        realm: &GcLocal<RealmRecord>,
        capture_source: I32Local,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<GcLocal<JsonParseRecord, Nullable>, EmitError> {
        let s = self.runtime_schema();
        let cursor = JsonCursor::open(text, s, f);
        let object_scratch = JsonParseObjectScratch::open(s, f);
        let start = s.reserve_i32_local(f);
        let ordinal = s.reserve_i64_local(f);
        let state = s.reserve_i32_local(f);
        let unit = s.reserve_i32_local(f);
        let expect_value = s.reserve_i32_local(f);
        let delivered = s.reserve_i32_local(f);
        json_i32(expect_value, 1, f);
        json_i32(delivered, 0, f);
        let current = s
            .reserve_gc_local::<JsonParseFrame, Nullable>(f)
            .initialize_null(s, f);
        let record = s
            .reserve_gc_local::<JsonParseRecord, Nullable>(f)
            .initialize_null(s, f);
        let source = s
            .reserve_gc_local::<StringValue, Nullable>(f)
            .initialize_null(s, f);
        let no_children = s
            .reserve_gc_local::<JsonParseChild, Nullable>(f)
            .initialize_null(s, f);
        let container = s.reserve_value_local(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.skip_whitespace(s, f);
        expect_value.load(f);
        self.open_frame(ControlFrameKind::If, f);
        cursor.at.load(f);
        start.store(f);
        cursor.peek(unit, s, f);
        for (code, array) in [(91, true), (123, false)] {
            unit.load(f);
            f.instruction(&Instruction::I32Const(code));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            cursor.advance(f);
            if array {
                let prototype = s.reserve_value_local(f);
                let array_proto = s
                    .reserve_gc_local(f)
                    .initialize(self.emit_load_realm_array_prototype(realm, f), f);
                prototype.set_reference(&array_proto, s, f);
                json_i64(ordinal, 0, f);
                let object = s.reserve_gc_local(f).initialize(
                    self.emit_alloc_array_payload_with_length_and_prototype(
                        ordinal, &prototype, f,
                    )?,
                    f,
                );
                value.set_reference(&object, s, f);
                object.clear(f);
                array_proto.clear(f);
                prototype.clear(f);
            } else {
                let object = self.emit_json_fresh_object(realm, f)?;
                value.set_reference(&object, s, f);
                object.clear(f);
            }
            let frame = self.emit_json_parse_frame(
                value,
                if array {
                    JsonParseFrameState::ArrayFirstOrEnd
                } else {
                    JsonParseFrameState::ObjectFirstKeyOrEnd
                },
                &current,
                f,
            );
            if !array {
                object_scratch.open_frame(&frame, s, f);
            }
            current.replace(frame.load(s, f).nullable(), f);
            frame.clear(f);
            json_i32(expect_value, 0, f);
            self.emit_branch_to_target(next, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        self.emit_json_parse_primitive(
            &cursor,
            realm,
            RuntimeErrorMessage::INVALID_JSON_PARSE_TEXT,
            value,
            f,
        )?;
        // Source records are private reviver input, never part of the parsed value.
        capture_source.load(f);
        self.open_frame(ControlFrameKind::If, f);
        source.replace(
            cursor.slice_string(start, cursor.at, self, f)?.nullable(),
            f,
        );
        let primitive = self.emit_json_parse_record(value, &source, &no_children, f);
        record.replace(primitive.load(s, f).nullable(), f);
        primitive.clear(f);
        source.set_null(s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        json_i32(delivered, 1, f);
        json_i32(expect_value, 0, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        delivered.load(f);
        self.open_frame(ControlFrameKind::If, f);
        current.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let frame = s
            .reserve_gc_local(f)
            .initialize(current.load(s, f).require_non_null(f), f);
        s.struct_type::<JsonParseFrame>()
            .field(JsonParseFrameSchema::STATE)
            .read(&frame, s, f)
            .store(state, f);
        state.load(f);
        f.instruction(&Instruction::I32Const(
            JsonParseFrameState::ObjectValue.wire_code(),
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let holder = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonParseFrame>()
                .field(JsonParseFrameSchema::CONTAINER)
                .read(&frame, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&holder, &container, s, f);
        let key = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonParseFrame>()
                .field(JsonParseFrameSchema::KEY)
                .read(&frame, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        self.emit_json_deliver_object_member(&object_scratch, &frame, &container, &key, value, f)?;
        capture_source.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let child = s
            .reserve_gc_local(f)
            .initialize(record.load(s, f).require_non_null(f), f);
        self.emit_json_parse_child(&frame, JsonParseChildKey::ObjectKey(&key), &child, f)?;
        child.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        key.clear(f);
        holder.clear(f);
        self.emit_json_parse_state(&frame, JsonParseFrameState::ObjectCommaOrEnd, f);
        f.instruction(&Instruction::Else);
        self.emit_json_parse_array_append(&frame, value, ordinal, f)?;
        capture_source.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let child = s
            .reserve_gc_local(f)
            .initialize(record.load(s, f).require_non_null(f), f);
        self.emit_json_parse_child(
            &frame,
            JsonParseChildKey::FreshArrayIndex(ordinal),
            &child,
            f,
        )?;
        child.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_parse_state(&frame, JsonParseFrameState::ArrayCommaOrEnd, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        frame.clear(f);
        json_i32(delivered, 0, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let frame = s
            .reserve_gc_local(f)
            .initialize(current.load(s, f).require_non_null(f), f);
        s.struct_type::<JsonParseFrame>()
            .field(JsonParseFrameSchema::STATE)
            .read(&frame, s, f)
            .store(state, f);
        cursor.skip_whitespace(s, f);
        cursor.peek(unit, s, f);
        let delivery = JsonParseDelivery {
            current: &current,
            record: &record,
            value,
            delivered,
            capture_source,
        };
        let close_container = s.reserve_i32_local(f);
        close_container.set_constant(0, f);
        let dispatched = self.open_frame(ControlFrameKind::Block, f);
        for selected in JsonParseFrameState::ALL {
            state.load(f);
            f.instruction(&Instruction::I32Const(selected.wire_code()));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            match selected {
                JsonParseFrameState::ArrayFirstOrEnd | JsonParseFrameState::ObjectFirstKeyOrEnd => {
                    let array = matches!(selected, JsonParseFrameState::ArrayFirstOrEnd);
                    unit.load(f);
                    f.instruction(&Instruction::I32Const(if array { 93 } else { 125 }));
                    f.instruction(&Instruction::I32Eq);
                    self.open_frame(ControlFrameKind::If, f);
                    close_container.set_constant(1, f);
                    f.instruction(&Instruction::Else);
                    self.emit_json_parse_state(
                        &frame,
                        if array {
                            JsonParseFrameState::ArrayValue
                        } else {
                            JsonParseFrameState::ObjectKey
                        },
                        f,
                    );
                    // An element follows at once; a key is parsed next turn.
                    if array {
                        json_i32(expect_value, 1, f);
                    }
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                }
                // A frame awaiting a value is answered by its delivery, which
                // moves it to the comma state before the next dispatch.
                JsonParseFrameState::ArrayValue | JsonParseFrameState::ObjectValue => {
                    json_i32(expect_value, 1, f)
                }
                JsonParseFrameState::ObjectKey => {
                    let key = self.emit_json_parse_string(
                        &cursor,
                        realm,
                        RuntimeErrorMessage::INVALID_JSON_PARSE_TEXT,
                        f,
                    )?;
                    s.struct_type::<JsonParseFrame>()
                        .field(JsonParseFrameSchema::KEY)
                        .write(&frame, GcOperand::nullable_reference(&key, s), s, f);
                    key.clear(f);
                    cursor.skip_whitespace(s, f);
                    cursor.expect_unit(
                        58,
                        realm,
                        RuntimeErrorMessage::INVALID_JSON_PARSE_TEXT,
                        self,
                        f,
                    )?;
                    self.emit_json_parse_state(&frame, JsonParseFrameState::ObjectValue, f);
                    json_i32(expect_value, 1, f);
                }
                JsonParseFrameState::ArrayCommaOrEnd | JsonParseFrameState::ObjectCommaOrEnd => {
                    let array = matches!(selected, JsonParseFrameState::ArrayCommaOrEnd);
                    unit.load(f);
                    f.instruction(&Instruction::I32Const(44));
                    f.instruction(&Instruction::I32Eq);
                    self.open_frame(ControlFrameKind::If, f);
                    cursor.advance(f);
                    self.emit_json_parse_state(
                        &frame,
                        if array {
                            JsonParseFrameState::ArrayValue
                        } else {
                            JsonParseFrameState::ObjectKey
                        },
                        f,
                    );
                    if array {
                        json_i32(expect_value, 1, f);
                    }
                    f.instruction(&Instruction::Else);
                    unit.load(f);
                    f.instruction(&Instruction::I32Const(if array { 93 } else { 125 }));
                    f.instruction(&Instruction::I32Ne);
                    self.open_frame(ControlFrameKind::If, f);
                    self.emit_json_bad_text(
                        realm,
                        RuntimeErrorMessage::INVALID_JSON_PARSE_TEXT,
                        f,
                    )?;
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    close_container.set_constant(1, f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                }
            }
            self.emit_branch_to_target(dispatched, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        close_container.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_complete_container(&frame, &delivery, &cursor, &object_scratch, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(close_container, f);
        frame.clear(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        cursor.skip_whitespace(s, f);
        cursor.at.load(f);
        cursor.end.load(f);
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_bad_text(realm, RuntimeErrorMessage::INVALID_JSON_PARSE_TEXT, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        container.clear(f);
        no_children.clear(f);
        source.clear(f);
        current.clear(f);
        s.release_i32_local(delivered, f);
        s.release_i32_local(expect_value, f);
        s.release_i32_local(unit, f);
        s.release_i32_local(state, f);
        s.release_i64_local(ordinal, f);
        s.release_i32_local(start, f);
        object_scratch.close(s, f);
        cursor.close(s, f);
        Ok(record)
    }
    fn emit_json_complete_container(
        &mut self,
        frame: &GcLocal<JsonParseFrame>,
        delivery: &JsonParseDelivery<'_>,
        cursor: &JsonCursor<'_>,
        object_scratch: &JsonParseObjectScratch,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        cursor.advance(f);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonParseFrame>()
                .field(JsonParseFrameSchema::CONTAINER)
                .read(frame, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, delivery.value, s, f);
        stored.clear(f);
        self.emit_json_reference_test::<OrdinaryObject>(delivery.value, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_complete_object_members(object_scratch, frame, delivery.value, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        delivery.capture_source.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let source = s
            .reserve_gc_local::<StringValue, Nullable>(f)
            .initialize_null(s, f);
        let children = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonParseFrame>()
                .field(JsonParseFrameSchema::CHILDREN)
                .read(frame, s, f)
                .reference(),
            f,
        );
        let record = self.emit_json_parse_record(delivery.value, &source, &children, f);
        delivery.record.replace(record.load(s, f).nullable(), f);
        record.clear(f);
        children.clear(f);
        source.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        json_i32(delivery.delivered, 1, f);
        delivery.current.replace(
            s.struct_type::<JsonParseFrame>()
                .field(JsonParseFrameSchema::PARENT)
                .read(frame, s, f)
                .reference(),
            f,
        );
        Ok(())
    }
    pub(super) fn emit_json_parse_entry(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let text = s.reserve_value_local(f);
        let reviver = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &text, f);
        self.emit_builtin_arg_to_value(1, &reviver, f);
        self.emit_value_to_string_payload(&text, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        let string = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        let realm = self.emit_execution_realm(f);
        // IsCallable reads only the object's call capability: it invokes no JS
        // and cannot throw, including for a revoked callable proxy. Evaluate it
        // once after ToString so parser metadata and the reviver share the fact.
        let capture_source = s.reserve_i32_local(f);
        self.emit_is_callable_i32(&reviver, f)?;
        capture_source.store(f);
        let record = self.emit_json_parse_text(&string, &realm, capture_source, &output, f)?;
        capture_source.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let root = s
            .reserve_gc_local(f)
            .initialize(record.load(s, f).require_non_null(f), f);
        self.emit_json_revive(&root, &reviver, &realm, &output, f)?;
        root.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&output, f);
        record.clear(f);
        s.release_i32_local(capture_source, f);
        realm.clear(f);
        string.clear(f);
        output.clear(f);
        pending.clear(f);
        reviver.clear(f);
        text.clear(f);
        Ok(())
    }
    pub(super) fn emit_json_raw_entry(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let input = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let value = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        self.emit_value_to_string_payload(&input, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        let text = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        let realm = self.emit_execution_realm(f);
        let cursor = JsonCursor::open(&text, s, f);
        // No whitespace/container is accepted: the primitive grammar must consume all units.
        self.emit_json_parse_primitive(
            &cursor,
            &realm,
            RuntimeErrorMessage::INVALID_JSON_RAWJSON_TEXT,
            &value,
            f,
        )?;
        cursor.at.load(f);
        cursor.end.load(f);
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_error(
            &realm,
            NativeErrorKind::SyntaxError,
            RuntimeErrorMessage::INVALID_JSON_RAWJSON_TEXT,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.close(s, f);
        let prototype = s.reserve_value_local(f);
        prototype.set_scalar(ScalarValue::Null, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), f)?,
            f,
        );
        let key_string = self.emit_json_string("rawJSON", f)?;
        let key_value = s.reserve_value_local(f);
        key_value.set_reference(&key_string, s, f);
        let key = self.emit_value_to_property_key_locals(&key_value, f)?;
        value.set_reference(&text, s, f);
        self.emit_object_append_data_property_with_flags(
            &header, &key, &value, false, true, false, f,
        )?;
        s.struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::EXTENSIBLE)
            .write(&header, GcOperand::boolean(false), s, f);
        let raw = s.reserve_gc_local(f).initialize(
            s.struct_type::<RawJsonObject>().construct(
                (
                    GcOperand::reference(&header, s),
                    GcOperand::reference(&text, s),
                ),
                f,
            ),
            f,
        );
        value.set_reference(&raw, s, f);
        self.completion().set_normal(&value, f);
        raw.clear(f);
        key.clear(f);
        key_value.clear(f);
        key_string.clear(f);
        header.clear(f);
        prototype.clear(f);
        realm.clear(f);
        text.clear(f);
        value.clear(f);
        pending.clear(f);
        input.clear(f);
        Ok(())
    }
}
