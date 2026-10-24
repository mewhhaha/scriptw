use super::replacement_pieces::SubstitutionPieces;
use super::*;
use crate::functions::ArgumentListConstruction;

enum LiteralSubstitution {
    Dollar,
    Matched,
    Prefix,
    Suffix,
}

/// The only append operations admit completed Strings or undefined. Raw
/// captures cannot reach GetSubstitution without their required conversions.
#[must_use]
pub(super) struct SubstitutionCaptureList(ArgumentListConstruction);
#[must_use]
pub(super) struct SubstitutionCaptures(GcLocal<ValueArray>);
impl SubstitutionCaptureList {
    pub(super) fn new(b: &FunctionBuilder<'_>, f: &mut Function) -> Self {
        Self(ArgumentListConstruction::new(b, f))
    }
    pub(super) fn append_string(
        &self,
        text: &GcLocal<StringValue>,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        let value = s.reserve_value_local(f);
        value.set_reference(text, s, f);
        self.0.append(&value, s, f);
        value.clear(f);
    }
    pub(super) fn append_undefined(&self, s: &RuntimeSchema, f: &mut Function) {
        let value = s.reserve_value_local(f);
        value.set_undefined(f);
        self.0.append(&value, s, f);
        value.clear(f);
    }
    pub(super) fn finish(
        self,
        b: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> SubstitutionCaptures {
        SubstitutionCaptures(self.0.finish(b, f))
    }
}
impl SubstitutionCaptures {
    /// Literal search has no capture groups. Its internal immutable empty List
    /// can reuse the existing runtime global without creating JS-visible state.
    pub(super) fn empty(s: &RuntimeSchema, f: &mut Function) -> Self {
        Self(s.reserve_gc_local(f).initialize(s.empty_value_array(f), f))
    }

