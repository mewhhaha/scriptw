//! SerializeJSONProperty is the sole registered recursive runtime owner.
//!
//! Every property serialization appends straight into the one output
//! accumulator of its JSON.stringify call; the helper result says only
//! whether anything was appended (Undefined when SerializeJSONProperty
//! returns undefined). An object member's key and separator are appended
//! before its value and truncated away again when the value is undefined.
//! The cycle stack holds the enclosing containers: its live prefix is
//! exactly `indent` entries long, so entering a container overwrites the
//! slot its previous sibling used.
use super::*;

impl FunctionBuilder<'_> {
    /// Calls SerializeJSONProperty and leaves the appended flag in `out`:
    /// Undefined when nothing was appended.
    fn emit_json_stringify_call(
        &mut self,
        holder: &ValueLocals,
        key: &ValueLocals,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let result = s.reserve_completion(f);
        s.call_helper(
            JsonStringifyValueArguments::new(holder, key, context, indent, self.current_program()),
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
        let key = s.reserve_value_local(f);
        let appended = s.reserve_value_local(f);
        let output = s.reserve_value_local(f);
        let callable = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &value, f);
        self.emit_builtin_arg_to_value(1, &replacer, f);
        self.emit_builtin_arg_to_value(2, &space, f);
        let realm = self.emit_execution_realm(f);
        let property_list = s
            .reserve_gc_local::<ValueArray, Nullable>(f)
            .initialize_null(s, f);
        self.emit_is_callable_i32(&replacer, f)?;
        callable.store(f);
        callable.load(f);
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
        let accumulator = s
            .reserve_gc_local(f)
            .initialize(StringAccumulatorCursor::allocate(s, 128, f), f);
        let stack_length = s.reserve_i32_local(f);
        json_i32(stack_length, 8, f);
        let stack = s.reserve_gc_local(f).initialize(
            s.array_type::<JsonStringifyStack>()
                .filled(GcOperand::null_eq(), stack_length, f),
            f,
        );
        s.release_i32_local(stack_length, f);
        let context = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>().construct(
                (
                    GcOperand::reference(&realm, s),
                    GcOperand::reference(&replacement, s),
                    GcOperand::reference(&property_list, s),
                    GcOperand::reference(&gap, s),
                    GcOperand::reference(&accumulator, s),
                    GcOperand::reference(&stack, s),
                ),
                f,
            ),
            f,
        );
        // The wrapper object is observable only as a replacer's receiver.
        // Without one, SerializeJSONProperty starts from the value itself
        // (an Undefined key), and Get(wrapper, "") is never observable.
        callable.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let root = self.emit_json_plain_object(&realm, f)?;
        holder.set_reference(&root, s, f);
        let empty = self.emit_json_string("", f)?;
        self.emit_json_define(&holder, &empty, &value, false, f)?;
        key.set_reference(&empty, s, f);
        empty.clear(f);
        root.clear(f);
        f.instruction(&Instruction::Else);
        holder.copy_from(&value, f);
        key.set_undefined(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let indent = s.reserve_i64_local(f);
        json_i64(indent, 0, f);
        self.emit_json_stringify_call(&holder, &key, &context, indent, &appended, f)?;
        output.set_undefined(f);
        self.emit_json_tag_test(&appended, WasmRuntimeValueTag::Undefined, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let text = s
            .reserve_gc_local(f)
            .initialize(StringAccumulatorCursor::publish(&accumulator, s, f), f);
        output.set_reference(&text, s, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&output, f);
        s.release_i64_local(indent, f);
        context.clear(f);
        stack.clear(f);
        accumulator.clear(f);
        replacement.clear(f);
        gap.clear(f);
        property_list.clear(f);
        realm.clear(f);
        s.release_i32_local(callable, f);
        output.clear(f);
        appended.clear(f);
        key.clear(f);
        holder.clear(f);
        space.clear(f);
        replacer.clear(f);
        value.clear(f);
        Ok(())
    }
    /// The String form of a SerializeJSONProperty key: the actual String,
    /// the decimal spelling of an array index Number, or "" for the root.
    fn emit_json_key_string(
        &mut self,
        key: &ValueLocals,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        out.copy_from(key, f);
        self.emit_json_tag_test(key, WasmRuntimeValueTag::Number, f);
        self.open_frame(ControlFrameKind::If, f);
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_number_to_string_payload(key.scalar(), f)?, f);
        out.set_reference(&text, s, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_tag_test(key, WasmRuntimeValueTag::Undefined, f);
        self.open_frame(ControlFrameKind::If, f);
        let text = self.emit_json_string("", f)?;
        out.set_reference(&text, s, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    /// Get(holder, key). An own data property is the whole result of
    /// ordinary [[Get]], so it is read directly; every other case (holes,
    /// accessors, inherited properties, Proxies, exotic objects) uses Get.
    fn emit_json_property_get(
        &mut self,
        holder: &ValueLocals,
        key: &ValueLocals,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let found = s.reserve_i32_local(f);
        json_i32(found, 0, f);
        self.emit_json_tag_test(key, WasmRuntimeValueTag::Number, f);
        self.open_frame(ControlFrameKind::If, f);
        // Array element keys are integral Numbers below the array length.
        self.emit_json_reference_test::<ArrayObject>(holder, f);
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(holder.cast_reference::<ArrayObject>(s, f), f);
        let index = s.reserve_i64_local(f);
        key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncF64U);
        index.store(f);
        self.emit_array_own_indexed_value(&array, index, out, found, f);
        s.release_i64_local(index, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_number_to_string_payload(key.scalar(), f)?, f);
        self.emit_json_get(holder, &text, out, f)?;
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        let text = s
            .reserve_gc_local(f)
            .initialize(key.cast_reference::<StringValue>(s, f), f);
        let property = PropertyKeyLocals::from_string(s, &text, f);
        let direct = self.emit_ordinary_own_data_read_i32(holder, &property, out, f)?;
        property.clear(f);
        direct.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_get(holder, &text, out, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(direct, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(found, f);
        Ok(())
    }
    fn emit_json_serialize_property(
        &mut self,
        holder: &ValueLocals,
        key: &ValueLocals,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        let method = s.reserve_value_local(f);
        let replacement = s.reserve_value_local(f);
        let key_value = s.reserve_value_local(f);
        let appended = s.reserve_i32_local(f);
        json_i32(appended, 1, f);
        self.emit_json_tag_test(key, WasmRuntimeValueTag::Undefined, f);
        self.open_frame(ControlFrameKind::If, f);
        value.copy_from(holder, f);
        f.instruction(&Instruction::Else);
        self.emit_json_property_get(holder, key, &value, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_is_heap_object_like_tag_i32(value.tag(), f);
        self.emit_json_tag_test(&value, WasmRuntimeValueTag::BigInt, f);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        let to_json = self.emit_json_string("toJSON", f)?;
        self.emit_json_get(&value, &to_json, &method, f)?;
        to_json.clear(f);
        self.emit_is_callable_i32(&method, f)?;
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_key_string(key, &key_value, f)?;
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
        self.emit_json_key_string(key, &key_value, f)?;
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
        let accumulator = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>()
                .field(JsonStringifyContextSchema::OUTPUT)
                .read(context, s, f)
                .reference(),
            f,
        );
        let realm = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>()
                .field(JsonStringifyContextSchema::REALM)
                .read(context, s, f)
                .reference(),
            f,
        );
        let serialized = self.open_frame(ControlFrameKind::Block, f);
        self.emit_json_tag_test(&value, WasmRuntimeValueTag::String, f);
        self.open_frame(ControlFrameKind::If, f);
        let text = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<StringValue>(s, f), f);
        self.emit_json_quote_into(&accumulator, &text, f)?;
        text.clear(f);
        self.emit_branch_to_target(serialized, f);
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
        let output = StringAccumulatorCursor::open(&accumulator, s, f);
        output.append_string(&accumulator, &text, s, f);
        output.close(&accumulator, s, f);
        text.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_json_append_literal(&accumulator, "null", f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(serialized, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_tag_test(&value, WasmRuntimeValueTag::Null, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(&accumulator, "null", f);
        self.emit_branch_to_target(serialized, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_tag_test(&value, WasmRuntimeValueTag::Boolean, f);
        self.open_frame(ControlFrameKind::If, f);
        value.scalar().load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(&accumulator, "false", f);
        f.instruction(&Instruction::Else);
        self.emit_json_append_literal(&accumulator, "true", f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(serialized, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
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
        let output = StringAccumulatorCursor::open(&accumulator, s, f);
        output.append_string(&accumulator, &text, s, f);
        output.close(&accumulator, s, f);
        text.clear(f);
        raw.clear(f);
        self.emit_branch_to_target(serialized, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_is_heap_object_like_tag_i32(value.tag(), f);
        self.emit_is_callable_i32(&value, f)?;
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_serialize_container(&value, context, &accumulator, indent, &realm, f)?;
        self.emit_branch_to_target(serialized, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Undefined, Symbol and callable values serialize to undefined.
        json_i32(appended, 0, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        out.set_undefined(f);
        appended.load(f);
        self.open_frame(ControlFrameKind::If, f);
        out.set_boolean(appended, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        realm.clear(f);
        accumulator.clear(f);
        s.release_i32_local(appended, f);
        key_value.clear(f);
        replacement.clear(f);
        method.clear(f);
        value.clear(f);
        Ok(())
    }
    fn emit_json_append_literal(
        &mut self,
        accumulator: &GcLocal<StringAccumulator>,
        text: &str,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let output = StringAccumulatorCursor::open(accumulator, s, f);
        output.push_literal(accumulator, text, s, f);
        output.close(accumulator, s, f);
    }
    /// "\n" followed by `depth` copies of the gap; only for a nonempty gap.
    fn emit_json_append_indent(
        &mut self,
        accumulator: &GcLocal<StringAccumulator>,
        gap: &GcLocal<StringValue>,
        depth: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let output = StringAccumulatorCursor::open(accumulator, s, f);
        output.push_literal(accumulator, "\n", s, f);
        output.append_repeated_string(accumulator, gap, depth, s, f);
        output.close(accumulator, s, f);
    }
    /// SerializeJSONObject and SerializeJSONArray share the cycle check, the
    /// stack slot and the separator protocol.
    fn emit_json_serialize_container(
        &mut self,
        value: &ValueLocals,
        context: &GcLocal<JsonStringifyContext>,
        accumulator: &GcLocal<StringAccumulator>,
        indent: I64Local,
        realm: &GcLocal<RealmRecord>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let depth = s.reserve_i32_local(f);
        let scan_index = s.reserve_i32_local(f);
        let capacity = s.reserve_i32_local(f);
        indent.load(f);
        f.instruction(&Instruction::I32WrapI64);
        depth.store(f);
        let stack = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>()
                .field(JsonStringifyContextSchema::STACK)
                .read(context, s, f)
                .reference(),
            f,
        );
        // The enclosing containers occupy exactly the first `indent` slots.
        json_i32(scan_index, 0, f);
        let scan_done = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        scan_index.load(f);
        depth.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(scan_done, f);
        let _ = s
            .array_type::<JsonStringifyStack>()
            .read(&stack, scan_index, s, f);
        value.reference().load(f);
        f.instruction(&Instruction::RefEq);
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
        s.array_type::<JsonStringifyStack>().length(&stack, s, f);
        capacity.store(f);
        depth.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        capacity.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        capacity.store(f);
        capacity.load(f);
        depth.load(f);
        f.instruction(&Instruction::I32LeU);
        self.open_frame(ControlFrameKind::If, f);
        depth.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        capacity.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let grown = s.reserve_gc_local(f).initialize(
            s.array_type::<JsonStringifyStack>()
                .filled(GcOperand::null_eq(), capacity, f),
            f,
        );
        s.array_type::<JsonStringifyStack>()
            .copy_prefix_from(&grown, &stack, depth, s, f);
        stack.replace(grown.load(s, f), f);
        grown.clear(f);
        s.struct_type::<JsonStringifyContext>()
            .field(JsonStringifyContextSchema::STACK)
            .write(context, GcOperand::reference(&stack, s), s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.array_type::<JsonStringifyStack>().write(
            &stack,
            depth,
            GcOperand::eq_reference(value.reference()),
            s,
            f,
        );
        stack.clear(f);
        s.release_i32_local(capacity, f);
        s.release_i32_local(scan_index, f);
        s.release_i32_local(depth, f);
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
        let result = s.reserve_value_local(f);
        let key_value = s.reserve_value_local(f);
        let entry = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let is_array = s.reserve_i32_local(f);
        let emitted = s.reserve_i32_local(f);
        let mark = s.reserve_i32_local(f);
        let index = s.reserve_i64_local(f);
        let length = s.reserve_i64_local(f);
        let item = s.reserve_i32_local(f);
        json_i32(emitted, 0, f);
        json_i64(index, 0, f);
        let keys = s
            .reserve_gc_local::<PropertyKeyTable, Nullable>(f)
            .initialize_null(s, f);
        let replacer_keys = s
            .reserve_gc_local::<ValueArray, Nullable>(f)
            .initialize_null(s, f);
        self.emit_is_array_i32(value, is_array, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        is_array.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(accumulator, "[", f);
        // An actual Array's "length" is its own data property.
        self.emit_json_reference_test::<ArrayObject>(value, f);
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<ArrayObject>(s, f), f);
        s.field(ArrayObjectSchema::LENGTH)
            .read(&array, s, f)
            .store_i64(length, f);
        array.clear(f);
        f.instruction(&Instruction::Else);
        let length_key = self.emit_json_string("length", f)?;
        self.emit_json_get(value, &length_key, &entry, f)?;
        self.emit_to_length_i64_from_value_locals(&entry, length, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        length_key.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        self.emit_json_append_literal(accumulator, "{", f);
        let property_list = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>()
                .field(JsonStringifyContextSchema::PROPERTY_LIST)
                .read(context, s, f)
                .reference(),
            f,
        );
        property_list.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let snapshot = self.emit_json_enumerable_key_table(value, item, f)?;
        keys.replace(snapshot.load(s, f).nullable(), f);
        snapshot.clear(f);
        item.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        f.instruction(&Instruction::Else);
        replacer_keys.replace(property_list.load(s, f), f);
        let list = s
            .reserve_gc_local(f)
            .initialize(property_list.load(s, f).require_non_null(f), f);
        s.array_type::<ValueArray>().length(&list, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        list.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        property_list.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        item.store(f);
        // The used length before this member's separator, key and value.
        let output = StringAccumulatorCursor::open(accumulator, s, f);
        output.length().load(f);
        mark.store(f);
        emitted.load(f);
        self.open_frame(ControlFrameKind::If, f);
        output.push_literal(accumulator, ",", s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        gap_length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        output.push_literal(accumulator, "\n", s, f);
        output.append_repeated_string(accumulator, &gap, child_depth, s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        output.close(accumulator, s, f);
        is_array.load(f);
        self.open_frame(ControlFrameKind::If, f);
        index.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        key_value.scalar().store(f);
        key_value.set_number(key_value.scalar(), f);
        f.instruction(&Instruction::Else);
        keys.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let table = s
            .reserve_gc_local(f)
            .initialize(keys.load(s, f).require_non_null(f), f);
        // The snapshot holds only accepted String keys.
        let stored = s.reserve_gc_local(f).initialize(
            s.array_type::<PropertyKeyTable>()
                .read(&table, item, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &key_value, s, f);
        stored.clear(f);
        table.clear(f);
        f.instruction(&Instruction::Else);
        let list = s
            .reserve_gc_local(f)
            .initialize(replacer_keys.load(s, f).require_non_null(f), f);
        self.emit_argument_vector_entry_to_value(&list, item, &key_value, f);
        list.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let name = s
            .reserve_gc_local(f)
            .initialize(key_value.cast_reference::<StringValue>(s, f), f);
        self.emit_json_quote_into(accumulator, &name, f)?;
        name.clear(f);
        self.emit_json_append_literal(accumulator, ":", f);
        gap_length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(accumulator, " ", f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_stringify_call(value, &key_value, context, child_depth, &result, f)?;
        self.emit_json_tag_test(&result, WasmRuntimeValueTag::Undefined, f);
        self.open_frame(ControlFrameKind::If, f);
        is_array.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(accumulator, "null", f);
        json_i32(emitted, 1, f);
        f.instruction(&Instruction::Else);
        // An undefined member contributes neither separator nor key.
        let output = StringAccumulatorCursor::open(accumulator, s, f);
        output.truncate(mark, f);
        output.close(accumulator, s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        json_i32(emitted, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
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
        self.emit_json_append_indent(accumulator, &gap, indent, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        is_array.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(accumulator, "]", f);
        f.instruction(&Instruction::Else);
        self.emit_json_append_literal(accumulator, "}", f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        replacer_keys.clear(f);
        keys.clear(f);
        s.release_i32_local(item, f);
        s.release_i64_local(length, f);
        s.release_i64_local(index, f);
        s.release_i32_local(mark, f);
        s.release_i32_local(emitted, f);
        s.release_i32_local(is_array, f);
        pending.clear(f);
        entry.clear(f);
        key_value.clear(f);
        result.clear(f);
        s.release_i64_local(child_depth, f);
        s.release_i64_local(gap_length, f);
        gap.clear(f);
        Ok(())
    }
}
