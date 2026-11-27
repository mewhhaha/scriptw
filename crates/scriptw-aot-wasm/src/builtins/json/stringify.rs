//! SerializeJSONProperty is owned by two registered recursive runtime
//! helpers: this general one, which performs every step for every context,
//! and the hot entry of a plain context (`stringify_plain`), which serializes
//! actual Arrays and ordinary objects whose chains provably lack "toJSON".
//! A declined entry tail-calls this helper. A traversal can also call it
//! for one fetched member, then resume from the same output cursor. In a
//! plain context the general container walk hands its members to the hot entry.
//!
//! Every property serialization appends straight into the one output
//! accumulator of its JSON.stringify call; the helper result says only
//! whether anything was appended (Undefined when SerializeJSONProperty
//! returns undefined). An object member's key and separator are appended
//! before its value and truncated away again when the value is undefined.
//! The cycle stack holds the enclosing containers: its live prefix is
//! exactly `indent` entries long, so entering a container overwrites the
//! slot its previous sibling used.
//!
//! A holder is observable only as a replacer function's receiver. Without
//! one, the container walk performs each member's Get itself and passes the
//! value as `fetched`: a plain data object's members come from its compact
//! storage while that storage is still the object's, primitive members are
//! appended in place, and only the remaining values enter the helper.
use super::*;

pub(super) const JSON_INITIAL_PATH_CAPACITY: i32 = 8;

