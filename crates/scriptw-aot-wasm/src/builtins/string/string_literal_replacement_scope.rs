use super::regexp_substitution::{NamedSubstitutionCaptures, SubstitutionCaptures};
use super::replacement_pieces::ReplacementPieces;
use super::*;
use crate::functions::ArgumentListConstruction;

enum StringLiteralReplacementScope {
    FirstOccurrence,
    AllOccurrences,
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_native_string_equal_at(
        &mut self,
        input: &GcLocal<StringValue>,
        needle: &GcLocal<StringValue>,
        input_length: I64Local,
        needle_length: I64Local,
        start: I64Local,
        equal: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let index = s.reserve_i64_local(f);
        let address = s.reserve_i64_local(f);
        let unit = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        equal.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        start.load(f);
        input_length.load(f);
        f.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(done, f);
        needle_length.load(f);
        input_length.load(f);
        start.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(done, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let compared = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        needle_length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(compared, f);
        start.load(f);
        index.load(f);
        f.instruction(&Instruction::I64Add);
        address.store(f);
        self.emit_gc_string_code_unit_i32(input, address, f);
        unit.store(f);
        self.emit_gc_string_code_unit_i32(needle, index, f);
        unit.load(f);
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(done, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(1));
        equal.store(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(unit, f);
        s.release_i64_local(address, f);
        s.release_i64_local(index, f);
    }

    pub(super) fn emit_native_string_index_of(
        &mut self,
        input: &GcLocal<StringValue>,
        needle: &GcLocal<StringValue>,
        input_length: I64Local,
        needle_length: I64Local,
        from: I64Local,
        found: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let cursor = s.reserve_i64_local(f);
        let bound = s.reserve_i64_local(f);
        let equal = s.reserve_i32_local(f);
        f.instruction(&Instruction::I64Const(-1));
        found.store(f);
        from.load(f);
        cursor.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        needle_length.load(f);
        input_length.load(f);
        f.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(done, f);
        input_length.load(f);
        needle_length.load(f);
        f.instruction(&Instruction::I64Sub);
        bound.store(f);
        let units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(input, s, f)
                .reference(),
            f,
        );
        let at = s.reserve_i32_local(f);
        let first = s.reserve_i32_local(f);
        first.set_constant(0, f);
        needle_length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let needle_units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(needle, s, f)
                .reference(),
            f,
        );
        at.set_constant(0, f);
        s.array_type::<CodeUnitArray>()
            .read(&needle_units, at, s, f)
            .store(first, f);
        needle_units.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let last = s.reserve_i32_local(f);
        bound.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I32WrapI64);
        last.store(f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(f);
        bound.load(f);
        f.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(done, f);
        // A position whose first unit differs cannot match a nonempty needle:
        // the leaf kernel skips to the next position holding that unit, or
        // to one past the last position that leaves room for the needle.
        // Its first unit equal, a one-unit needle matches without a compare.
        needle_length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        cursor.load(f);
        f.instruction(&Instruction::I32WrapI64);
        at.store(f);
        s.call_helper(
            crate::runtime_helpers::CodeUnitFindArguments::new(&units, at, last, first),
            self.runtime_helper_base()
                .expect("native string search requires its registered find helper"),
            f,
        )
        .store(at, f);
        at.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        cursor.store(f);
        at.load(f);
        last.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, f);
        needle_length.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        cursor.load(f);
        found.store(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_native_string_equal_at(
            input,
            needle,
            input_length,
            needle_length,
            cursor,
            equal,
            f,
        );
        equal.load(f);
        self.open_frame(ControlFrameKind::If, f);
        cursor.load(f);
        found.store(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        cursor.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        s.release_i32_local(last, f);
        s.release_i32_local(first, f);
        s.release_i32_local(at, f);
        units.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(equal, f);
        s.release_i64_local(bound, f);
        s.release_i64_local(cursor, f);
    }

    pub(super) fn emit_string_replace_literal_first_occurrence_from_string_locals(
        &mut self,
        input: &GcLocal<StringValue>,
        search: &ValueLocals,
        replacement: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_replace_literal(
            StringLiteralReplacementScope::FirstOccurrence,
            input,
            search,
            replacement,
            output,
            exit,
            f,
        )
    }
    pub(super) fn emit_string_replace_literal_all_occurrences_from_string_locals(
        &mut self,
        input: &GcLocal<StringValue>,
        search: &ValueLocals,
        replacement: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_replace_literal(
            StringLiteralReplacementScope::AllOccurrences,
            input,
            search,
            replacement,
            output,
            exit,
            f,
        )
    }

    fn emit_native_string_replace_literal(
        &mut self,
        scope: StringLiteralReplacementScope,
        input: &GcLocal<StringValue>,
        search: &ValueLocals,
        replacement: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        let value = s.reserve_value_local(f);
        let matched_value = s.reserve_value_local(f);
        let position_value = s.reserve_value_local(f);
        let input_value = s.reserve_value_local(f);
        let undefined = s.reserve_value_local(f);
        let input_length = s.reserve_i64_local(f);
        let needle_length = s.reserve_i64_local(f);
        let cursor = s.reserve_i64_local(f);
        let position = s.reserve_i64_local(f);
        let advance = s.reserve_i64_local(f);
        let end_of_last_match = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let bits = s.reserve_i64_local(f);
        let ordinal = s.reserve_i32_local(f);
        let functional = s.reserve_i32_local(f);
        self.emit_value_to_string_payload(search, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        let needle = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        self.emit_is_callable_i32(replacement, f)?;
        functional.store(f);
        let replacement_string = s
            .reserve_gc_local(f)
            .initialize(self.emit_native_string_static("", f), f);
        functional.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_string_payload(replacement, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        replacement_string.replace(pending.value().cast_reference::<StringValue>(s, f), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_native_gc_string_length(input, input_length, f);
        self.emit_native_gc_string_length(&needle, needle_length, f);
        needle_length.load(f);
        advance.store(f);
        advance.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(1));
        advance.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Complete the position List before the first replacement callback.
        let positions = ArgumentListConstruction::new(self, f);
        f.instruction(&Instruction::I64Const(0));
        cursor.store(f);
        let positions_done = self.open_frame(ControlFrameKind::Block, f);
        let position_next = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_native_string_index_of(
            input,
            &needle,
            input_length,
            needle_length,
            cursor,
            position,
            f,
        );
        position.load(f);
        f.instruction(&Instruction::I64Const(-1));
        f.instruction(&Instruction::I64Eq);
        self.emit_branch_if_to_target(positions_done, f);
        position.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        bits.store(f);
        value.set_number(bits, f);
        positions.append(&value, s, f);
        match scope {
            StringLiteralReplacementScope::FirstOccurrence => {
                self.emit_branch_to_target(positions_done, f)
            }
            StringLiteralReplacementScope::AllOccurrences => {
                position.load(f);
                advance.load(f);
                f.instruction(&Instruction::I64Add);
                cursor.store(f);
                self.emit_branch_to_target(position_next, f);
            }
        }
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let positions = positions.finish(self, f);
        s.array_type::<ValueArray>().length(&positions, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        count.store(f);
        // Search/replacement conversions and the entire positions List already
        // completed. No match needs no result assembly or further user code.
        count.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_native_string_normal_reference(input, output, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let captures = SubstitutionCaptures::empty(s, f);
        let named = NamedSubstitutionCaptures::absent(s, f);
        let pieces = ReplacementPieces::new(self, f);
        f.instruction(&Instruction::I64Const(0));
        end_of_last_match.store(f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        matched_value.set_reference(&needle, s, f);
        input_value.set_reference(input, s, f);
        undefined.set_undefined(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        ordinal.store(f);
        self.emit_argument_vector_entry_to_value(&positions, ordinal, &position_value, f);
        position_value.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncSatF64U);
        position.store(f);
        let preserved = s.reserve_gc_local(f).initialize(
            self.emit_gc_string_slice(input, end_of_last_match, position, f),
            f,
        );
        functional.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let arguments =
            self.emit_pre_evaluated_arg_vector(&[&matched_value, &position_value, &input_value], f);
        self.emit_function_or_proxy_call_with_argv(
            replacement,
            &undefined,
            &arguments,
            &pending,
            f,
        )?;
        arguments.clear(f);
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        value.copy_from(pending.value(), f);
        self.emit_value_to_string_payload(&value, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        let converted = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        pieces.append(self, &preserved, output, exit, f)?;
        pieces.append(self, &converted, output, exit, f)?;
        converted.clear(f);
        f.instruction(&Instruction::Else);
        let substituted = self.emit_regexp_get_substitution(
            &replacement_string,
            input,
            &needle,
            position,
            &captures,
            &named,
            output,
            exit,
            f,
        )?;
        pieces.append(self, &preserved, output, exit, f)?;
        pieces.append(self, &substituted, output, exit, f)?;
        substituted.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        preserved.clear(f);
        position.load(f);
        needle_length.load(f);
        f.instruction(&Instruction::I64Add);
        end_of_last_match.store(f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let tail = s.reserve_gc_local(f).initialize(
            self.emit_gc_string_slice(input, end_of_last_match, input_length, f),
            f,
        );
        pieces.append(self, &tail, output, exit, f)?;
        tail.clear(f);
        let accumulated = pieces.publish(self, f);
        self.emit_native_string_normal_reference(&accumulated, output, f);
        accumulated.clear(f);
        named.clear(f);
        captures.clear(f);
        positions.clear(f);
        replacement_string.clear(f);
        needle.clear(f);
        s.release_i32_local(functional, f);
        s.release_i32_local(ordinal, f);
        for local in [
            bits,
            index,
            count,
            end_of_last_match,
            advance,
            position,
            cursor,
            needle_length,
            input_length,
        ] {
            s.release_i64_local(local, f);
        }
        undefined.clear(f);
        input_value.clear(f);
        position_value.clear(f);
        matched_value.clear(f);
        value.clear(f);
        pending.clear(f);
        Ok(())
    }

    pub(super) fn emit_native_string_split_literal(
        &mut self,
        input: &GcLocal<StringValue>,
        separator: &ValueLocals,
        limit: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        let value = s.reserve_value_local(f);
        let input_length = s.reserve_i64_local(f);
        let needle_length = s.reserve_i64_local(f);
        let max = s.reserve_i64_local(f);
        let cursor = s.reserve_i64_local(f);
        let position = s.reserve_i64_local(f);
        let last_end = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        limit.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        max.store(f);
        f.instruction(&Instruction::Else);
        self.emit_value_to_number_payload(limit, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        self.emit_to_uint32_i64_from_number_payload(pending.value().scalar(), max, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Separator conversion still runs when the converted limit is zero.
        self.emit_value_to_string_payload(separator, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        let needle = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        self.emit_native_gc_string_length(input, input_length, f);
        self.emit_native_gc_string_length(&needle, needle_length, f);
        // The result List is private until CreateArrayFromList returns it,
        // so each piece is appended once into the Array's own packed backing.
        let list = crate::gc_types::PackedValueListConstruction::allocate(s, f);
        f.instruction(&Instruction::I64Const(0));
        count.store(f);
        f.instruction(&Instruction::I64Const(0));
        last_end.store(f);
        f.instruction(&Instruction::I64Const(0));
        cursor.store(f);
        let list_done = self.open_frame(ControlFrameKind::Block, f);
        max.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(list_done, f);
        separator.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        value.set_reference(input, s, f);
        list.append(&value, s, f);
        self.emit_branch_to_target(list_done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        needle_length.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let units_next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(f);
        input_length.load(f);
        f.instruction(&Instruction::I64GeU);
        count.load(f);
        max.load(f);
        f.instruction(&Instruction::I64GeU);
        f.instruction(&Instruction::I32Or);
        self.emit_branch_if_to_target(list_done, f);
        cursor.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        end.store(f);
        let unit = s
            .reserve_gc_local(f)
            .initialize(self.emit_gc_string_slice(input, cursor, end, f), f);
        value.set_reference(&unit, s, f);
        list.append(&value, s, f);
        unit.clear(f);
        end.load(f);
        cursor.store(f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        count.store(f);
        self.emit_branch_to_target(units_next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let search_done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_native_string_index_of(
            input,
            &needle,
            input_length,
            needle_length,
            cursor,
            position,
            f,
        );
        position.load(f);
        f.instruction(&Instruction::I64Const(-1));
        f.instruction(&Instruction::I64Eq);
        self.emit_branch_if_to_target(search_done, f);
        let part = s
            .reserve_gc_local(f)
            .initialize(self.emit_gc_string_slice(input, last_end, position, f), f);
        value.set_reference(&part, s, f);
        list.append(&value, s, f);
        part.clear(f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        count.store(f);
        count.load(f);
        max.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(list_done, f);
        position.load(f);
        needle_length.load(f);
        f.instruction(&Instruction::I64Add);
        cursor.store(f);
        cursor.load(f);
        last_end.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let tail = s.reserve_gc_local(f).initialize(
            self.emit_gc_string_slice(input, last_end, input_length, f),
            f,
        );
        value.set_reference(&tail, s, f);
        list.append(&value, s, f);
        tail.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let prototype = self.emit_load_current_function_realm_array_prototype(f);
        let array = s.reserve_gc_local(f).initialize(
            self.emit_publish_packed_value_list_with_current_function_realm_prototype(
                list, prototype, f,
            )?,
            f,
        );
        value.set_reference(&array, s, f);
        output.set_normal(&value, f);
        array.clear(f);
        needle.clear(f);
        for local in [
            end,
            count,
            last_end,
            position,
            cursor,
            max,
            needle_length,
            input_length,
        ] {
            s.release_i64_local(local, f);
        }
        value.clear(f);
        pending.clear(f);
        Ok(())
    }

    /// Fused `input.split(separator).join(replacement)` for proven primitive
    /// Strings with a non-empty separator under unchanged Realm protectors.
    /// Returns `input` directly with zero allocations when `separator` does
    /// not occur, builds one exact-size String when it does, or returns null
    /// if the output exceeds `MAX_STRING_LENGTH` so the caller runs the
    /// general path and throws its exact RangeError.
    pub(crate) fn compile_string_split_join_helper(&mut self) -> Result<Function, EmitError> {
        use crate::runtime_helpers::{HelperParameters, StringSplitJoinParameters};
        let mut f = self.begin_helper_body(RuntimeHelperId::StringSplitJoin);
        let p = self.helper_parameters::<StringSplitJoinParameters>(&mut f);
        let s = self.runtime_schema();
        let result = s
            .reserve_gc_local::<StringValue, Nullable>(&mut f)
            .initialize_null(s, &mut f);
        let input_units = s.reserve_gc_local(&mut f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(&p.input, s, &mut f)
                .reference(),
            &mut f,
        );
        let sep_units = s.reserve_gc_local(&mut f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(&p.separator, s, &mut f)
                .reference(),
            &mut f,
        );
        let rep_units = s.reserve_gc_local(&mut f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(&p.replacement, s, &mut f)
                .reference(),
            &mut f,
        );
        let input_len = s.reserve_i64_local(&mut f);
        let sep_len = s.reserve_i64_local(&mut f);
        let rep_len = s.reserve_i64_local(&mut f);
        let first_pos = s.reserve_i64_local(&mut f);
        let pos = s.reserve_i64_local(&mut f);
        let cursor = s.reserve_i64_local(&mut f);
        let matches = s.reserve_i64_local(&mut f);
        let total = s.reserve_i64_local(&mut f);
        let dest = s.reserve_i32_local(&mut f);
        let src_start = s.reserve_i32_local(&mut f);
        let span = s.reserve_i32_local(&mut f);
        let rep_len32 = s.reserve_i32_local(&mut f);
        s.array_type::<CodeUnitArray>()
            .length(&input_units, s, &mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        input_len.store(&mut f);
        s.array_type::<CodeUnitArray>()
            .length(&sep_units, s, &mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        sep_len.store(&mut f);
        s.array_type::<CodeUnitArray>()
            .length(&rep_units, s, &mut f);
        rep_len32.store(&mut f);
        rep_len32.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        rep_len.store(&mut f);
        let done = self.open_frame(ControlFrameKind::Block, &mut f);
        // Empty separator declines to general path.
        sep_len.load(&mut f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(done, &mut f);
        cursor.set_constant(0, &mut f);
        self.emit_native_string_index_of(
            &p.input,
            &p.separator,
            input_len,
            sep_len,
            cursor,
            first_pos,
            &mut f,
        );
        // No match: return the immutable input String with zero allocations.
        first_pos.load(&mut f);
        f.instruction(&Instruction::I64Const(-1));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, &mut f);
        result.replace(p.input.load(s, &mut f).nullable(), &mut f);
        self.emit_branch_to_target(done, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Pass 1: count all non-overlapping matches to compute exact output length.
        matches.set_constant(1, &mut f);
        first_pos.load(&mut f);
        sep_len.load(&mut f);
        f.instruction(&Instruction::I64Add);
        cursor.store(&mut f);
        let counted = self.open_frame(ControlFrameKind::Block, &mut f);
        let count_loop = self.open_frame(ControlFrameKind::Loop, &mut f);
        self.emit_native_string_index_of(
            &p.input,
            &p.separator,
            input_len,
            sep_len,
            cursor,
            pos,
            &mut f,
        );
        pos.load(&mut f);
        f.instruction(&Instruction::I64Const(-1));
        f.instruction(&Instruction::I64Eq);
        self.emit_branch_if_to_target(counted, &mut f);
        self.emit_increment_local(matches, 1, &mut f);
        pos.load(&mut f);
        sep_len.load(&mut f);
        f.instruction(&Instruction::I64Add);
        cursor.store(&mut f);
        self.emit_branch_to_target(count_loop, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // Check `input_len - matches * sep_len + matches * rep_len <= MAX_STRING_LENGTH`
        // without u64 overflow (`matches <= input_len <= u32::MAX` and `rep_len <= u32::MAX`,
        // so `matches * rep_len` never overflows u64).
        input_len.load(&mut f);
        matches.load(&mut f);
        sep_len.load(&mut f);
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64Sub);
        total.store(&mut f);
        matches.load(&mut f);
        rep_len.load(&mut f);
        f.instruction(&Instruction::I64Mul);
        cursor.store(&mut f);
        cursor.load(&mut f);
        f.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
        f.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(done, &mut f);
        total.load(&mut f);
        cursor.load(&mut f);
        f.instruction(&Instruction::I64Add);
        total.store(&mut f);
        total.load(&mut f);
        f.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
        f.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(done, &mut f);
        // Pass 2: allocate one exact-size StringConstruction and copy spans + replacements.
        total.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        span.store(&mut f);
        let construction =
            crate::gc_types::StringConstruction::allocate(s, s.reserve_gc_local(&mut f), span, &mut f);
        dest.set_constant(0, &mut f);
        cursor.set_constant(0, &mut f);
        first_pos.load(&mut f);
        pos.store(&mut f);
        let copied = self.open_frame(ControlFrameKind::Block, &mut f);
        let copy_loop = self.open_frame(ControlFrameKind::Loop, &mut f);
        pos.load(&mut f);
        f.instruction(&Instruction::I64Const(-1));
        f.instruction(&Instruction::I64Eq);
        self.emit_branch_if_to_target(copied, &mut f);
        pos.load(&mut f);
        cursor.load(&mut f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        cursor.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        src_start.store(&mut f);
        pos.load(&mut f);
        cursor.load(&mut f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I32WrapI64);
        span.store(&mut f);
        construction.copy_units_range_from(dest, &input_units, src_start, span, s, &mut f);
        dest.load(&mut f);
        span.load(&mut f);
        f.instruction(&Instruction::I32Add);
        dest.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        rep_len32.load(&mut f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, &mut f);
        construction.copy_units_from(dest, &rep_units, rep_len32, s, &mut f);
        dest.load(&mut f);
        rep_len32.load(&mut f);
        f.instruction(&Instruction::I32Add);
        dest.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        pos.load(&mut f);
        sep_len.load(&mut f);
        f.instruction(&Instruction::I64Add);
        cursor.store(&mut f);
        self.emit_native_string_index_of(
            &p.input,
            &p.separator,
            input_len,
            sep_len,
            cursor,
            pos,
            &mut f,
        );
        self.emit_branch_to_target(copy_loop, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        input_len.load(&mut f);
        cursor.load(&mut f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        cursor.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        src_start.store(&mut f);
        input_len.load(&mut f);
        cursor.load(&mut f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I32WrapI64);
        span.store(&mut f);
        construction.copy_units_range_from(dest, &input_units, src_start, span, s, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        result.replace(construction.publish(s, &mut f).nullable(), &mut f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        result.load(s, &mut f);
        s.release_i32_local(rep_len32, &mut f);
        s.release_i32_local(span, &mut f);
        s.release_i32_local(src_start, &mut f);
        s.release_i32_local(dest, &mut f);
        for local in [
            total, matches, cursor, pos, first_pos, rep_len, sep_len, input_len,
        ] {
            s.release_i64_local(local, &mut f);
        }
        rep_units.clear(&mut f);
        sep_units.clear(&mut f);
        input_units.clear(&mut f);
        result.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