    pub(super) fn values(&self) -> &GcLocal<ValueArray> {
        &self.0
    }
    pub(super) fn clear(self, f: &mut Function) {
        self.0.clear(f)
    }
}
#[must_use]
pub(super) struct NamedSubstitutionCaptures(ValueLocals);
impl NamedSubstitutionCaptures {
    pub(super) fn absent(s: &RuntimeSchema, f: &mut Function) -> Self {
        let value = s.reserve_value_local(f);
        value.set_undefined(f);
        Self(value)
    }
    pub(super) fn clear(self, f: &mut Function) {
        self.0.clear(f)
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_complete_named_substitution_captures(
        &mut self,
        input: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<NamedSubstitutionCaptures, EmitError> {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        value.copy_from(input, f);
        let pending = s.reserve_completion(f);
        input.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_current_function_realm_object_locals(input, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        value.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        pending.clear(f);
        Ok(NamedSubstitutionCaptures(value))
    }

    pub(super) fn emit_regexp_get_substitution(
        &mut self,
        replacement: &GcLocal<StringValue>,
        input: &GcLocal<StringValue>,
        matched: &GcLocal<StringValue>,
        position: I64Local,
        captures: &SubstitutionCaptures,
        named: &NamedSubstitutionCaptures,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let s = self.runtime_schema();
        let replacement_length = s.reserve_i64_local(f);
        let input_length = s.reserve_i64_local(f);
        let matched_length = s.reserve_i64_local(f);
        let capture_count = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let next_index = s.reserve_i64_local(f);
        let after = s.reserve_i64_local(f);
        let consumed = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        let zero = s.reserve_i64_local(f);
        let capture = s.reserve_i64_local(f);
        let candidate = s.reserve_i64_local(f);
        let scan = s.reserve_i64_local(f);
        let name_start = s.reserve_i64_local(f);
        let total_length = s.reserve_i64_local(f);
        let piece_length = s.reserve_i64_local(f);
        let unit = s.reserve_i32_local(f);
        let next_unit = s.reserve_i32_local(f);
        let digit = s.reserve_i32_local(f);
        let found = s.reserve_i32_local(f);
        let ordinal = s.reserve_i32_local(f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        self.emit_native_gc_string_length(replacement, replacement_length, f);
        // Captures and named-capture ToObject admission already completed at
        // the caller. A template with no dollar token is the immutable result.
        let accumulated = s.reserve_gc_local(f).initialize(replacement.load(s, f), f);
        let has_dollar = s.reserve_i32_local(f);
        has_dollar.set_constant(0, f);
        index.set_constant(0, f);
        let scanned = self.open_frame(ControlFrameKind::Block, f);
        let scan_next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        replacement_length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(scanned, f);
        self.emit_gc_string_code_unit_i32(replacement, index, f);
        f.instruction(&Instruction::I32Const(36));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        has_dollar.set_constant(1, f);
        self.emit_branch_to_target(scanned, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(scan_next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        has_dollar.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_native_gc_string_length(input, input_length, f);
        self.emit_native_gc_string_length(matched, matched_length, f);
        s.array_type::<ValueArray>().length(captures.values(), s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        capture_count.store(f);
        f.instruction(&Instruction::I64Const(0));
        zero.store(f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        position.load(f);
        matched_length.load(f);
        f.instruction(&Instruction::I64Add);
        end.store(f);
        end.load(f);
        input_length.load(f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        input_length.load(f);
        end.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        total_length.set_constant(0, f);
        let pieces = SubstitutionPieces::new(self, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        replacement_length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        self.emit_gc_string_code_unit_i32(replacement, index, f);
        unit.store(f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        next_index.store(f);
        f.instruction(&Instruction::I64Const(1));
        consumed.store(f);
        let piece = s.reserve_gc_local(f).initialize(
            self.emit_gc_string_slice(replacement, index, next_index, f),
            f,
        );
        unit.load(f);
        f.instruction(&Instruction::I32Const(36));
        f.instruction(&Instruction::I32Eq);
        next_index.load(f);
        replacement_length.load(f);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_gc_string_code_unit_i32(replacement, next_index, f);
        next_unit.store(f);
        for (code, kind) in [
            (36, LiteralSubstitution::Dollar),
            (38, LiteralSubstitution::Matched),
            (96, LiteralSubstitution::Prefix),
            (39, LiteralSubstitution::Suffix),
        ] {
            next_unit.load(f);
            f.instruction(&Instruction::I32Const(code));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I64Const(2));
            consumed.store(f);
            match kind {
                LiteralSubstitution::Dollar => {
                    piece.replace(self.emit_native_string_static("$", f), f)
                }
                LiteralSubstitution::Matched => piece.replace(matched.load(s, f), f),
                LiteralSubstitution::Prefix => {
                    piece.replace(self.emit_gc_string_slice(input, zero, position, f), f)
                }
                LiteralSubstitution::Suffix => {
                    piece.replace(self.emit_gc_string_slice(input, end, input_length, f), f)
                }
            }
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        next_unit.load(f);
        f.instruction(&Instruction::I32Const(48));
        f.instruction(&Instruction::I32GeU);
        next_unit.load(f);
        f.instruction(&Instruction::I32Const(57));
        f.instruction(&Instruction::I32LeU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        next_unit.load(f);
        f.instruction(&Instruction::I32Const(48));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I64ExtendI32U);
        capture.store(f);
        capture.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        capture.load(f);
        capture_count.load(f);
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(2));
        consumed.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        next_index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        after.store(f);
        after.load(f);
        replacement_length.load(f);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_gc_string_code_unit_i32(replacement, after, f);
        digit.store(f);
        digit.load(f);
        f.instruction(&Instruction::I32Const(48));
        f.instruction(&Instruction::I32GeU);
        digit.load(f);
        f.instruction(&Instruction::I32Const(57));
        f.instruction(&Instruction::I32LeU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        capture.load(f);
        f.instruction(&Instruction::I64Const(10));
        f.instruction(&Instruction::I64Mul);
        digit.load(f);
        f.instruction(&Instruction::I32Const(48));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Add);
        candidate.store(f);
        candidate.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        candidate.load(f);
        capture_count.load(f);
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        candidate.load(f);
        capture.store(f);
        f.instruction(&Instruction::I64Const(3));
        consumed.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        consumed.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        capture.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I32WrapI64);
        ordinal.store(f);
        self.emit_argument_vector_entry_to_value(captures.values(), ordinal, &value, f);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        piece.replace(self.emit_native_string_static("", f), f);
        f.instruction(&Instruction::Else);
        piece.replace(value.cast_reference::<StringValue>(s, f), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        next_unit.load(f);
        f.instruction(&Instruction::I32Const(60));
        f.instruction(&Instruction::I32Eq);
        named.0.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(2));
        f.instruction(&Instruction::I64Add);
        name_start.store(f);
        name_start.load(f);
        scan.store(f);
        f.instruction(&Instruction::I32Const(0));
        found.store(f);
        let close_done = self.open_frame(ControlFrameKind::Block, f);
        let close_next = self.open_frame(ControlFrameKind::Loop, f);
        scan.load(f);
        replacement_length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(close_done, f);
        self.emit_gc_string_code_unit_i32(replacement, scan, f);
        f.instruction(&Instruction::I32Const(62));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I32Const(1));
        found.store(f);
        self.emit_branch_to_target(close_done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        scan.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        scan.store(f);
        self.emit_branch_to_target(close_next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let name = s.reserve_gc_local(f).initialize(
            self.emit_gc_string_slice(replacement, name_start, scan, f),
            f,
        );
        let key = PropertyKeyLocals::from_string(s, &name, f);
        name.clear(f);
        self.emit_object_read(&named.0, &named.0, &key, &pending, f)?;
        key.clear(f);
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        value.copy_from(pending.value(), f);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        piece.replace(self.emit_native_string_static("", f), f);
        f.instruction(&Instruction::Else);
        self.emit_value_to_string_payload(&value, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        piece.replace(pending.value().cast_reference::<StringValue>(s, f), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        scan.load(f);
        index.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        consumed.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_native_gc_string_length(&piece, piece_length, f);
        total_length.load(f);
        piece_length.load(f);
        f.instruction(&Instruction::I64Add);
        total_length.store(f);
        total_length.load(f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        f.instruction(&Instruction::I64GtU);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::REPLACEMENT_RESULT_WOULD_EXCEED_MAXIMUM_STRING_LENGTH,
            NativeErrorKind::RangeError,
            output,
            exit,
            f,
        )?;
        pieces.append(&piece, s, f);
        piece.clear(f);
        index.load(f);
        consumed.load(f);
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let substituted = pieces.publish(self, total_length, f);
        accumulated.replace(substituted.load(s, f), f);
        substituted.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(has_dollar, f);
        pending.clear(f);
        value.clear(f);
        for local in [ordinal, found, digit, next_unit, unit] {
            s.release_i32_local(local, f);
        }
        for local in [
            total_length,
            piece_length,
            name_start,
            scan,
            candidate,
            capture,
            zero,
            end,
            consumed,
            after,
            next_index,
            index,
            capture_count,
            matched_length,
            input_length,
            replacement_length,
        ] {
            s.release_i64_local(local, f);
        }
        Ok(accumulated)
    }
}