impl FunctionBuilder<'_> {
    /// Calls SerializeJSONProperty and leaves the appended flag in `out`:
    /// Undefined when nothing was appended. With `fetched` set, `holder` is
    /// already the result of Get(holder, key), which only a caller without a
    /// replacer function may pass.
    pub(super) fn emit_json_stringify_call(
        &mut self,
        holder: &ValueLocals,
        key: &ValueLocals,
        fetched: I32Local,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let result = s.reserve_completion(f);
        // Nested containers recurse through this helper as deeply as the
        // value nests: a guarded recursive edge (see
        // `emit_native_recursion_guard`).
        s.emit_native_recursion_exhausted_i32(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_runtime_error(
            NativeErrorKind::RangeError,
            RuntimeErrorMessage::MAXIMUM_NATIVE_RECURSION_DEPTH_EXCEEDED,
            &result,
            f,
        )?;
        f.instruction(&Instruction::Else);
        let base = self.runtime_helper_base()?;
        let arguments = JsonStringifyValueArguments::new(
            holder,
            key,
            fetched,
            context,
            indent,
            self.current_program(),
        );
        s.emit_native_recursion_counted(f, |f| {
            s.call_helper(arguments, base, f).store(&result, f);
            Ok::<(), EmitError>(())
        })?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
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
            parameters.fetched,
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
    fn emit_json_default_stringify_context(
        &self,
        realm: &GcLocal<RealmRecord>,
        gap: &GcLocal<StringValue>,
        accumulator: &GcLocal<StringAccumulator>,
        f: &mut Function,
    ) -> GcLocal<JsonStringifyContext> {
        let s = self.runtime_schema();
        let property_list = s
            .reserve_gc_local::<ValueArray, Nullable>(f)
            .initialize_null(s, f);
        let replacement = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        let stack_length = s.reserve_i32_local(f);
        stack_length.set_constant(JSON_INITIAL_PATH_CAPACITY, f);
        let stack = s.reserve_gc_local(f).initialize(
            s.array_type::<JsonStringifyStack>()
                .filled(GcOperand::null_eq(), stack_length, f),
            f,
        );
        s.release_i32_local(stack_length, f);
        let context = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>().construct(
                (
                    GcOperand::reference(realm, s),
                    GcOperand::reference(&replacement, s),
                    GcOperand::reference(&property_list, s),
                    GcOperand::reference(gap, s),
                    GcOperand::reference(accumulator, s),
                    GcOperand::reference(&stack, s),
                ),
                f,
            ),
            f,
        );
        stack.clear(f);
        replacement.clear(f);
        property_list.clear(f);
        context
    }
    pub(super) fn emit_json_stringify_entry(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        let replacer = s.reserve_value_local(f);
        let space = s.reserve_value_local(f);
        let key = s.reserve_value_local(f);
        let appended = s.reserve_value_local(f);
        let output = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &value, f);
        self.emit_builtin_arg_to_value(1, &replacer, f);
        self.emit_builtin_arg_to_value(2, &space, f);
        let realm = self.emit_execution_realm(f);
        let indent = s.reserve_i64_local(f);
        json_i64(indent, 0, f);
        let accumulator_opt = s
            .reserve_gc_local::<StringAccumulator, Nullable>(f)
            .initialize_null(s, f);
        // Fast path when both `replacer` and `space` are `undefined`: skip
        // callable, property-list, gap, wrapper-holder and plain-context checks.
        replacer.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        space.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let accumulator = s
            .reserve_gc_local(f)
            .initialize(StringAccumulatorCursor::allocate(s, 128, f), f);
        accumulator_opt.replace(accumulator.load(s, f).nullable(), f);
        let root_done = self.open_frame(ControlFrameKind::Block, f);
        let general_root = self.open_frame(ControlFrameKind::Block, f);
        self.emit_json_compact_root_or_branch(&value, &accumulator, general_root, f)?;
        appended.set_scalar(ScalarValue::Boolean(true), f);
        self.emit_branch_to_target(root_done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let empty = self.emit_json_string("", f)?;
        key.set_reference(&empty, s, f);
        let context = self.emit_json_default_stringify_context(&realm, &empty, &accumulator, f);
        self.emit_json_stringify_plain_call(&value, &key, &context, indent, &appended, f)?;
        context.clear(f);
        empty.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        accumulator.clear(f);
        f.instruction(&Instruction::Else);
        let holder = s.reserve_value_local(f);
        let callable = s.reserve_i32_local(f);
        let property_list = s
            .reserve_gc_local::<ValueArray, Nullable>(f)
            .initialize_null(s, f);
        callable.set_constant(0, f);
        replacer.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
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
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let gap_opt = s
            .reserve_gc_local::<StringValue, Nullable>(f)
            .initialize_null(s, f);
        space.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let empty_gap = self.emit_json_string("", f)?;
        gap_opt.replace(empty_gap.load(s, f).nullable(), f);
        empty_gap.clear(f);
        f.instruction(&Instruction::Else);
        let prepared_gap = self.emit_json_prepare_gap(&space, f)?;
        gap_opt.replace(prepared_gap.load(s, f).nullable(), f);
        prepared_gap.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let gap = s
            .reserve_gc_local(f)
            .initialize(gap_opt.load(s, f).require_non_null(f), f);
        gap_opt.clear(f);
        let replacement_opt = s
            .reserve_gc_local::<StoredValue, Nullable>(f)
            .initialize_null(s, f);
        replacer.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        replacement_opt.replace(s.stored_undefined(f).nullable(), f);
        f.instruction(&Instruction::Else);
        replacement_opt.replace(
            s.struct_type::<StoredValue>()
                .from_value(&replacer, f)
                .nullable(),
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let replacement = s
            .reserve_gc_local(f)
            .initialize(replacement_opt.load(s, f).require_non_null(f), f);
        replacement_opt.clear(f);
        let accumulator = s
            .reserve_gc_local(f)
            .initialize(StringAccumulatorCursor::allocate(s, 128, f), f);
        accumulator_opt.replace(accumulator.load(s, f).nullable(), f);
        let stack_length = s.reserve_i32_local(f);
        json_i32(stack_length, JSON_INITIAL_PATH_CAPACITY, f);
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
        // Without one, SerializeJSONProperty starts from the value itself,
        // already fetched: Get(wrapper, "") is never observable.
        let empty = self.emit_json_string("", f)?;
        key.set_reference(&empty, s, f);
        callable.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let root = self.emit_json_plain_object(&realm, f)?;
        holder.set_reference(&root, s, f);
        self.emit_json_define(&holder, &empty, &value, false, f)?;
        root.clear(f);
        f.instruction(&Instruction::Else);
        holder.copy_from(&value, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        empty.clear(f);
        let fetched = s.reserve_i32_local(f);
        callable.load(f);
        f.instruction(&Instruction::I32Eqz);
        fetched.store(f);
        // A plain context (no replacer function, no property list, an empty
        // gap) serializes through the hot entry, which also serves every
        // nested value; the general helper serves every other context.
        let plain = self.emit_json_plain_context_i32(callable, &property_list, &gap, f);
        plain.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_stringify_plain_call(&holder, &key, &context, indent, &appended, f)?;
        f.instruction(&Instruction::Else);
        self.emit_json_stringify_call(&holder, &key, fetched, &context, indent, &appended, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(plain, f);
        s.release_i32_local(fetched, f);
        context.clear(f);
        stack.clear(f);
        accumulator.clear(f);
        replacement.clear(f);
        gap.clear(f);
        property_list.clear(f);
        s.release_i32_local(callable, f);
        holder.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        output.set_undefined(f);
        self.emit_json_tag_test(&appended, WasmRuntimeValueTag::Undefined, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let accumulator = s
            .reserve_gc_local(f)
            .initialize(accumulator_opt.load(s, f).require_non_null(f), f);
        let text = s
            .reserve_gc_local(f)
            .initialize(StringAccumulatorCursor::publish(&accumulator, s, f), f);
        output.set_reference(&text, s, f);
        text.clear(f);
        accumulator.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&output, f);
        accumulator_opt.clear(f);
        s.release_i64_local(indent, f);
        realm.clear(f);
        output.clear(f);
        appended.clear(f);
        key.clear(f);
        space.clear(f);
        replacer.clear(f);
        value.clear(f);
        Ok(())
    }
    /// The String form of a SerializeJSONProperty key: the actual String or
    /// the decimal spelling of an array index Number.
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
            .initialize(self.emit_json_index_string(key.scalar(), f)?, f);
        out.set_reference(&text, s, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    /// Get(holder, key). An own data property is the whole result of
    /// ordinary [[Get]], so it is read directly; every other case (holes,
    /// accessors, inherited properties, Proxies, exotic objects) uses Get.
    pub(super) fn emit_json_property_get(
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
            .initialize(self.emit_json_index_string(key.scalar(), f)?, f);
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

    /// The numeric form of a property key is used only for Array indices.
    pub(super) fn emit_json_index_string(
        &mut self,
        bits: I64Local,
        f: &mut Function,
    ) -> Result<GcStackReference<StringValue>, EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i32_local(f);
        let unit = s.reserve_i32_local(f);
        let text = s
            .reserve_gc_local::<StringValue, Nullable>(f)
            .initialize_null(s, f);
        bits.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I32TruncSatF64U);
        index.store(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(100));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32RemU);
        f.instruction(&Instruction::I32Const(i32::from(b'0')));
        f.instruction(&Instruction::I32Add);
        unit.store(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        let construction = StringConstruction::from_code_unit(s, s.reserve_gc_local(f), unit, f);
        text.replace(construction.publish(s, f).nullable(), f);
        f.instruction(&Instruction::Else);
        let tens = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32DivU);
        f.instruction(&Instruction::I32Const(i32::from(b'0')));
        f.instruction(&Instruction::I32Add);
        tens.store(f);
        let construction =
            StringConstruction::from_code_unit_pair(s, s.reserve_gc_local(f), tens, unit, f);
        text.replace(construction.publish(s, f).nullable(), f);
        s.release_i32_local(tens, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        text.replace(self.emit_number_to_string_payload(bits, f)?.nullable(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(unit, f);
        s.release_i32_local(index, f);
        let result = text.load(s, f).require_non_null(f);
        text.clear(f);
        Ok(result)
    }

    fn emit_json_serialize_property(
        &mut self,
        holder: &ValueLocals,
        key: &ValueLocals,
        fetched: I32Local,
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
        fetched.load(f);
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
        // Get(value, "toJSON"). The call-free walk answers from the live
        // chain exactly when OrdinaryGet finds a data property or none
        // without running anything; every other value uses Get.
        key_value.set_reference(&to_json, s, f);
        let walked = s.reserve_completion(f);
        let read = self.open_frame(ControlFrameKind::Block, f);
        let answered = self.open_frame(ControlFrameKind::Block, f);
        self.emit_computed_read_walk(&value, &key_value, &walked, answered, f);
        self.emit_json_get(&value, &to_json, &method, f)?;
        self.emit_branch_to_target(read, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        method.copy_from(walked.value(), f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        walked.clear(f);
        key_value.set_undefined(f);
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
        self.emit_json_serialize_primitive(&value, &accumulator, serialized, f)?;
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
        self.emit_json_serialize_container(&value, context, &accumulator, indent, f)?;
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
    /// Appends a String, Number, null or Boolean value and branches to
    /// `serialized`; every other value falls through with nothing appended.
    pub(super) fn emit_json_serialize_primitive(
        &mut self,
        value: &ValueLocals,
        accumulator: &GcLocal<StringAccumulator>,
        serialized: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.emit_json_serializable_primitive_i32(value, f);
        self.open_frame(ControlFrameKind::If, f);
        let output = StringAccumulatorCursor::open(accumulator, s, f);
        self.emit_json_serialize_primitive_into_cursor(value, accumulator, &output, f)?;
        output.close(accumulator, s, f);
        self.emit_branch_to_target(serialized, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    /// The four scalar/string kinds that append JSON without a property lookup.
    pub(super) fn emit_json_serializable_primitive_i32(
        &self,
        value: &ValueLocals,
        f: &mut Function,
    ) {
        const FIRST: i32 = WasmRuntimeValueTag::Null as i32;
        const LAST: i32 = WasmRuntimeValueTag::String as i32;
        const _: () = {
            assert!(WasmRuntimeValueTag::Boolean as i32 == FIRST + 1);
            assert!(WasmRuntimeValueTag::Number as i32 == FIRST + 2);
            assert!(LAST == FIRST + 3);
        };
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(FIRST));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(LAST - FIRST));
        f.instruction(&Instruction::I32LeU);
    }

    /// Appends an admitted String, Number, null or Boolean using the live cursor.
    pub(super) fn emit_json_serialize_primitive_into_cursor(
        &mut self,
        value: &ValueLocals,
        accumulator: &GcLocal<StringAccumulator>,
        output: &StringAccumulatorCursor,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let serialized = self.open_frame(ControlFrameKind::Block, f);
        self.emit_json_tag_test(value, WasmRuntimeValueTag::String, f);
        self.open_frame(ControlFrameKind::If, f);
        let text = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<StringValue>(s, f), f);
        self.emit_json_quote_into_cursor(accumulator, output, &text, None, f);
        text.clear(f);
        self.emit_branch_to_target(serialized, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_tag_test(value, WasmRuntimeValueTag::Number, f);
        self.open_frame(ControlFrameKind::If, f);
        value.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Abs);
        f.instruction(&Instruction::F64Const(f64::INFINITY.into()));
        f.instruction(&Instruction::F64Lt);
        self.open_frame(ControlFrameKind::If, f);
        // An integral Number in the nonnegative I32 range, either zero
        // included, is spelled by its decimal digits alone, which are
        // appended without a String; every other finite Number converts.
        let integer = s.reserve_i32_local(f);
        integer.set_constant(-1, f);
        value.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Abs);
        f.instruction(&Instruction::F64Const(2147483648.0.into()));
        f.instruction(&Instruction::F64Lt);
        self.open_frame(ControlFrameKind::If, f);
        value.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I32TruncF64S);
        integer.store(f);
        integer.load(f);
        f.instruction(&Instruction::F64ConvertI32S);
        value.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Ne);
        self.open_frame(ControlFrameKind::If, f);
        integer.set_constant(-1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        integer.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_small_decimal(accumulator, output, integer, f);
        f.instruction(&Instruction::Else);
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_number_to_string_payload(value.scalar(), f)?, f);
        output.append_string(accumulator, &text, s, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(integer, f);
        f.instruction(&Instruction::Else);
        output.push_literal(accumulator, "null", s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(serialized, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_tag_test(value, WasmRuntimeValueTag::Null, f);
        self.open_frame(ControlFrameKind::If, f);
        output.push_literal(accumulator, "null", s, f);
        self.emit_branch_to_target(serialized, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.scalar().load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        output.push_literal(accumulator, "false", s, f);
        f.instruction(&Instruction::Else);
        output.push_literal(accumulator, "true", s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_json_append_small_decimal(
        &mut self,
        accumulator: &GcLocal<StringAccumulator>,
        output: &StringAccumulatorCursor,
        integer: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        integer.load(f);
        f.instruction(&Instruction::I32Const(100));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        let unit = s.reserve_i32_local(f);
        unit.set_constant(2, f);
        output.reserve(accumulator, unit, s, f);
        integer.load(f);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        integer.load(f);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32DivU);
        f.instruction(&Instruction::I32Const(i32::from(b'0')));
        f.instruction(&Instruction::I32Add);
        unit.store(f);
        output.push(unit, s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        integer.load(f);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32RemU);
        f.instruction(&Instruction::I32Const(i32::from(b'0')));
        f.instruction(&Instruction::I32Add);
        unit.store(f);
        output.push(unit, s, f);
        s.release_i32_local(unit, f);
        f.instruction(&Instruction::Else);
        output.push_decimal(accumulator, integer, s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }
    pub(super) fn emit_json_append_literal(
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
    /// The cycle check of SerializeJSONObject and SerializeJSONArray, then
    /// the push of `value`: the enclosing containers occupy exactly the first
    /// `indent` slots of the stack, so `value` takes slot `indent`, growing
    /// the stack when it is full. A cycle throws the TypeError before
    /// anything of `value` is serialized.
    pub(super) fn emit_json_cycle_enter(
        &mut self,
        value: &ValueLocals,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let depth = s.reserve_i32_local(f);
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
        self.emit_json_cycle_check(value, context, &stack, depth, f)?;
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
        s.release_i32_local(depth, f);
        Ok(())
    }

    /// Compares the candidate with the active prefix, nearest parent first.
    pub(super) fn emit_json_cycle_check(
        &mut self,
        value: &ValueLocals,
        context: &GcLocal<JsonStringifyContext>,
        stack: &GcLocal<JsonStringifyStack>,
        depth: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i32_local(f);
        depth.load(f);
        index.store(f);
        let clear = self.open_frame(ControlFrameKind::Block, f);
        index.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(clear, f);
        let cycle = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        index.store(f);
        s.array_type::<JsonStringifyStack>()
            .read(stack, index, s, f);
        value.reference().load(f);
        f.instruction(&Instruction::RefEq);
        self.emit_branch_if_to_target(cycle, f);
        index.load(f);
        self.emit_branch_if_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(clear, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let realm = s.reserve_gc_local(f).initialize(
            s.field(JsonStringifyContextSchema::REALM)
                .read(context, s, f)
                .reference(),
            f,
        );
        self.emit_json_error(
            &realm,
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CONVERTING_CIRCULAR_STRUCTURE_TO_JSON,
            f,
        )?;
        realm.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(index, f);
        Ok(())
    }
    /// SerializeJSONObject and SerializeJSONArray share the cycle check, the
    /// stack slot and the separator protocol.
    fn emit_json_serialize_container(
        &mut self,
        value: &ValueLocals,
        context: &GcLocal<JsonStringifyContext>,
        accumulator: &GcLocal<StringAccumulator>,
        indent: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.emit_json_cycle_enter(value, context, indent, f)?;
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
        // A plain data object's own keys and values: its compact storage as
        // captured here and that storage's immutable shape keys.
        let plain = s
            .reserve_gc_local::<CompactDataStorage, Nullable>(f)
            .initialize_null(s, f);
        let plain_keys = s
            .reserve_gc_local::<CompactShapeKeys, Nullable>(f)
            .initialize_null(s, f);
        let member = s.reserve_value_local(f);
        let fetched = s.reserve_i32_local(f);
        let direct = s.reserve_i32_local(f);
        // Without a replacer function no holder is observable, so this walk
        // performs each member's Get itself.
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>()
                .field(JsonStringifyContextSchema::REPLACER)
                .read(context, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &entry, s, f);
        stored.clear(f);
        self.emit_json_tag_test(&entry, WasmRuntimeValueTag::Undefined, f);
        direct.store(f);
        entry.set_undefined(f);
        // A container that reaches this general walk in a plain context
        // (a toJSON method's result, a Proxy, an object whose toJSON lookup
        // the hot entry could not prove absent) hands its members back to
        // the hot entry.
        let plain_children = s.reserve_i32_local(f);
        s.struct_type::<JsonStringifyContext>()
            .field(JsonStringifyContextSchema::PROPERTY_LIST)
            .read(context, s, f)
            .reference()
            .is_null(f);
        direct.load(f);
        f.instruction(&Instruction::I32And);
        gap_length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32And);
        plain_children.store(f);
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
        let captured = self.emit_plain_data_object_storage(value, f);
        plain.replace(captured.load(s, f), f);
        captured.clear(f);
        plain.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let snapshot = self.emit_json_enumerable_key_table(value, item, f)?;
        keys.replace(snapshot.load(s, f).nullable(), f);
        snapshot.clear(f);
        item.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        f.instruction(&Instruction::Else);
        // The shape's keys, in order, are EnumerableOwnProperties(value, key).
        let storage = s
            .reserve_gc_local(f)
            .initialize(plain.load(s, f).require_non_null(f), f);
        let shape_keys = self.emit_compact_shape_keys(&storage, f);
        s.array_type::<CompactShapeKeys>().length(&shape_keys, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        plain_keys.replace(shape_keys.load(s, f).nullable(), f);
        shape_keys.clear(f);
        storage.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
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
        plain_keys.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let shape_keys = s
            .reserve_gc_local(f)
            .initialize(plain_keys.load(s, f).require_non_null(f), f);
        let name = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeKeys>()
                .read(&shape_keys, item, s, f)
                .reference(),
            f,
        );
        key_value.set_reference(&name, s, f);
        name.clear(f);
        shape_keys.clear(f);
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
        let serialized = self.open_frame(ControlFrameKind::Block, f);
        member.copy_from(value, f);
        json_i32(fetched, 0, f);
        direct.load(f);
        self.open_frame(ControlFrameKind::If, f);
        // Get(value, key): the captured storage's slot while it is still
        // the object's storage, and otherwise the general own-data read or
        // Get, which is what a member of any other holder uses.
        let got = self.open_frame(ControlFrameKind::Block, f);
        let changed = self.open_frame(ControlFrameKind::Block, f);
        plain.load(s, f).is_null(f);
        self.emit_branch_if_to_target(changed, f);
        let storage = s
            .reserve_gc_local(f)
            .initialize(plain.load(s, f).require_non_null(f), f);
        self.emit_plain_data_object_value_or_branch(value, &storage, item, &entry, changed, f);
        storage.clear(f);
        self.emit_branch_to_target(got, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_json_property_get(value, &key_value, &entry, f)?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // A primitive has no toJSON lookup and, here, no replacer call:
        // SerializeJSONProperty only appends it, or returns undefined.
        json_i32(fetched, 1, f);
        result.set_boolean(fetched, f);
        self.emit_json_serialize_primitive(&entry, accumulator, serialized, f)?;
        result.set_undefined(f);
        self.emit_json_tag_test(&entry, WasmRuntimeValueTag::Undefined, f);
        self.emit_branch_if_to_target(serialized, f);
        member.copy_from(&entry, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // A plain context implies `direct`, so `member` is the fetched value.
        plain_children.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_stringify_plain_call(&member, &key_value, context, child_depth, &result, f)?;
        f.instruction(&Instruction::Else);
        self.emit_json_stringify_call(
            &member,
            &key_value,
            fetched,
            context,
            child_depth,
            &result,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        member.set_undefined(f);
        entry.set_undefined(f);
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
        s.release_i32_local(plain_children, f);
        s.release_i32_local(direct, f);
        s.release_i32_local(fetched, f);
        member.clear(f);
        plain_keys.clear(f);
        plain.clear(f);
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
