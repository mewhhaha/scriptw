//! SerializeJSONProperty is the sole registered recursive runtime owner.
use super::*;

impl FunctionBuilder<'_> {
    fn emit_json_stringify_call(
        &mut self,
        holder: &ValueLocals,
        key: &GcLocal<StringValue>,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        seen: &GcLocal<ValueArray>,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let result = s.reserve_completion(f);
        s.call_helper(
            JsonStringifyValueArguments::new(
                holder,
                key,
                context,
                indent,
                seen,
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            f,
        )
        .store(&result, f);
        self.completion().copy_from(&result, f);
        self.emit_propagate_current_throw_if_needed(f);
        out.copy_from(result.value(), f);
        result.clear(f);
        Ok(())
    }
    pub(crate) fn compile_json_stringify_value_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::JsonStringifyValue);
        let parameters = self.helper_parameters::<JsonStringifyValueParameters>(&mut f);
        self.completion().initialize(&mut f);
        let result = self.runtime_schema().reserve_value_local(&mut f);
        self.emit_json_serialize_property(
            &parameters.holder,
            &parameters.key,
            &parameters.context,
            parameters.indent,
            &parameters.seen,
            &result,
            &mut f,
        )?;
        self.completion().set_normal(&result, &mut f);
        self.completion().emit(&mut f);
        result.clear(&mut f);
        parameters.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
    fn emit_json_unbox(&mut self, value: &ValueLocals, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.emit_json_reference_test::<PrimitiveBox>(value, f);
        self.open_frame(ControlFrameKind::If, f);
        let object = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<PrimitiveBox>(s, f), f);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<PrimitiveBox>()
                .field(PrimitiveBoxSchema::PRIMITIVE)
                .read(&object, s, f)
                .reference(),
            f,
        );
        let primitive = s.reserve_value_local(f);
        let result = s.reserve_completion(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &primitive, s, f);
        self.emit_json_tag_test(&primitive, WasmRuntimeValueTag::Number, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_number_payload(value, &result, f)?;
        self.completion().copy_from(&result, f);
        self.emit_propagate_current_throw_if_needed(f);
        value.copy_from(result.value(), f);
        f.instruction(&Instruction::Else);
        self.emit_json_tag_test(&primitive, WasmRuntimeValueTag::String, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_string_payload(value, &result, f)?;
        self.completion().copy_from(&result, f);
        self.emit_propagate_current_throw_if_needed(f);
        value.copy_from(result.value(), f);
        f.instruction(&Instruction::Else);
        // Boolean and BigInt wrapper conversion uses their internal data.
        self.emit_json_tag_test(&primitive, WasmRuntimeValueTag::Boolean, f);
        self.emit_json_tag_test(&primitive, WasmRuntimeValueTag::BigInt, f);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        value.copy_from(&primitive, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        result.clear(f);
        primitive.clear(f);
        stored.clear(f);
        object.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_json_prepare_gap(
        &mut self,
        space: &ValueLocals,
        f: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        self.emit_json_reference_test::<PrimitiveBox>(space, f);
        self.open_frame(ControlFrameKind::If, f);
        let boxed = s
            .reserve_gc_local(f)
            .initialize(space.cast_reference::<PrimitiveBox>(s, f), f);
        let primitive = s.reserve_value_local(f);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<PrimitiveBox>()
                .field(PrimitiveBoxSchema::PRIMITIVE)
                .read(&boxed, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &primitive, s, f);
        self.emit_json_tag_test(&primitive, WasmRuntimeValueTag::Number, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_number_payload(space, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        space.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_tag_test(&primitive, WasmRuntimeValueTag::String, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_string_payload(space, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        space.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        stored.clear(f);
        primitive.clear(f);
        boxed.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let gap = self.emit_json_string("", f)?;
        let start = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        json_i64(start, 0, f);
        self.emit_json_tag_test(space, WasmRuntimeValueTag::String, f);
        self.open_frame(ControlFrameKind::If, f);
        let string = s
            .reserve_gc_local(f)
            .initialize(space.cast_reference::<StringValue>(s, f), f);
        self.emit_native_gc_string_length(&string, end, f);
        end.load(f);
        f.instruction(&Instruction::I64Const(10));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        json_i64(end, 10, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        gap.replace(self.emit_gc_string_slice(&string, start, end, f), f);
        string.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_tag_test(space, WasmRuntimeValueTag::Number, f);
        self.open_frame(ControlFrameKind::If, f);
        let number = s.reserve_f64_local(f);
        space.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        number.store(f);
        json_i64(end, 0, f);
        number.load(f);
        f.instruction(&Instruction::F64Const(0.0.into()));
        f.instruction(&Instruction::F64Gt);
        self.open_frame(ControlFrameKind::If, f);
        number.load(f);
        f.instruction(&Instruction::F64Const(10.0.into()));
        f.instruction(&Instruction::F64Ge);
        self.open_frame(ControlFrameKind::If, f);
        json_i64(end, 10, f);
        f.instruction(&Instruction::Else);
        number.load(f);
        f.instruction(&Instruction::I64TruncF64U);
        end.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let spaces = self.emit_json_string("          ", f)?;
        gap.replace(self.emit_gc_string_slice(&spaces, start, end, f), f);
        spaces.clear(f);
        s.release_f64_local(number, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(end, f);
        s.release_i64_local(start, f);
        pending.clear(f);
        Ok(gap)
    }
    fn emit_json_property_list(
        &mut self,
        replacer: &ValueLocals,
        f: &mut Function,
    ) -> Result<GcLocal<ValueArray, Nullable>, EmitError> {
        let s = self.runtime_schema();
        let list = s
            .reserve_gc_local::<ValueArray, Nullable>(f)
            .initialize_null(s, f);
        let array = s.reserve_i32_local(f);
        let pending = s.reserve_completion(f);
        self.emit_is_array_i32(replacer, array, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        array.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let length_key = self.emit_json_string("length", f)?;
        let element = s.reserve_value_local(f);
        let primitive = s.reserve_value_local(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let accepted = s.reserve_i32_local(f);
        let count = s.reserve_i32_local(f);
        let cursor = s.reserve_i32_local(f);
        let duplicate = s.reserve_i32_local(f);
        self.emit_json_get(replacer, &length_key, &element, f)?;
        self.emit_to_length_i64_from_value_locals(&element, length, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        length_key.clear(f);
        json_i64(index, 0, f);
        // The growing list is a native List, never an observable Array.
        let empty = self.emit_pre_evaluated_arg_vector(&[], f);
        let normalized = s.reserve_gc_local(f).initialize(empty.load(s, f), f);
        empty.clear(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let key = self.emit_json_index_key(index, f)?;
        self.emit_json_get(replacer, &key, &element, f)?;
        key.clear(f);
        json_i32(accepted, 0, f);
        self.emit_json_tag_test(&element, WasmRuntimeValueTag::String, f);
        self.emit_json_tag_test(&element, WasmRuntimeValueTag::Number, f);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        json_i32(accepted, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_reference_test::<PrimitiveBox>(&element, f);
        self.open_frame(ControlFrameKind::If, f);
        let boxed = s
            .reserve_gc_local(f)
            .initialize(element.cast_reference::<PrimitiveBox>(s, f), f);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<PrimitiveBox>()
                .field(PrimitiveBoxSchema::PRIMITIVE)
                .read(&boxed, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &primitive, s, f);
        self.emit_json_tag_test(&primitive, WasmRuntimeValueTag::String, f);
        self.emit_json_tag_test(&primitive, WasmRuntimeValueTag::Number, f);
        f.instruction(&Instruction::I32Or);
        accepted.store(f);
        stored.clear(f);
        boxed.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        accepted.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_string_payload(&element, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        element.copy_from(pending.value(), f);
        json_i32(duplicate, 0, f);
        json_i32(cursor, 0, f);
        s.array_type::<ValueArray>().length(&normalized, s, f);
        count.store(f);
        let scan_done = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_branch_to_target(scan_done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_argument_vector_entry_to_value(&normalized, cursor, &primitive, f);
        self.emit_tagged_payload_same_value_i32(&element, &primitive, f)?;
        self.open_frame(ControlFrameKind::If, f);
        json_i32(duplicate, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        cursor.store(f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        duplicate.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let construction = ArgumentListConstruction::new(self, f);
        self.emit_json_copy_list(&construction, &normalized, f);
        construction.append(&element, s, f);
        let completed = construction.finish(self, f);
        normalized.replace(completed.load(s, f), f);
        completed.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        json_increment(index, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        list.replace(normalized.load(s, f).nullable(), f);
        normalized.clear(f);
        s.release_i32_local(duplicate, f);
        s.release_i32_local(cursor, f);
        s.release_i32_local(count, f);
        s.release_i32_local(accepted, f);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        primitive.clear(f);
        element.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        pending.clear(f);
        s.release_i32_local(array, f);
        Ok(list)
    }
    fn emit_json_copy_list(
        &mut self,
        construction: &ArgumentListConstruction,
        list: &GcLocal<ValueArray>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let count = s.reserve_i32_local(f);
        let index = s.reserve_i32_local(f);
        let value = s.reserve_value_local(f);
        s.array_type::<ValueArray>().length(list, s, f);
        count.store(f);
        json_i32(index, 0, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_argument_vector_entry_to_value(list, index, &value, f);
        construction.append(&value, s, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.clear(f);
        s.release_i32_local(index, f);
        s.release_i32_local(count, f);
    }
    pub(super) fn emit_json_stringify_entry(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        let replacer = s.reserve_value_local(f);
        let space = s.reserve_value_local(f);
        let holder = s.reserve_value_local(f);
        let output = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &value, f);
        self.emit_builtin_arg_to_value(1, &replacer, f);
        self.emit_builtin_arg_to_value(2, &space, f);
        let realm = self.emit_execution_realm(f);
        let property_list = s
            .reserve_gc_local::<ValueArray, Nullable>(f)
            .initialize_null(s, f);
        self.emit_is_callable_i32(&replacer, f)?;
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Else);
        let normalized = self.emit_json_property_list(&replacer, f)?;
        property_list.replace(normalized.load(s, f), f);
        normalized.clear(f);
        replacer.set_undefined(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let gap = self.emit_json_prepare_gap(&space, f)?;
        let replacement = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&replacer, f), f);
        let context = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>().construct(
                (
                    GcOperand::reference(&realm, s),
                    GcOperand::reference(&replacement, s),
                    GcOperand::reference(&property_list, s),
                    GcOperand::reference(&gap, s),
                ),
                f,
            ),
            f,
        );
        let root = self.emit_json_plain_object(&realm, f)?;
        holder.set_reference(&root, s, f);
        let key = self.emit_json_string("", f)?;
        self.emit_json_define(&holder, &key, &value, false, f)?;
        let seen = self.emit_pre_evaluated_arg_vector(&[], f);
        let indent = s.reserve_i64_local(f);
        json_i64(indent, 0, f);
        self.emit_json_stringify_call(&holder, &key, &context, indent, &seen, &output, f)?;
        self.completion().set_normal(&output, f);
        s.release_i64_local(indent, f);
        seen.clear(f);
        key.clear(f);
        root.clear(f);
        context.clear(f);
        replacement.clear(f);
        gap.clear(f);
        property_list.clear(f);
        realm.clear(f);
        output.clear(f);
        holder.clear(f);
        space.clear(f);
        replacer.clear(f);
        value.clear(f);
        Ok(())
    }
    fn emit_json_serialize_property(
        &mut self,
        holder: &ValueLocals,
        key: &GcLocal<StringValue>,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        seen: &GcLocal<ValueArray>,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        let method = s.reserve_value_local(f);
        let replacement = s.reserve_value_local(f);
        let key_value = s.reserve_value_local(f);
        key_value.set_reference(key, s, f);
        self.emit_json_get(holder, key, &value, f)?;
        self.emit_is_heap_object_like_tag_i32(value.tag(), f);
        self.emit_json_tag_test(&value, WasmRuntimeValueTag::BigInt, f);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        let to_json = self.emit_json_string("toJSON", f)?;
        self.emit_json_get(&value, &to_json, &method, f)?;
        to_json.clear(f);
        self.emit_is_callable_i32(&method, f)?;
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_call(&method, &value, &[&key_value], &replacement, f)?;
        value.copy_from(&replacement, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>()
                .field(JsonStringifyContextSchema::REPLACER)
                .read(context, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &method, s, f);
        stored.clear(f);
        self.emit_is_callable_i32(&method, f)?;
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_apply_replacer_with_this(
            JsonStringifyReplacerInvocationLocals::new(
                JsonStringifyReplacerFunctionLocals::new(&method),
                JsonStringifyReplacerReceiverLocals::new(holder),
                JsonStringifyReplacerPropertyKeyLocals::new(&key_value),
                JsonStringifyReplacerValueLocals::new(&value),
            ),
            &replacement,
            f,
        )?;
        value.copy_from(&replacement, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_unbox(&value, f)?;
        out.set_undefined(f);
        self.emit_json_tag_test(&value, WasmRuntimeValueTag::String, f);
        self.open_frame(ControlFrameKind::If, f);
        let text = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<StringValue>(s, f), f);
        let quoted = self.emit_json_quote(&text, f)?;
        out.set_reference(&quoted, s, f);
        quoted.clear(f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_tag_test(&value, WasmRuntimeValueTag::Number, f);
        self.open_frame(ControlFrameKind::If, f);
        value.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Abs);
        f.instruction(&Instruction::F64Const(f64::INFINITY.into()));
        f.instruction(&Instruction::F64Lt);
        self.open_frame(ControlFrameKind::If, f);
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_number_to_string_payload(value.scalar(), f)?, f);
        out.set_reference(&text, s, f);
        text.clear(f);
        f.instruction(&Instruction::Else);
        let text = self.emit_json_string("null", f)?;
        out.set_reference(&text, s, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        for tag in [WasmRuntimeValueTag::Null, WasmRuntimeValueTag::Boolean] {
            self.emit_json_tag_test(&value, tag, f);
            self.open_frame(ControlFrameKind::If, f);
            if tag == WasmRuntimeValueTag::Null {
                let text = self.emit_json_string("null", f)?;
                out.set_reference(&text, s, f);
                text.clear(f);
            } else {
                value.scalar().load(f);
                f.instruction(&Instruction::I64Eqz);
                self.open_frame(ControlFrameKind::If, f);
                let text = self.emit_json_string("false", f)?;
                out.set_reference(&text, s, f);
                text.clear(f);
                f.instruction(&Instruction::Else);
                let text = self.emit_json_string("true", f)?;
                out.set_reference(&text, s, f);
                text.clear(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        let realm = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>()
                .field(JsonStringifyContextSchema::REALM)
                .read(context, s, f)
                .reference(),
            f,
        );
        self.emit_json_tag_test(&value, WasmRuntimeValueTag::BigInt, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_error(
            &realm,
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::DO_NOT_KNOW_HOW_TO_SERIALIZE_A_BIGINT,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_reference_test::<RawJsonObject>(&value, f);
        self.open_frame(ControlFrameKind::If, f);
        let raw = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<RawJsonObject>(s, f), f);
        let text = s.reserve_gc_local(f).initialize(
            s.struct_type::<RawJsonObject>()
                .field(RawJsonObjectSchema::RAW_JSON)
                .read(&raw, s, f)
                .reference(),
            f,
        );
        out.set_reference(&text, s, f);
        text.clear(f);
        raw.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_is_heap_object_like_tag_i32(value.tag(), f);
        self.emit_is_callable_i32(&value, f)?;
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_serialize_container(&value, context, indent, seen, &realm, out, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        realm.clear(f);
        key_value.clear(f);
        replacement.clear(f);
        method.clear(f);
        value.clear(f);
        Ok(())
    }
    fn emit_json_indent(
        &mut self,
        gap: &GcLocal<StringValue>,
        depth: I64Local,
        f: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let s = self.runtime_schema();
        let result = self.emit_json_string("", f)?;
        let index = s.reserve_i64_local(f);
        json_i64(index, 0, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        depth.load(f);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        result.replace(self.emit_concat_gc_strings(&result, gap, f), f);
        json_increment(index, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(index, f);
        Ok(result)
    }
    fn emit_json_append_literal(
        &mut self,
        output: &StringOutputConstruction,
        text: &str,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        output.append_literal(text, self.runtime_schema(), f);
        Ok(())
    }
    fn emit_json_serialize_container(
        &mut self,
        value: &ValueLocals,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        seen: &GcLocal<ValueArray>,
        realm: &GcLocal<RealmRecord>,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let count = s.reserve_i32_local(f);
        let scan_index = s.reserve_i32_local(f);
        let entry = s.reserve_value_local(f);
        s.array_type::<ValueArray>().length(seen, s, f);
        count.store(f);
        json_i32(scan_index, 0, f);
        let scan_done = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        scan_index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_branch_to_target(scan_done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_argument_vector_entry_to_value(seen, scan_index, &entry, f);
        self.emit_tagged_payload_same_value_i32(value, &entry, f)?;
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_error(
            realm,
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CONVERTING_CIRCULAR_STRUCTURE_TO_JSON,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        scan_index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        scan_index.store(f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let path = ArgumentListConstruction::new(self, f);
        self.emit_json_copy_list(&path, seen, f);
        path.append(value, s, f);
        let descendants = path.finish(self, f);
        let gap = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>()
                .field(JsonStringifyContextSchema::GAP)
                .read(context, s, f)
                .reference(),
            f,
        );
        let gap_length = s.reserve_i64_local(f);
        self.emit_native_gc_string_length(&gap, gap_length, f);
        let child_depth = s.reserve_i64_local(f);
        indent.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        child_depth.store(f);
        let prefix = self.emit_json_indent(&gap, child_depth, f)?;
        let parent_prefix = self.emit_json_indent(&gap, indent, f)?;
        let output = StringOutputConstruction::new(s, f);
        let result = s.reserve_value_local(f);
        let key_value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let is_array = s.reserve_i32_local(f);
        let emitted = s.reserve_i32_local(f);
        let index = s.reserve_i64_local(f);
        let length = s.reserve_i64_local(f);
        json_i32(emitted, 0, f);
        json_i64(index, 0, f);
        let keys = s
            .reserve_gc_local::<ValueArray, Nullable>(f)
            .initialize_null(s, f);
        self.emit_is_array_i32(value, is_array, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        is_array.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(&output, "[", f)?;
        let length_key = self.emit_json_string("length", f)?;
        self.emit_json_get(value, &length_key, &entry, f)?;
        self.emit_to_length_i64_from_value_locals(&entry, length, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        length_key.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_json_append_literal(&output, "{", f)?;
        let property_list = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>()
                .field(JsonStringifyContextSchema::PROPERTY_LIST)
                .read(context, s, f)
                .reference(),
            f,
        );
        property_list.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let snapshot = self.emit_json_enumerable_keys(value, f)?;
        keys.replace(snapshot.load(s, f).nullable(), f);
        snapshot.clear(f);
        f.instruction(&Instruction::Else);
        keys.replace(property_list.load(s, f), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        property_list.clear(f);
        let list = s
            .reserve_gc_local(f)
            .initialize(keys.load(s, f).require_non_null(f), f);
        s.array_type::<ValueArray>().length(&list, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        list.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let key = s
            .reserve_gc_local::<StringValue, Nullable>(f)
            .initialize_null(s, f);
        is_array.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let name = self.emit_json_index_key(index, f)?;
        key.replace(name.load(s, f).nullable(), f);
        name.clear(f);
        f.instruction(&Instruction::Else);
        let list = s
            .reserve_gc_local(f)
            .initialize(keys.load(s, f).require_non_null(f), f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        scan_index.store(f);
        self.emit_argument_vector_entry_to_value(&list, scan_index, &key_value, f);
        key.replace(key_value.cast_reference::<StringValue>(s, f).nullable(), f);
        list.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let name = s
            .reserve_gc_local(f)
            .initialize(key.load(s, f).require_non_null(f), f);
        self.emit_json_stringify_call(
            value,
            &name,
            context,
            child_depth,
            &descendants,
            &result,
            f,
        )?;
        is_array.load(f);
        self.emit_json_tag_test(&result, WasmRuntimeValueTag::Undefined, f);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let null = self.emit_json_string("null", f)?;
        result.set_reference(&null, s, f);
        null.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_tag_test(&result, WasmRuntimeValueTag::Undefined, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        emitted.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(&output, ",", f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        gap_length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(&output, "\n", f)?;
        output.append(&prefix, s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        is_array.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let quoted = self.emit_json_quote(&name, f)?;
        output.append(&quoted, s, f);
        quoted.clear(f);
        self.emit_json_append_literal(&output, ":", f)?;
        gap_length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(&output, " ", f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let text = s
            .reserve_gc_local(f)
            .initialize(result.cast_reference::<StringValue>(s, f), f);
        output.append(&text, s, f);
        text.clear(f);
        json_i32(emitted, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        name.clear(f);
        key.clear(f);
        json_increment(index, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        emitted.load(f);
        gap_length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(&output, "\n", f)?;
        output.append(&parent_prefix, s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        is_array.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(&output, "]", f)?;
        f.instruction(&Instruction::Else);
        self.emit_json_append_literal(&output, "}", f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let output = output.finish(s, f);
        out.set_reference(&output, s, f);
        keys.clear(f);
        s.release_i64_local(length, f);
        s.release_i64_local(index, f);
        s.release_i32_local(emitted, f);
        s.release_i32_local(is_array, f);
        pending.clear(f);
        key_value.clear(f);
        result.clear(f);
        output.clear(f);
        parent_prefix.clear(f);
        prefix.clear(f);
        s.release_i64_local(child_depth, f);
        s.release_i64_local(gap_length, f);
        gap.clear(f);
        descendants.clear(f);
        entry.clear(f);
        s.release_i32_local(scan_index, f);
        s.release_i32_local(count, f);
        Ok(())
    }
}
