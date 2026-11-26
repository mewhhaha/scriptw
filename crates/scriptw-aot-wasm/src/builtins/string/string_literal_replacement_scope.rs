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

    #[allow(clippy::too_many_arguments)]
    fn emit_native_string_replace_literal_plain_fast_path(
        &mut self,
        scope: &StringLiteralReplacementScope,
        input: &GcLocal<StringValue>,
        needle: &GcLocal<StringValue>,
        replacement_string: &GcLocal<StringValue>,
        input_length: I64Local,
        needle_length: I64Local,
        functional: I32Local,
        cursor: I64Local,
        position: I64Local,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        functional.load(f);
        f.instruction(&Instruction::I32Eqz);
        needle_length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let rep_units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(replacement_string, s, f)
                .reference(),
            f,
        );
        let rep_len32 = s.reserve_i32_local(f);
        let zero32 = s.reserve_i32_local(f);
        let dollar = s.reserve_i32_local(f);
        let dollar_pos = s.reserve_i32_local(f);
        s.array_type::<CodeUnitArray>().length(&rep_units, s, f);
        rep_len32.store(f);
        zero32.set_constant(0, f);
        dollar.set_constant(i32::from(b'$'), f);
        s.call_helper(
            crate::runtime_helpers::CodeUnitFindArguments::new(
                &rep_units, zero32, rep_len32, dollar,
            ),
            self.runtime_helper_base()?,
            f,
        )
        .store(dollar_pos, f);
        s.release_i32_local(dollar, f);
        s.release_i32_local(zero32, f);
        dollar_pos.load(f);
        rep_len32.load(f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        match scope {
            StringLiteralReplacementScope::AllOccurrences => {
                let fused = s
                    .call_helper(
                        crate::runtime_helpers::StringSplitJoinArguments::new(
                            input,
                            needle,
                            replacement_string,
                        ),
                        self.runtime_helper_base()?,
                        f,
                    )
                    .bind(s, s.reserve_gc_local(f), f);
                fused.load(s, f).is_null(f);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                let present = s
                    .reserve_gc_local(f)
                    .initialize(fused.load(s, f).require_non_null(f), f);
                self.emit_native_string_normal_reference(&present, output, f);
                present.clear(f);
                fused.set_null(s, f);
                self.emit_branch_to_target(exit, f);
                f.instruction(&Instruction::Else);
                // `needle_length > 0` is already proved above, so null uniquely
                // means the replacement result exceeds `MAX_STRING_LENGTH`.
                // Throw the RangeError immediately instead of building a
                // position list that can exhaust the live heap first.
                self.emit_throw_runtime_error(
                    NativeErrorKind::RangeError,
                    RuntimeErrorMessage::STRING_LENGTH_EXCEEDS_IMPLEMENTATION_LIMIT,
                    output,
                    f,
                )?;
                self.emit_branch_to_target(exit, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                fused.clear(f);
            }
            StringLiteralReplacementScope::FirstOccurrence => {
                cursor.set_constant(0, f);
                self.emit_native_string_index_of(
                    input,
                    needle,
                    input_length,
                    needle_length,
                    cursor,
                    position,
                    f,
                );
                position.load(f);
                f.instruction(&Instruction::I64Const(-1));
                f.instruction(&Instruction::I64Eq);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_native_string_normal_reference(input, output, f);
                self.emit_branch_to_target(exit, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                // Match found: input_length >= needle_length > 0, so
                // (input_length - needle_length) + rep_len cannot overflow u64.
                let total = s.reserve_i64_local(f);
                input_length.load(f);
                needle_length.load(f);
                f.instruction(&Instruction::I64Sub);
                rep_len32.load(f);
                f.instruction(&Instruction::I64ExtendI32U);
                f.instruction(&Instruction::I64Add);
                total.store(f);
                total.load(f);
                f.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
                f.instruction(&Instruction::I64LeU);
                self.open_frame(ControlFrameKind::If, f);
                let total_len32 = s.reserve_i32_local(f);
                total.load(f);
                f.instruction(&Instruction::I32WrapI64);
                total_len32.store(f);
                total_len32.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                let empty = s
                    .reserve_gc_local(f)
                    .initialize(self.emit_interned_string_reference("", f)?, f);
                self.emit_native_string_normal_reference(&empty, output, f);
                empty.clear(f);
                self.emit_branch_to_target(exit, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                let input_units = s.reserve_gc_local(f).initialize(
                    s.field(StringValueSchema::CODE_UNITS)
                        .read(input, s, f)
                        .reference(),
                    f,
                );
                let construction = crate::gc_types::StringConstruction::allocate(
                    s,
                    s.reserve_gc_local(f),
                    total_len32,
                    f,
                );
                let dest = s.reserve_i32_local(f);
                let pos32 = s.reserve_i32_local(f);
                let tail_start32 = s.reserve_i32_local(f);
                let tail_len32 = s.reserve_i32_local(f);
                dest.set_constant(0, f);
                position.load(f);
                f.instruction(&Instruction::I32WrapI64);
                pos32.store(f);
                pos32.load(f);
                self.open_frame(ControlFrameKind::If, f);
                construction.copy_units_from(dest, &input_units, pos32, s, f);
                pos32.load(f);
                dest.store(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                rep_len32.load(f);
                self.open_frame(ControlFrameKind::If, f);
                construction.copy_units_from(dest, &rep_units, rep_len32, s, f);
                dest.load(f);
                rep_len32.load(f);
                f.instruction(&Instruction::I32Add);
                dest.store(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                position.load(f);
                needle_length.load(f);
                f.instruction(&Instruction::I64Add);
                f.instruction(&Instruction::I32WrapI64);
                tail_start32.store(f);
                input_length.load(f);
                f.instruction(&Instruction::I32WrapI64);
                tail_start32.load(f);
                f.instruction(&Instruction::I32Sub);
                tail_len32.store(f);
                tail_len32.load(f);
                self.open_frame(ControlFrameKind::If, f);
                construction.copy_units_range_from(
                    dest,
                    &input_units,
                    tail_start32,
                    tail_len32,
                    s,
                    f,
                );
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                let built = s
                    .reserve_gc_local(f)
                    .initialize(construction.publish(s, f), f);
                self.emit_native_string_normal_reference(&built, output, f);
                built.clear(f);
                s.release_i32_local(tail_len32, f);
                s.release_i32_local(tail_start32, f);
                s.release_i32_local(pos32, f);
                s.release_i32_local(dest, f);
                input_units.clear(f);
                s.release_i32_local(total_len32, f);
                self.emit_branch_to_target(exit, f);
                f.instruction(&Instruction::Else);
                self.emit_throw_runtime_error(
                    NativeErrorKind::RangeError,
                    RuntimeErrorMessage::STRING_LENGTH_EXCEEDS_IMPLEMENTATION_LIMIT,
                    output,
                    f,
                )?;
                self.emit_branch_to_target(exit, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                s.release_i64_local(total, f);
            }
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(dollar_pos, f);
        s.release_i32_local(rep_len32, f);
        rep_units.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
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
        // Fast path for non-callable, non-empty literal replacement when the
        // replacement string contains no '$' substitution sequences.
        self.emit_native_string_replace_literal_plain_fast_path(
            &scope,
            input,
            &needle,
            &replacement_string,
            input_length,
            needle_length,
            functional,
            cursor,
            position,
            output,
            exit,
            f,
        )?;
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
        let split_done = self.open_frame(ControlFrameKind::Block, f);
        // 1. max == 0 -> empty Array (no PackedValueTable allocation).
        max.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let zero_len = s.reserve_i64_local(f);
        zero_len.set_constant(0, f);
        let prototype = self.emit_load_current_function_realm_array_prototype(f);
        let array = s.reserve_gc_local(f).initialize(
            self.emit_alloc_array_with_current_function_realm_prototype(zero_len, prototype, f)?,
            f,
        );
        value.set_reference(&array, s, f);
        output.set_normal(&value, f);
        array.clear(f);
        s.release_i64_local(zero_len, f);
        self.emit_branch_to_target(split_done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // 2. separator === undefined (with max > 0) -> exact 1-element Array [input].
        separator.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_singleton_string_array_to_output(input, output, f)?;
        self.emit_branch_to_target(split_done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // 3. Fast 1-unit separator split (max > 0, separator defined): hoist
        // input_units and sep_unit once and scan in i32 directly with CodeUnitFind.
        needle_length.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        let helper_base = self.runtime_helper_base()?;
        let input_units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(input, s, f)
                .reference(),
            f,
        );
        let needle_units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(&needle, s, f)
                .reference(),
            f,
        );
        let input_len32 = s.reserve_i32_local(f);
        let sep_unit = s.reserve_i32_local(f);
        let last_end32 = s.reserve_i32_local(f);
        let pos32 = s.reserve_i32_local(f);
        input_length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        input_len32.store(f);
        last_end32.set_constant(0, f);
        s.array_type::<CodeUnitArray>()
            .read(&needle_units, last_end32, s, f)
            .store(sep_unit, f);
        needle_units.clear(f);
        s.call_helper(
            crate::runtime_helpers::CodeUnitFindArguments::new(
                &input_units,
                last_end32,
                input_len32,
                sep_unit,
            ),
            helper_base,
            f,
        )
        .store(pos32, f);
        pos32.load(f);
        input_len32.load(f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        // Zero matches (including "".split(",")): exact 1-element Array reusing immutable input.
        self.emit_singleton_string_array_to_output(input, output, f)?;
        f.instruction(&Instruction::Else);
        let fast_list = crate::gc_types::PackedValueListConstruction::allocate(s, f);
        count.set_constant(0, f);
        let fast_list_done = self.open_frame(ControlFrameKind::Block, f);
        let fast_search_done = self.open_frame(ControlFrameKind::Block, f);
        let fast_next = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_code_units_slice_to_value(&input_units, last_end32, pos32, &value, f)?;
        fast_list.append(&value, s, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        count.store(f);
        count.load(f);
        max.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(fast_list_done, f);
        pos32.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        last_end32.store(f);
        s.call_helper(
            crate::runtime_helpers::CodeUnitFindArguments::new(
                &input_units,
                last_end32,
                input_len32,
                sep_unit,
            ),
            helper_base,
            f,
        )
        .store(pos32, f);
        pos32.load(f);
        input_len32.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(fast_search_done, f);
        self.emit_branch_to_target(fast_next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_code_units_slice_to_value(&input_units, last_end32, input_len32, &value, f)?;
        fast_list.append(&value, s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let prototype = self.emit_load_current_function_realm_array_prototype(f);
        let array = s.reserve_gc_local(f).initialize(
            self.emit_publish_packed_value_list_with_current_function_realm_prototype(
                fast_list, prototype, f,
            )?,
            f,
        );
        value.set_reference(&array, s, f);
        output.set_normal(&value, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(pos32, f);
        s.release_i32_local(last_end32, f);
        s.release_i32_local(sep_unit, f);
        s.release_i32_local(input_len32, f);
        input_units.clear(f);
        self.emit_branch_to_target(split_done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // 4. General path: empty separator (needle_length == 0) or multi-unit separator.
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
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
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

    /// CreateArrayFromList(« element ») with an exact 1-element PackedValues
    /// backing (capacity 1, a power of two), reusing the immutable `element`
    /// String with zero slice allocation.
    fn emit_singleton_string_array_to_output(
        &mut self,
        element: &GcLocal<StringValue>,
        output: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let prototype = s
            .reserve_gc_local(f)
            .initialize(self.emit_load_realm_array_prototype(&realm, f), f);
        realm.clear(f);
        let proto_value = s.reserve_value_local(f);
        proto_value.set_reference(&prototype, s, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&proto_value), f)?,
            f,
        );
        proto_value.clear(f);
        prototype.clear(f);
        let elem_value = s.reserve_value_local(f);
        elem_value.set_reference(element, s, f);
        let list = crate::gc_types::PackedValueListConstruction::singleton(&elem_value, s, f);
        let array = s.reserve_gc_local(f).initialize(
            s.struct_type::<ArrayObject>()
                .with_packed_value_list(&header, list, s, f),
            f,
        );
        header.clear(f);
        elem_value.set_reference(&array, s, f);
        output.set_normal(&elem_value, f);
        array.clear(f);
        elem_value.clear(f);
        Ok(())
    }

    /// Slice `units[start..end]` into `destination`, returning the interned
    /// empty String when `start == end` and allocating one exact-size String
    /// otherwise.
    fn emit_code_units_slice_to_value(
        &mut self,
        units: &GcLocal<CodeUnitArray>,
        start: I32Local,
        end: I32Local,
        destination: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let span = s.reserve_i32_local(f);
        end.load(f);
        start.load(f);
        f.instruction(&Instruction::I32Sub);
        span.store(f);
        span.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let empty = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("", f)?, f);
        destination.set_reference(&empty, s, f);
        empty.clear(f);
        f.instruction(&Instruction::Else);
        let zero = s.reserve_i32_local(f);
        zero.set_constant(0, f);
        let construction =
            crate::gc_types::StringConstruction::allocate(s, s.reserve_gc_local(f), span, f);
        construction.copy_units_range_from(zero, units, start, span, s, f);
        s.release_i32_local(zero, f);
        let part = s
            .reserve_gc_local(f)
            .initialize(construction.publish(s, f), f);
        destination.set_reference(&part, s, f);
        part.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(span, f);
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
        let input_len32 = s.reserve_i32_local(&mut f);
        s.array_type::<CodeUnitArray>()
            .length(&input_units, s, &mut f);
        input_len32.store(&mut f);
        input_len32.load(&mut f);
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
        // Fast path for 1-unit separators: hoist sep_unit once and scan in i32
        // directly with CodeUnitFind across both counting and copy passes.
        sep_len.load(&mut f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, &mut f);
        let helper_base = self.runtime_helper_base()?;
        let sep_unit = s.reserve_i32_local(&mut f);
        let at32 = s.reserve_i32_local(&mut f);
        let first_pos32 = s.reserve_i32_local(&mut f);
        let pos32 = s.reserve_i32_local(&mut f);
        let matches32 = s.reserve_i32_local(&mut f);
        at32.set_constant(0, &mut f);
        s.array_type::<CodeUnitArray>()
            .read(&sep_units, at32, s, &mut f)
            .store(sep_unit, &mut f);
        s.call_helper(
            crate::runtime_helpers::CodeUnitFindArguments::new(
                &input_units,
                at32,
                input_len32,
                sep_unit,
            ),
            helper_base,
            &mut f,
        )
        .store(first_pos32, &mut f);
        first_pos32.load(&mut f);
        input_len32.load(&mut f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, &mut f);
        result.replace(p.input.load(s, &mut f).nullable(), &mut f);
        self.emit_branch_to_target(done, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Pass 1: count all matches in i32.
        matches32.set_constant(1, &mut f);
        first_pos32.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        at32.store(&mut f);
        let counted1 = self.open_frame(ControlFrameKind::Block, &mut f);
        let count_loop1 = self.open_frame(ControlFrameKind::Loop, &mut f);
        s.call_helper(
            crate::runtime_helpers::CodeUnitFindArguments::new(
                &input_units,
                at32,
                input_len32,
                sep_unit,
            ),
            helper_base,
            &mut f,
        )
        .store(pos32, &mut f);
        pos32.load(&mut f);
        input_len32.load(&mut f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(counted1, &mut f);
        matches32.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        matches32.store(&mut f);
        pos32.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        at32.store(&mut f);
        self.emit_branch_to_target(count_loop1, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        matches32.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        matches.store(&mut f);
        // Check `input_len - matches + matches * rep_len <= MAX_STRING_LENGTH`
        // without u64 overflow.
        input_len.load(&mut f);
        matches.load(&mut f);
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
        let construction1 = crate::gc_types::StringConstruction::allocate(
            s,
            s.reserve_gc_local(&mut f),
            span,
            &mut f,
        );
        dest.set_constant(0, &mut f);
        src_start.set_constant(0, &mut f);
        first_pos32.load(&mut f);
        pos32.store(&mut f);
        let copied1 = self.open_frame(ControlFrameKind::Block, &mut f);
        let copy_loop1 = self.open_frame(ControlFrameKind::Loop, &mut f);
        pos32.load(&mut f);
        src_start.load(&mut f);
        f.instruction(&Instruction::I32GtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        pos32.load(&mut f);
        src_start.load(&mut f);
        f.instruction(&Instruction::I32Sub);
        span.store(&mut f);
        construction1.copy_units_range_from(dest, &input_units, src_start, span, s, &mut f);
        dest.load(&mut f);
        span.load(&mut f);
        f.instruction(&Instruction::I32Add);
        dest.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        rep_len32.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        construction1.copy_units_from(dest, &rep_units, rep_len32, s, &mut f);
        dest.load(&mut f);
        rep_len32.load(&mut f);
        f.instruction(&Instruction::I32Add);
        dest.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        pos32.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        src_start.store(&mut f);
        s.call_helper(
            crate::runtime_helpers::CodeUnitFindArguments::new(
                &input_units,
                src_start,
                input_len32,
                sep_unit,
            ),
            helper_base,
            &mut f,
        )
        .store(pos32, &mut f);
        pos32.load(&mut f);
        input_len32.load(&mut f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(copied1, &mut f);
        self.emit_branch_to_target(copy_loop1, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        input_len32.load(&mut f);
        src_start.load(&mut f);
        f.instruction(&Instruction::I32GtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        input_len32.load(&mut f);
        src_start.load(&mut f);
        f.instruction(&Instruction::I32Sub);
        span.store(&mut f);
        construction1.copy_units_range_from(dest, &input_units, src_start, span, s, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        result.replace(construction1.publish(s, &mut f).nullable(), &mut f);
        s.release_i32_local(matches32, &mut f);
        s.release_i32_local(pos32, &mut f);
        s.release_i32_local(first_pos32, &mut f);
        s.release_i32_local(at32, &mut f);
        s.release_i32_local(sep_unit, &mut f);
        self.emit_branch_to_target(done, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
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
        s.release_i32_local(input_len32, &mut f);
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
