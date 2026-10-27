use super::super::*;
use crate::gc_types::{I32Local, I64Local};
use crate::runtime_helpers::RegExpMatcherParameters;
use crate::runtime_helpers::{
    RegExpMatcherFailure, RegExpMatcherStatus, REGEXP_MATCHER_SCRATCH_MAX_BYTES,
};
use scriptw_ir::REGEXP_NAMED_GROUP_TABLE_MAGIC_VERSION;
use scriptw_ir::{
    RegExpCaseFolding, RegExpModifierOverride, RegExpRepeatBoundWord, RegExpRepeatMaximumKind,
    RegExpRepeatStateWord, REGEXP_BACKREFERENCE_IGNORE_CASE, REGEXP_BACKREFERENCE_NONEMPTY,
    REGEXP_INSTRUCTION_WIDTH, REGEXP_OPCODE_ACCEPT, REGEXP_OPCODE_ASSERT_END,
    REGEXP_OPCODE_ASSERT_START, REGEXP_OPCODE_CAPTURE_END, REGEXP_OPCODE_CAPTURE_START,
    REGEXP_OPCODE_CLEAR_CAPTURE_RANGE, REGEXP_OPCODE_DOT, REGEXP_OPCODE_JUMP,
    REGEXP_OPCODE_LITERAL_ASCII, REGEXP_OPCODE_LITERAL_CODE_POINT, REGEXP_OPCODE_LOOKAROUND_END,
    REGEXP_OPCODE_LOOKAROUND_FAILURE, REGEXP_OPCODE_LOOKAROUND_START,
    REGEXP_OPCODE_NAMED_BACKREFERENCE, REGEXP_OPCODE_NEGATIVE_ASCII_CLASS,
    REGEXP_OPCODE_NOT_WHITESPACE, REGEXP_OPCODE_NUMBERED_BACKREFERENCE,
    REGEXP_OPCODE_POSITIVE_ASCII_CLASS, REGEXP_OPCODE_PROGRESS_CHECK, REGEXP_OPCODE_PROGRESS_SPLIT,
    REGEXP_OPCODE_REPEAT_BEGIN, REGEXP_OPCODE_REPEAT_END, REGEXP_OPCODE_REPEAT_EXIT,
    REGEXP_OPCODE_REPEAT_GUARD, REGEXP_OPCODE_SPLIT, REGEXP_OPCODE_UNICODE_PROPERTY,
    REGEXP_OPCODE_WHITESPACE, REGEXP_OPCODE_WORD_BOUNDARY, REGEXP_PROGRAM_HEADER_SIZE,
    REGEXP_RANGE_ENTRY_WIDTH, REGEXP_REPEAT_BOUND_RECORD_SIZE,
    REGEXP_REPEAT_COUNTER_DECIMAL_DIGITS, REGEXP_REPEAT_COUNTER_LIMB_WIDTH,
    REGEXP_REPEAT_COUNTER_RADIX, REGEXP_REPEAT_STATE_HEADER_SIZE,
};

mod backreference;
mod backtracking;
use backtracking::{BacktrackRequest, BacktrackRequestScope};
mod compiler;
mod counted;
mod matcher_workspace;
mod program;
mod transient;
use matcher_workspace::{ChoiceEntryKind, MatcherWorkspace, SnapshotChoice};
use transient::ScratchFailure;
mod range_search;
mod word_boundary;

use backreference::{RegExpCharacterLocals, RegExpInputCursor};
pub(in crate::builtins) use program::{
    RegExpProgramLayoutFailure, ValidatedRegExpProgramLayoutLocals,
};

enum RegExpMatcherResult {
    Match,
    NoMatch,
    Failed(RegExpMatcherFailure),
}

impl<'a> FunctionBuilder<'a> {
    /// Compiles the fixed-width ordered-backtracking `RegExpProgram` matcher.
    ///
    /// The flat matcher grammar uses ordered choices for repetition. Nullable
    /// attempts pair `ProgressSplit` with `ProgressCheck`, retaining their
    /// authority in the same capture-owning choice frames as ordinary splits.
    /// Matching reads immutable program/string roots. Its exclusive private
    /// workspace grows on demand, snapshots captures and repeat counters, and
    /// publishes captures into the caller's output before rewinding its lease.
    ///
    /// Named registered inputs keep immutable GC program/String roots; only
    /// the private backtracking work arena is a memory0 byte address.
    pub(crate) fn compile_regexp_matcher_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RegExpMatcher);
        let schema = self.runtime_schema();
        let parameters = self.helper_parameters::<RegExpMatcherParameters>(&mut function);
        let checkpoint = self.runtime_schema().reserve_i64_local(&mut function);
        let start = self.runtime_schema().reserve_i64_local(&mut function);
        let flags = self.runtime_schema().reserve_i64_local(&mut function);
        let scratch = self.runtime_schema().reserve_i64_local(&mut function);
        let output_scratch = self.runtime_schema().reserve_i64_local(&mut function);
        function.instruction(&Instruction::GlobalGet(PRIVATE_BYTE_CURSOR_GLOBAL_INDEX));
        checkpoint.store(&mut function);
        parameters.start.load(&mut function);
        start.store(&mut function);
        parameters.flags.load(&mut function);
        function.instruction(&Instruction::I64ExtendI32U);
        flags.store(&mut function);
        parameters.scratch.load(&mut function);
        output_scratch.store(&mut function);
        let named_candidates_end = self.runtime_schema().reserve_i64_local(&mut function);
        let named_next_candidate = self.runtime_schema().reserve_i64_local(&mut function);
        let named_next_name = self.runtime_schema().reserve_i64_local(&mut function);
        let input_offset = self.runtime_schema().reserve_i64_local(&mut function);
        let input_len = self.runtime_schema().reserve_i64_local(&mut function);
        let input_utf16_len = self.runtime_schema().reserve_i64_local(&mut function);
        let candidate_byte = self.runtime_schema().reserve_i64_local(&mut function);
        let candidate_utf16 = self.runtime_schema().reserve_i64_local(&mut function);
        let match_byte = self.runtime_schema().reserve_i64_local(&mut function);
        let match_utf16 = self.runtime_schema().reserve_i64_local(&mut function);
        let pc = self.runtime_schema().reserve_i64_local(&mut function);
        let instruction_address = self.runtime_schema().reserve_i64_local(&mut function);
        let opcode = self.runtime_schema().reserve_i64_local(&mut function);
        let operand0 = self.runtime_schema().reserve_i64_local(&mut function);
        let operand1 = self.runtime_schema().reserve_i64_local(&mut function);
        let byte = self.runtime_schema().reserve_i64_local(&mut function);
        let codepoint = self.runtime_schema().reserve_i64_local(&mut function);
        let byte_advance = self.runtime_schema().reserve_i64_local(&mut function);
        let utf16_advance = self.runtime_schema().reserve_i64_local(&mut function);
        let decode_temp = self.runtime_schema().reserve_i64_local(&mut function);
        let literal_value = self.runtime_schema().reserve_i64_local(&mut function);
        let literal_advance_byte = self.runtime_schema().reserve_i64_local(&mut function);
        let candidate_on_low_surrogate = self.runtime_schema().reserve_i64_local(&mut function);
        let match_on_low_surrogate = self.runtime_schema().reserve_i64_local(&mut function);
        let choice_header = self.runtime_schema().reserve_i64_local(&mut function);
        let progress_no_advance = self.runtime_schema().reserve_i64_local(&mut function);
        let choice_lazy = self.runtime_schema().reserve_i64_local(&mut function);
        let sticky = self.runtime_schema().reserve_i64_local(&mut function);
        let unicode = self.runtime_schema().reserve_i64_local(&mut function);
        let multiline = self.runtime_schema().reserve_i64_local(&mut function);
        let dot_all = self.runtime_schema().reserve_i64_local(&mut function);
        let capture_index = self.runtime_schema().reserve_i64_local(&mut function);
        let capture_address = self.runtime_schema().reserve_i64_local(&mut function);
        let capture_start = self.runtime_schema().reserve_i64_local(&mut function);
        let named_group_count = self.runtime_schema().reserve_i64_local(&mut function);
        let named_candidate_total = self.runtime_schema().reserve_i64_local(&mut function);
        let named_records_ptr = self.runtime_schema().reserve_i64_local(&mut function);
        let named_record_ptr = self.runtime_schema().reserve_i64_local(&mut function);
        let named_candidate_ptr = self.runtime_schema().reserve_i64_local(&mut function);
        let named_candidate_count = self.runtime_schema().reserve_i64_local(&mut function);
        let named_candidate_id = self.runtime_schema().reserve_i64_local(&mut function);
        let named_selected_count = self.runtime_schema().reserve_i64_local(&mut function);
        let named_candidate_start = self.runtime_schema().reserve_i64_local(&mut function);
        let named_capture_end = self.runtime_schema().reserve_i64_local(&mut function);
        let capture_byte = self.runtime_schema().reserve_i64_local(&mut function);
        let capture_utf16 = self.runtime_schema().reserve_i64_local(&mut function);
        let capture_on_low_surrogate = self.runtime_schema().reserve_i64_local(&mut function);
        let compare_byte = self.runtime_schema().reserve_i64_local(&mut function);
        let compare_utf16 = self.runtime_schema().reserve_i64_local(&mut function);
        let compare_on_low_surrogate = self.runtime_schema().reserve_i64_local(&mut function);
        let capture_unit = self.runtime_schema().reserve_i64_local(&mut function);
        let compare_unit = self.runtime_schema().reserve_i64_local(&mut function);
        let backreference_seek_utf16 = self.runtime_schema().reserve_i64_local(&mut function);
        let backreference_limit_utf16 = self.runtime_schema().reserve_i64_local(&mut function);
        let case_folding_table = self.runtime_schema().reserve_i64_local(&mut function);
        let case_folding_count = self.runtime_schema().reserve_i64_local(&mut function);
        let reverse_mode = self.runtime_schema().reserve_i64_local(&mut function);
        let previous_byte = self.runtime_schema().reserve_i64_local(&mut function);
        let range_low = self.runtime_schema().reserve_i64_local(&mut function);
        let range_high = self.runtime_schema().reserve_i64_local(&mut function);
        let range_middle = self.runtime_schema().reserve_i64_local(&mut function);
        let range_count = self.runtime_schema().reserve_i64_local(&mut function);
        let class_character = self.runtime_schema().reserve_i64_local(&mut function);
        let effective_multiline = self.runtime_schema().reserve_i64_local(&mut function);
        let effective_dot_all = self.runtime_schema().reserve_i64_local(&mut function);
        let capture_cursor = RegExpInputCursor {
            byte: capture_byte,
            utf16: capture_utf16,
            on_low_surrogate: capture_on_low_surrogate,
        };
        let compare_cursor = RegExpInputCursor {
            byte: compare_byte,
            utf16: compare_utf16,
            on_low_surrogate: compare_on_low_surrogate,
        };
        let character_locals = RegExpCharacterLocals {
            byte,
            codepoint,
            byte_advance,
            utf16_advance,
            decode_temp,
            previous_byte,
        };

        let pending_layout = self.reserve_regexp_program_layout(&mut function);
        let layout = self.emit_validate_regexp_program_layout(
            &parameters.program,
            pending_layout,
            RegExpProgramLayoutFailure::Matcher,
            &mut function,
        )?;
        let program_ptr = self.runtime_schema().reserve_i64_local(&mut function);
        let named_group_table_ptr = self.runtime_schema().reserve_i64_local(&mut function);
        let instruction_count = self.runtime_schema().reserve_i64_local(&mut function);
        let capture_count = self.runtime_schema().reserve_i64_local(&mut function);
        let range_base = self.runtime_schema().reserve_i64_local(&mut function);
        let program_range_count = self.runtime_schema().reserve_i64_local(&mut function);
        let program_end = self.runtime_schema().reserve_i64_local(&mut function);
        let repeat_slot_count = self.runtime_schema().reserve_i64_local(&mut function);
        let repeat_state_bytes = self.runtime_schema().reserve_i64_local(&mut function);
        let repeat_bound_base = self.runtime_schema().reserve_i64_local(&mut function);
        for (source, output) in [
            (layout.instruction_count(), instruction_count),
            (layout.capture_count(), capture_count),
            (layout.range_count(), program_range_count),
            (layout.repeat_slot_count(), repeat_slot_count),
            (layout.repeat_state_byte_length(), repeat_state_bytes),
        ] {
            source.load(&mut function);
            output.store(&mut function);
        }
        let allocation = schema.reserve_i64_local(&mut function);
        self.emit_regexp_transient_allocation(
            layout.end(),
            allocation,
            &ScratchFailure::Matcher { checkpoint, start },
            &mut function,
        )?;
        for (source, output) in [
            (layout.instructions(), program_ptr),
            (layout.ranges(), range_base),
            (layout.end(), program_end),
            (layout.repeat_bounds(), repeat_bound_base),
        ] {
            allocation.load(&mut function);
            source.load(&mut function);
            function.instruction(&Instruction::I64Add);
            output.store(&mut function);
        }
        layout.named_groups().load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::I64Const(0));
        named_group_table_ptr.store(&mut function);
        function.instruction(&Instruction::Else);
        allocation.load(&mut function);
        layout.named_groups().load(&mut function);
        function.instruction(&Instruction::I64Add);
        named_group_table_ptr.store(&mut function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let cursor = schema.reserve_i64_local(&mut function);
        let copied_byte = schema.reserve_i64_local(&mut function);
        function.instruction(&Instruction::I64Const(0));
        cursor.store(&mut function);
        let copied = self.open_frame(ControlFrameKind::Block, &mut function);
        let copying = self.open_frame(ControlFrameKind::Loop, &mut function);
        cursor.load(&mut function);
        layout.end().load(&mut function);
        function.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(copied, &mut function);
        layout.read_byte(
            cursor,
            copied_byte,
            self,
            RegExpProgramLayoutFailure::Matcher,
            &mut function,
        )?;
        allocation.load(&mut function);
        cursor.load(&mut function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        copied_byte.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Store8(Self::memarg8(0)));
        cursor.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        cursor.store(&mut function);
        self.emit_branch_to_target(copying, &mut function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i64_local(copied_byte, &mut function);
        schema.release_i64_local(cursor, &mut function);
        schema.release_i64_local(allocation, &mut function);
        self.release_regexp_program_layout(layout, &mut function);

        flags.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        sticky.store(&mut function);
        flags.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        unicode.store(&mut function);
        unicode.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        for (index, folding) in [RegExpCaseFolding::Unicode, RegExpCaseFolding::Legacy]
            .into_iter()
            .enumerate()
        {
            if index != 0 {
                function.instruction(&Instruction::Else);
            }
            let table = self.strings.regexp_case_folding_table(folding);
            function.instruction(&Instruction::I64Const(
                table.map_or(0, |table| table.ptr as i64),
            ));
            case_folding_table.store(&mut function);
            function.instruction(&Instruction::I64Const(
                table.map_or(0, |table| table.count as i64),
            ));
            case_folding_count.store(&mut function);
        }
        function.instruction(&Instruction::End);
        flags.load(&mut function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        multiline.store(&mut function);
        flags.load(&mut function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        dot_all.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        reverse_mode.store(&mut function);
        // Named records, candidate IDs and name bytes own disjoint canonical
        // sections inside this descriptor, including names never backreferenced.
        named_group_table_ptr.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Else);
        for (offset, local) in [
            (0, decode_temp),
            (8, named_group_count),
            (16, named_candidate_total),
            (24, named_records_ptr),
        ] {
            self.emit_regexp_scratch_load_word(named_group_table_ptr, offset, local, &mut function);
        }
        decode_temp.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_NAMED_GROUP_TABLE_MAGIC_VERSION as i64,
        ));
        function.instruction(&Instruction::I64Ne);
        named_group_count.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Or);
        named_group_count.load(&mut function);
        capture_count.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        named_candidate_total.load(&mut function);
        capture_count.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        named_records_ptr.load(&mut function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            start,
            start,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        named_group_table_ptr.load(&mut function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Add);
        named_records_ptr.store(&mut function);
        named_records_ptr.load(&mut function);
        named_group_count.load(&mut function);
        function.instruction(&Instruction::I64Const(24));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        named_next_candidate.store(&mut function);
        named_next_candidate.load(&mut function);
        named_candidate_total.load(&mut function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        named_candidates_end.store(&mut function);
        named_candidates_end.load(&mut function);
        named_next_name.store(&mut function);
        named_next_name.load(&mut function);
        program_end.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            start,
            start,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(0));
        capture_index.store(&mut function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        capture_index.load(&mut function);
        named_group_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        named_records_ptr.load(&mut function);
        capture_index.load(&mut function);
        function.instruction(&Instruction::I64Const(24));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        named_record_ptr.store(&mut function);
        self.emit_regexp_scratch_load_word(named_record_ptr, 0, decode_temp, &mut function);
        decode_temp.load(&mut function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        byte_advance.store(&mut function);
        decode_temp.load(&mut function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        named_group_table_ptr.load(&mut function);
        function.instruction(&Instruction::I64Add);
        literal_value.store(&mut function);
        byte_advance.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        literal_value.load(&mut function);
        named_next_name.load(&mut function);
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32Or);
        literal_value.load(&mut function);
        program_end.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        byte_advance.load(&mut function);
        program_end.load(&mut function);
        literal_value.load(&mut function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            start,
            start,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        literal_value.load(&mut function);
        byte_advance.load(&mut function);
        function.instruction(&Instruction::I64Add);
        named_next_name.store(&mut function);
        self.emit_regexp_scratch_load_word(named_record_ptr, 8, named_candidate_ptr, &mut function);
        self.emit_regexp_scratch_load_word(
            named_record_ptr,
            16,
            named_candidate_count,
            &mut function,
        );
        named_candidate_ptr.load(&mut function);
        named_next_candidate.load(&mut function);
        named_group_table_ptr.load(&mut function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Ne);
        named_candidate_count.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Or);
        named_candidate_count.load(&mut function);
        named_candidates_end.load(&mut function);
        named_next_candidate.load(&mut function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            start,
            start,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        named_next_candidate.load(&mut function);
        named_candidate_ptr.store(&mut function);
        named_next_candidate.load(&mut function);
        named_candidate_count.load(&mut function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        named_next_candidate.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        named_candidate_id.store(&mut function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        named_candidate_id.load(&mut function);
        named_candidate_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        named_candidate_ptr.load(&mut function);
        named_candidate_id.load(&mut function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(0)));
        decode_temp.store(&mut function);
        decode_temp.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        decode_temp.load(&mut function);
        capture_count.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            start,
            start,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        named_candidate_id.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        named_candidate_id.store(&mut function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        capture_index.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        capture_index.store(&mut function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        named_next_candidate.load(&mut function);
        named_candidates_end.load(&mut function);
        function.instruction(&Instruction::I64Ne);
        named_next_name.load(&mut function);
        program_end.load(&mut function);
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            start,
            start,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        self.emit_regexp_transient_text(
            &parameters.input,
            input_offset,
            input_len,
            ScratchFailure::Matcher { checkpoint, start },
            &mut function,
        )?;
        // Counted-repeat proofs need the full UTF-16 extent even when the
        // pattern has no captures. The immutable String owns that exact length.
        self.emit_native_gc_string_length(&parameters.input, input_utf16_len, &mut function);
        // All immutable program/text transients are complete. Only this owner
        // may extend the tail until the match checkpoint is rewound.
        let workspace = MatcherWorkspace::allocate(
            self,
            capture_count,
            repeat_state_bytes,
            checkpoint,
            start,
            &mut function,
        )?;
        workspace.base.load(&mut function);
        scratch.store(&mut function);
        for local in [candidate_byte, candidate_utf16, candidate_on_low_surrogate] {
            function.instruction(&Instruction::I64Const(0));
            local.store(&mut function);
        }

        // Seek once from the beginning. A Unicode-mode start inside an astral
        // scalar normalizes to its leading code unit; non-Unicode matching
        // preserves the requested low-surrogate code-unit position.
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        candidate_byte.load(&mut function);
        input_len.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        candidate_utf16.load(&mut function);
        start.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_regexp_scratch_byte(input_offset, candidate_byte, byte, &mut function);
        self.emit_regexp_scratch_decode_scalar(
            input_offset,
            candidate_byte,
            input_len,
            byte,
            codepoint,
            byte_advance,
            decode_temp,
            &mut function,
        );
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        utf16_advance.store(&mut function);
        candidate_utf16.load(&mut function);
        utf16_advance.load(&mut function);
        function.instruction(&Instruction::I64Add);
        start.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I64ExtendI32U);
        candidate_on_low_surrogate.store(&mut function);
        candidate_on_low_surrogate.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        unicode.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        start.load(&mut function);
        candidate_utf16.store(&mut function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        candidate_on_low_surrogate.store(&mut function);
        function.instruction(&Instruction::End);
        // The byte cursor remains at the containing scalar in either mode.
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment_by_local(candidate_byte, byte_advance, &mut function);
        self.emit_regexp_scratch_increment_by_local(candidate_utf16, utf16_advance, &mut function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        // Preserve a non-Unicode low-surrogate start as an empty-match
        // candidate, retaining its containing scalar's byte cursor.
        candidate_on_low_surrogate.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        start.load(&mut function);
        candidate_utf16.store(&mut function);
        function.instruction(&Instruction::End);

        // A start beyond the string cannot produce a match, including an empty
        // one. A Unicode-normalized low-surrogate start is deliberately below
        // the raw start index while its byte cursor still points into input.
        candidate_utf16.load(&mut function);
        start.load(&mut function);
        function.instruction(&Instruction::I64LtU);
        candidate_byte.load(&mut function);
        input_len.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);

        // Candidate positions and atom cursors advance incrementally; byte offsets
        // are never exposed as RegExp indices.
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        candidate_byte.load(&mut function);
        match_byte.store(&mut function);
        candidate_utf16.load(&mut function);
        match_utf16.store(&mut function);
        candidate_on_low_surrogate.load(&mut function);
        match_on_low_surrogate.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        pc.store(&mut function);
        workspace.choices().reset(&mut function);

        workspace.reset_repeats(&mut function);
        // Each candidate owns a fresh capture vector.  -1/-1 is the unmatched
        // sentinel and is deliberately copied into every saved choice frame.
        function.instruction(&Instruction::I64Const(0));
        capture_index.store(&mut function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        capture_index.load(&mut function);
        capture_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        scratch.load(&mut function);
        capture_index.load(&mut function);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        capture_address.store(&mut function);
        for offset in [0u64, 8] {
            capture_address.load(&mut function);
            function.instruction(&Instruction::I32WrapI64);
            function.instruction(&Instruction::I64Const(-1));
            function.instruction(&Instruction::I64Store(Self::memarg8(offset)));
        }
        capture_index.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        capture_index.store(&mut function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        let backtrack = BacktrackRequestScope::open(self, &mut function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        pc.load(&mut function);
        instruction_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        /* Capture instructions are dispatched below, after their three words
         * have been loaded. */
        program_ptr.load(&mut function);
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_INSTRUCTION_WIDTH as i64));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        instruction_address.store(&mut function);
        self.emit_regexp_instruction_load(instruction_address, 0, opcode, &mut function);
        self.emit_regexp_instruction_load(instruction_address, 8, operand0, &mut function);
        self.emit_regexp_instruction_load(instruction_address, 16, operand1, &mut function);
        self.emit_regexp_counted_dispatch(
            &workspace,
            backtrack.request(),
            &counted::CountedMatcherLocals {
                program: program_ptr,
                instructions: instruction_count,
                slots: repeat_slot_count,
                bounds: repeat_bound_base,
                opcode,
                operand0,
                pc,
                cursor: RegExpInputCursor {
                    byte: match_byte,
                    utf16: match_utf16,
                    on_low_surrogate: match_on_low_surrogate,
                },
                reverse_mode,
                choice_header,
                input_utf16_len,
                named_group_table_ptr,
                named_records_ptr,
                named_group_count,
            },
            &mut function,
        )?;
        // `.`, `^` and `$` carry a RegExp-modifier override in `operand0`: 0
        // defers to the pattern flag, 1 forces the mode on and 2 forces it off.
        for (source, effective) in [
            (multiline, effective_multiline),
            (dot_all, effective_dot_all),
        ] {
            operand0.load(&mut function);
            function.instruction(&Instruction::I64Const(
                RegExpModifierOverride::ForceOn.operand_code() as i64,
            ));
            function.instruction(&Instruction::I64Eq);
            function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::Else);
            operand0.load(&mut function);
            function.instruction(&Instruction::I64Const(
                RegExpModifierOverride::ForceOff.operand_code() as i64,
            ));
            function.instruction(&Instruction::I64Eq);
            function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
            function.instruction(&Instruction::I64Const(0));
            function.instruction(&Instruction::Else);
            source.load(&mut function);
            function.instruction(&Instruction::End);
            function.instruction(&Instruction::End);
            effective.store(&mut function);
        }

        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_LOOKAROUND_START as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64GtU);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        operand0.load(&mut function);
        reverse_mode.store(&mut function);
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);

        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_LOOKAROUND_END as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        instruction_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(0x3fff_ffff_ffff_ffff));
        function.instruction(&Instruction::I64And);
        instruction_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);

        workspace.choices().find_assertion(
            self,
            operand0,
            &mut function,
            |entry, builder, function| {
                entry.restore_cursor(
                    RegExpInputCursor {
                        byte: match_byte,
                        utf16: match_utf16,
                        on_low_surrogate: match_on_low_surrogate,
                    },
                    function,
                );
                operand1.load(function);
                function.instruction(&Instruction::I64Const(63));
                function.instruction(&Instruction::I64ShrU);
                function.instruction(&Instruction::I32WrapI64);
                function.instruction(&Instruction::If(BlockType::Empty));
                entry.restore_captures(function);
                function.instruction(&Instruction::End);
                entry.restore_repeats(builder, function);
                entry.discard_through(builder, function);
                Ok(())
            },
        )?;
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(62));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        reverse_mode.store(&mut function);
        // A completed negative body rejects the assertion immediately. Its
        // private alternatives cannot be revisited by an outer continuation.
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(63));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I32WrapI64);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(0x3fff_ffff_ffff_ffff));
        function.instruction(&Instruction::I64And);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);

        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_LOOKAROUND_FAILURE as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        instruction_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        reverse_mode.store(&mut function);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Eqz);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        operand0.load(&mut function);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);

        // Capture IDs are one based.  Starts and ends never consume input.
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_CAPTURE_START as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        operand0.load(&mut function);
        capture_count.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        reverse_mode.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        scratch.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        match_utf16.load(&mut function);
        function.instruction(&Instruction::I64Store(Self::memarg8(0)));
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);
        scratch.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        match_utf16.load(&mut function);
        function.instruction(&Instruction::I64Store(Self::memarg8(0)));
        scratch.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Store(Self::memarg8(8)));
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        // ClearCaptureRange is non-consuming and clears the canonical
        // half-open one-based range [operand0, operand1).
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_CLEAR_CAPTURE_RANGE as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64LtU);
        operand0.load(&mut function);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I32Or);
        operand1.load(&mut function);
        capture_count.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        operand0.load(&mut function);
        capture_index.store(&mut function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        capture_index.load(&mut function);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        scratch.load(&mut function);
        capture_index.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        capture_address.store(&mut function);
        for offset in [0u64, 8] {
            capture_address.load(&mut function);
            function.instruction(&Instruction::I32WrapI64);
            function.instruction(&Instruction::I64Const(-1));
            function.instruction(&Instruction::I64Store(Self::memarg8(offset)));
        }
        capture_index.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        capture_index.store(&mut function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        // NamedBackreference selects the single participating capture for a
        // name. Multiple participating candidates are an invalid program
        // state; no participating candidate is the specified empty match.
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_NAMED_BACKREFERENCE as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_NUMBERED_BACKREFERENCE as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(
            !(REGEXP_BACKREFERENCE_NONEMPTY | REGEXP_BACKREFERENCE_IGNORE_CASE) as i64,
        ));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_BACKREFERENCE_IGNORE_CASE as i64,
        ));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        case_folding_table.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_NUMBERED_BACKREFERENCE as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64LtU);
        operand0.load(&mut function);
        capture_count.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        scratch.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        capture_address.store(&mut function);
        capture_address.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(0)));
        capture_start.store(&mut function);
        capture_start.load(&mut function);
        named_candidate_start.store(&mut function);
        capture_address.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(8)));
        named_capture_end.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        named_selected_count.store(&mut function);
        capture_start.load(&mut function);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Ne);
        named_capture_end.load(&mut function);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        named_selected_count.store(&mut function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_BACKREFERENCE_NONEMPTY as i64));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        named_group_table_ptr.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Or);
        operand0.load(&mut function);
        named_group_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        named_records_ptr.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(24));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        named_record_ptr.store(&mut function);
        named_record_ptr.load(&mut function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(0)));
        named_group_table_ptr.load(&mut function);
        function.instruction(&Instruction::I64Add);
        named_candidate_ptr.store(&mut function);
        named_record_ptr.load(&mut function);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(0)));
        named_candidate_count.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        named_selected_count.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        named_candidate_id.store(&mut function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        named_candidate_id.load(&mut function);
        named_candidate_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        named_candidate_ptr.load(&mut function);
        named_candidate_id.load(&mut function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(0)));
        decode_temp.store(&mut function);
        scratch.load(&mut function);
        decode_temp.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        capture_address.store(&mut function);
        capture_address.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(0)));
        named_candidate_start.store(&mut function);
        named_candidate_start.load(&mut function);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        capture_address.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(8)));
        named_capture_end.store(&mut function);
        named_capture_end.load(&mut function);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        named_candidate_start.load(&mut function);
        capture_start.store(&mut function);
        capture_start.load(&mut function);
        named_capture_end.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        named_capture_end.load(&mut function);
        input_utf16_len.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        named_selected_count.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        named_selected_count.store(&mut function);
        named_selected_count.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        named_candidate_id.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        named_candidate_id.store(&mut function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        named_selected_count.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);
        capture_start.load(&mut function);
        named_capture_end.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        named_capture_end.load(&mut function);
        input_utf16_len.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        // Traverse the capture and candidate in the active direction. Capture
        // boundaries are UTF-16 indices even when comparison reads full scalars.
        reverse_mode.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        named_capture_end.load(&mut function);
        backreference_seek_utf16.store(&mut function);
        capture_start.load(&mut function);
        backreference_limit_utf16.store(&mut function);
        function.instruction(&Instruction::Else);
        capture_start.load(&mut function);
        backreference_seek_utf16.store(&mut function);
        named_capture_end.load(&mut function);
        backreference_limit_utf16.store(&mut function);
        function.instruction(&Instruction::End);
        // Unit-wise seeking retains legacy captures at either astral half.
        for local in [capture_byte, capture_utf16, capture_on_low_surrogate] {
            function.instruction(&Instruction::I64Const(0));
            local.store(&mut function);
        }
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        capture_utf16.load(&mut function);
        backreference_seek_utf16.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        capture_byte.load(&mut function);
        input_len.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        self.emit_regexp_read_utf16_unit(
            input_offset,
            input_len,
            capture_byte,
            capture_utf16,
            capture_on_low_surrogate,
            capture_unit,
            byte,
            codepoint,
            byte_advance,
            decode_temp,
            &mut function,
        );
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        for (source, destination) in [
            (match_byte, compare_byte),
            (match_utf16, compare_utf16),
            (match_on_low_surrogate, compare_on_low_surrogate),
        ] {
            source.load(&mut function);
            destination.store(&mut function);
        }
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        capture_utf16.load(&mut function);
        backreference_limit_utf16.load(&mut function);
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::BrIf(1));
        // Unicode comparisons may not start between paired surrogate units.
        unicode.load(&mut function);
        capture_on_low_surrogate.load(&mut function);
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        reverse_mode.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        compare_utf16.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::Else);
        compare_byte.load(&mut function);
        input_len.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::End);
        unicode.load(&mut function);
        compare_on_low_surrogate.load(&mut function);
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Or);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        self.emit_regexp_read_character(
            checkpoint,
            input_offset,
            input_len,
            capture_cursor,
            reverse_mode,
            unicode,
            capture_unit,
            character_locals,
            candidate_utf16,
            &mut function,
        );
        self.emit_regexp_read_character(
            checkpoint,
            input_offset,
            input_len,
            compare_cursor,
            reverse_mode,
            unicode,
            compare_unit,
            character_locals,
            candidate_utf16,
            &mut function,
        );
        capture_utf16.load(&mut function);
        capture_start.load(&mut function);
        function.instruction(&Instruction::I64LtU);
        capture_utf16.load(&mut function);
        named_capture_end.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_BACKREFERENCE_IGNORE_CASE as i64,
        ));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        for character in [capture_unit, compare_unit] {
            self.emit_regexp_canonicalize_character(
                case_folding_table,
                case_folding_count,
                character,
                range_low,
                range_high,
                range_middle,
                &mut function,
            );
        }
        function.instruction(&Instruction::End);
        capture_unit.load(&mut function);
        compare_unit.load(&mut function);
        function.instruction(&Instruction::I64Ne);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        // Only a complete comparison commits the authoritative match cursor.
        for (source, destination) in [
            (compare_byte, match_byte),
            (compare_utf16, match_utf16),
            (compare_on_low_surrogate, match_on_low_surrogate),
        ] {
            source.load(&mut function);
            destination.store(&mut function);
        }
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_WORD_BOUNDARY as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_word_boundary_mismatch(
            checkpoint,
            input_offset,
            input_len,
            match_byte,
            match_on_low_surrogate,
            unicode,
            range_base,
            program_range_count,
            operand0,
            operand1,
            candidate_utf16,
            &mut function,
        );
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);

        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_ASSERT_START as i64));
        function.instruction(&Instruction::I64Eq);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_ASSERT_END as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64GtU);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(0));
        decode_temp.store(&mut function);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_ASSERT_START as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        match_utf16.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        decode_temp.store(&mut function);
        function.instruction(&Instruction::Else);
        effective_multiline.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        match_byte.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        // Between UTF-16 surrogates, the byte cursor still points at the scalar's start.
        match_on_low_surrogate.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        input_offset.load(&mut function);
        match_byte.load(&mut function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load8U(Self::memarg8(0)));
        function.instruction(&Instruction::I64ExtendI32U);
        byte.store(&mut function);
        byte.load(&mut function);
        function.instruction(&Instruction::I64Const(0x0A));
        function.instruction(&Instruction::I64Eq);
        byte.load(&mut function);
        function.instruction(&Instruction::I64Const(0x0D));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        decode_temp.store(&mut function);
        function.instruction(&Instruction::Else);
        match_byte.load(&mut function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64GeU);
        byte.load(&mut function);
        function.instruction(&Instruction::I64Const(0xA8));
        function.instruction(&Instruction::I64Eq);
        byte.load(&mut function);
        function.instruction(&Instruction::I64Const(0xA9));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        input_offset.load(&mut function);
        match_byte.load(&mut function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load8U(Self::memarg8(0)));
        function.instruction(&Instruction::I32Const(0x80));
        function.instruction(&Instruction::I32Eq);
        input_offset.load(&mut function);
        match_byte.load(&mut function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load8U(Self::memarg8(0)));
        function.instruction(&Instruction::I32Const(0xE2));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I64ExtendI32U);
        decode_temp.store(&mut function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        match_byte.load(&mut function);
        input_len.load(&mut function);
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        decode_temp.store(&mut function);
        function.instruction(&Instruction::Else);
        effective_multiline.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte(input_offset, match_byte, byte, &mut function);
        byte.load(&mut function);
        function.instruction(&Instruction::I64Const(0x0A));
        function.instruction(&Instruction::I64Eq);
        byte.load(&mut function);
        function.instruction(&Instruction::I64Const(0x0D));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        decode_temp.store(&mut function);
        function.instruction(&Instruction::Else);
        byte.load(&mut function);
        function.instruction(&Instruction::I64Const(0xE2));
        function.instruction(&Instruction::I64Eq);
        match_byte.load(&mut function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Add);
        input_len.load(&mut function);
        function.instruction(&Instruction::I64LeU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        input_offset.load(&mut function);
        match_byte.load(&mut function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load8U(Self::memarg8(0)));
        function.instruction(&Instruction::I32Const(0x80));
        function.instruction(&Instruction::I32Eq);
        input_offset.load(&mut function);
        match_byte.load(&mut function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load8U(Self::memarg8(0)));
        function.instruction(&Instruction::I32Const(0xA8));
        function.instruction(&Instruction::I32Eq);
        input_offset.load(&mut function);
        match_byte.load(&mut function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load8U(Self::memarg8(0)));
        function.instruction(&Instruction::I32Const(0xA9));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I64ExtendI32U);
        decode_temp.store(&mut function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        decode_temp.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_CAPTURE_END as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        operand0.load(&mut function);
        capture_count.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        reverse_mode.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        scratch.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        match_utf16.load(&mut function);
        function.instruction(&Instruction::I64Store(Self::memarg8(8)));
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);
        scratch.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        capture_address.store(&mut function);
        capture_address.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(0)));
        capture_start.store(&mut function);
        capture_start.load(&mut function);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        capture_address.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        match_utf16.load(&mut function);
        function.instruction(&Instruction::I64Store(Self::memarg8(8)));
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_ACCEPT as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        // Positive lookahead can capture beyond the consumed match. Capture
        // boundaries are bounded by the whole input, not the match end.
        function.instruction(&Instruction::I64Const(0));
        capture_index.store(&mut function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        capture_index.load(&mut function);
        capture_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        scratch.load(&mut function);
        capture_index.load(&mut function);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        capture_address.store(&mut function);
        capture_address.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(0)));
        capture_start.store(&mut function);
        capture_address.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(8)));
        operand0.store(&mut function);
        capture_start.load(&mut function);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::Else);
        capture_start.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        operand0.load(&mut function);
        input_utf16_len.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        capture_index.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        capture_index.store(&mut function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        workspace.publish_captures(output_scratch, &mut function);
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            match_utf16,
            RegExpMatcherResult::Match,
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        // `Split` records the fallback before taking the primary arm. Both
        // target operands are absolute instruction indices and must be within
        // this untrusted program span.
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_SPLIT as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        instruction_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        operand1.load(&mut function);
        instruction_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);

        workspace.choices().push_snapshot(
            self,
            SnapshotChoice::Ordinary {
                fallback: operand1,
                origin: pc,
            },
            RegExpInputCursor {
                byte: match_byte,
                utf16: match_utf16,
                on_low_surrogate: match_on_low_surrogate,
            },
            &mut function,
        )?;
        operand0.load(&mut function);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);

        // Nullable attempts retain their cursor, complete slab, kind and
        // actual source identity in the same linked ordered-choice arena.
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_PROGRESS_SPLIT as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        instruction_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        instruction_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I32Or);
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(0x3fff_ffff));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);

        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        choice_lazy.store(&mut function);
        choice_lazy.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        operand0.load(&mut function);
        function.instruction(&Instruction::Else);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::End);
        choice_header.store(&mut function);
        workspace.choices().push_snapshot(
            self,
            SnapshotChoice::Progress {
                fallback: choice_header,
                origin: pc,
                lazy: choice_lazy,
            },
            RegExpInputCursor {
                byte: match_byte,
                utf16: match_utf16,
                on_low_surrogate: match_on_low_surrogate,
            },
            &mut function,
        )?;

        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::Else);
        operand0.load(&mut function);
        function.instruction(&Instruction::End);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);

        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_JUMP as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        instruction_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        operand0.load(&mut function);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);

        // The check does not assume its progress frame is on top: ordered
        // alternatives inside the atom remain newer choices and must run
        // first if the current attempt made no progress.
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_PROGRESS_CHECK as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        instruction_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        operand1.load(&mut function);
        instruction_count.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);

        workspace.choices().find_progress(
            self,
            operand0,
            &mut function,
            |entry, _, function| {
                entry.load_utf16(function);
                match_utf16.load(function);
                function.instruction(&Instruction::I64Eq);
                function.instruction(&Instruction::I64ExtendI32U);
                progress_no_advance.store(function);
                Ok(())
            },
        )?;
        progress_no_advance.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        operand1.load(&mut function);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);

        reverse_mode.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_LITERAL_ASCII as i64));
        function.instruction(&Instruction::I64Eq);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_LITERAL_CODE_POINT as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_UNICODE_PROPERTY as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_WHITESPACE as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_NOT_WHITESPACE as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_DOT as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_POSITIVE_ASCII_CLASS as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_NEGATIVE_ASCII_CLASS as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        match_utf16.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);

        match_on_low_surrogate.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte(input_offset, match_byte, byte, &mut function);
        self.emit_regexp_scratch_decode_scalar(
            input_offset,
            match_byte,
            input_len,
            byte,
            codepoint,
            byte_advance,
            decode_temp,
            &mut function,
        );
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(0xd800));
        function.instruction(&Instruction::I64Add);
        codepoint.store(&mut function);
        function.instruction(&Instruction::I64Const(1));
        utf16_advance.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        match_on_low_surrogate.store(&mut function);
        function.instruction(&Instruction::Else);
        match_byte.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        previous_byte.store(&mut function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        input_offset.load(&mut function);
        previous_byte.load(&mut function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load8U(Self::memarg8(0)));
        function.instruction(&Instruction::I32Const(0xc0));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Const(0x80));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::BrIf(1));
        previous_byte.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        previous_byte.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        previous_byte.store(&mut function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_byte(input_offset, previous_byte, byte, &mut function);
        self.emit_regexp_scratch_decode_scalar(
            input_offset,
            previous_byte,
            input_len,
            byte,
            codepoint,
            byte_advance,
            decode_temp,
            &mut function,
        );
        previous_byte.load(&mut function);
        match_byte.store(&mut function);
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64GeU);
        unicode.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(0x3ff));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(0xdc00));
        function.instruction(&Instruction::I64Add);
        codepoint.store(&mut function);
        function.instruction(&Instruction::I64Const(1));
        utf16_advance.store(&mut function);
        function.instruction(&Instruction::I64Const(1));
        match_on_low_surrogate.store(&mut function);
        function.instruction(&Instruction::Else);
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        utf16_advance.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        match_on_low_surrogate.store(&mut function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_LITERAL_ASCII as i64));
        function.instruction(&Instruction::I64Eq);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_LITERAL_CODE_POINT as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Ne);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        match_utf16.load(&mut function);
        utf16_advance.load(&mut function);
        function.instruction(&Instruction::I64Sub);
        match_utf16.store(&mut function);
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);

        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_UNICODE_PROPERTY as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        // Reverse matching uses the same canonical range slice and membership
        // test as forward matching. Only cursor movement differs.
        operand0.load(&mut function);
        program_range_count.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        program_range_count.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        self.emit_regexp_unicode_property_mismatch(
            range_base,
            operand0,
            operand1,
            codepoint,
            range_count,
            range_low,
            range_high,
            range_middle,
            &mut function,
        );
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        match_utf16.load(&mut function);
        utf16_advance.load(&mut function);
        function.instruction(&Instruction::I64Sub);
        match_utf16.store(&mut function);
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);

        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_WHITESPACE as i64));
        function.instruction(&Instruction::I64Eq);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_NOT_WHITESPACE as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_whitespace_mismatch(opcode, codepoint, &mut function);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        match_utf16.load(&mut function);
        utf16_advance.load(&mut function);
        function.instruction(&Instruction::I64Sub);
        match_utf16.store(&mut function);
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);

        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_DOT as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        for line_terminator in [0x000A_i64, 0x000D, 0x2028, 0x2029] {
            codepoint.load(&mut function);
            function.instruction(&Instruction::I64Const(line_terminator));
            function.instruction(&Instruction::I64Eq);
        }
        for _ in 1..4 {
            function.instruction(&Instruction::I32Or);
        }
        effective_dot_all.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32And);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        function.instruction(&Instruction::Else);
        self.emit_regexp_ascii_class_contains(codepoint, operand0, operand1, &mut function);
        function.instruction(&Instruction::I64ExtendI32U);
        decode_temp.store(&mut function);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_POSITIVE_ASCII_CLASS as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        decode_temp.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::Else);
        decode_temp.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::End);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        function.instruction(&Instruction::End);
        match_utf16.load(&mut function);
        utf16_advance.load(&mut function);
        function.instruction(&Instruction::I64Sub);
        match_utf16.store(&mut function);
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_LITERAL_ASCII as i64));
        function.instruction(&Instruction::I64Eq);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_LITERAL_CODE_POINT as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_UNICODE_PROPERTY as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_POSITIVE_ASCII_CLASS as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_NEGATIVE_ASCII_CLASS as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_WHITESPACE as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_NOT_WHITESPACE as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_DOT as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        // Canonical operands are part of program validation, even when the
        // current input position cannot satisfy a consuming instruction.
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_WHITESPACE as i64));
        function.instruction(&Instruction::I64Eq);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_NOT_WHITESPACE as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_DOT as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64GtU);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_LITERAL_CODE_POINT as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10ffff));
        function.instruction(&Instruction::I64GtU);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_UNICODE_PROPERTY as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        // The referenced slice of the range pool must stay inside static data.
        operand0.load(&mut function);
        program_range_count.load(&mut function);
        function.instruction(&Instruction::I64GtU);
        operand1.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        program_range_count.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        // Literals, dot, negative ASCII classes, pooled classes and non-whitespace
        // may consume the low half of a paired scalar in legacy mode.
        match_on_low_surrogate.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_DOT as i64));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32And);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_LITERAL_CODE_POINT as i64,
        ));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32And);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_NEGATIVE_ASCII_CLASS as i64,
        ));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32And);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_NOT_WHITESPACE as i64));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32And);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_UNICODE_PROPERTY as i64,
        ));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32And);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        match_byte.load(&mut function);
        input_len.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        self.emit_regexp_scratch_byte(input_offset, match_byte, byte, &mut function);
        self.emit_regexp_scratch_decode_scalar(
            input_offset,
            match_byte,
            input_len,
            byte,
            codepoint,
            byte_advance,
            decode_temp,
            &mut function,
        );
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_DOT as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        for line_terminator in [0x000A_i64, 0x000D, 0x2028, 0x2029] {
            codepoint.load(&mut function);
            function.instruction(&Instruction::I64Const(line_terminator));
            function.instruction(&Instruction::I64Eq);
        }
        for _ in 1..4 {
            function.instruction(&Instruction::I32Or);
        }
        effective_dot_all.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32And);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        self.emit_regexp_advance_matched_character(
            match_byte,
            match_utf16,
            match_on_low_surrogate,
            unicode,
            codepoint,
            byte_advance,
            utf16_advance,
            &mut function,
        );
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_UNICODE_PROPERTY as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(&mut function);
        class_character.store(&mut function);
        unicode.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        // Membership uses the selected unit, but advancement still needs the
        // decoded scalar to retain or leave the paired-surrogate byte cursor.
        match_on_low_surrogate.load(&mut function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(0x3ff));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(0xdc00));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::Else);
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(0xd800));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::End);
        class_character.store(&mut function);
        function.instruction(&Instruction::End);
        self.emit_regexp_unicode_property_mismatch(
            range_base,
            operand0,
            operand1,
            class_character,
            range_count,
            range_low,
            range_high,
            range_middle,
            &mut function,
        );
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        self.emit_regexp_advance_matched_character(
            match_byte,
            match_utf16,
            match_on_low_surrogate,
            unicode,
            codepoint,
            byte_advance,
            utf16_advance,
            &mut function,
        );
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_NEGATIVE_ASCII_CLASS as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x7f));
        function.instruction(&Instruction::I64LeU);
        self.emit_regexp_ascii_class_contains(codepoint, operand0, operand1, &mut function);
        function.instruction(&Instruction::I32And);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        self.emit_regexp_advance_matched_character(
            match_byte,
            match_utf16,
            match_on_low_surrogate,
            unicode,
            codepoint,
            byte_advance,
            utf16_advance,
            &mut function,
        );
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(
            REGEXP_OPCODE_LITERAL_CODE_POINT as i64,
        ));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(&mut function);
        literal_value.store(&mut function);
        function.instruction(&Instruction::I64Const(1));
        literal_advance_byte.store(&mut function);
        function.instruction(&Instruction::I64Const(1));
        utf16_advance.store(&mut function);

        match_on_low_surrogate.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        unicode.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(0xd800));
        function.instruction(&Instruction::I64Add);
        literal_value.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        literal_advance_byte.store(&mut function);
        function.instruction(&Instruction::I64Const(1));
        match_on_low_surrogate.store(&mut function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        utf16_advance.store(&mut function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(0x3ff));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(0xdc00));
        function.instruction(&Instruction::I64Add);
        literal_value.store(&mut function);
        function.instruction(&Instruction::End);

        literal_value.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Ne);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        literal_advance_byte.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Else);
        self.emit_regexp_scratch_increment_by_local(match_byte, byte_advance, &mut function);
        function.instruction(&Instruction::I64Const(0));
        match_on_low_surrogate.store(&mut function);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment_by_local(match_utf16, utf16_advance, &mut function);
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_WHITESPACE as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_whitespace_mismatch(opcode, codepoint, &mut function);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        self.emit_regexp_scratch_increment_by_local(match_byte, byte_advance, &mut function);
        function.instruction(&Instruction::I64Const(1));
        utf16_advance.store(&mut function);
        self.emit_regexp_scratch_increment_by_local(match_utf16, utf16_advance, &mut function);
        self.emit_regexp_scratch_increment_by_local(pc, utf16_advance, &mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_NOT_WHITESPACE as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_whitespace_mismatch(opcode, codepoint, &mut function);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        self.emit_regexp_advance_matched_character(
            match_byte,
            match_utf16,
            match_on_low_surrogate,
            unicode,
            codepoint,
            byte_advance,
            utf16_advance,
            &mut function,
        );
        pc.load(&mut function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pc.store(&mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x80));
        function.instruction(&Instruction::I64GeU);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        opcode.load(&mut function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_LITERAL_ASCII as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(&mut function);
        operand0.load(&mut function);
        function.instruction(&Instruction::I64Ne);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        function.instruction(&Instruction::Else);
        self.emit_regexp_ascii_class_contains(codepoint, operand0, operand1, &mut function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_regexp_backtrack_request_if(backtrack.request(), &mut function);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment_by_local(match_byte, byte_advance, &mut function);
        function.instruction(&Instruction::I64Const(1));
        utf16_advance.store(&mut function);
        self.emit_regexp_scratch_increment_by_local(match_utf16, utf16_advance, &mut function);
        self.emit_regexp_scratch_increment_by_local(pc, utf16_advance, &mut function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        let recovery = backtrack.close(self, &mut function);
        self.emit_regexp_backtrack_recovery(
            &workspace,
            RegExpInputCursor {
                byte: match_byte,
                utf16: match_utf16,
                on_low_surrogate: match_on_low_surrogate,
            },
            pc,
            recovery,
            &mut function,
        );

        sticky.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        candidate_on_low_surrogate.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        candidate_on_low_surrogate.store(&mut function);
        function.instruction(&Instruction::I64Const(1));
        utf16_advance.store(&mut function);
        self.emit_regexp_scratch_byte(input_offset, candidate_byte, byte, &mut function);
        self.emit_regexp_scratch_decode_scalar(
            input_offset,
            candidate_byte,
            input_len,
            byte,
            codepoint,
            byte_advance,
            decode_temp,
            &mut function,
        );
        self.emit_regexp_scratch_increment_by_local(candidate_byte, byte_advance, &mut function);
        self.emit_regexp_scratch_increment_by_local(candidate_utf16, utf16_advance, &mut function);
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);
        candidate_byte.load(&mut function);
        input_len.load(&mut function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::NoMatch,
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_byte(input_offset, candidate_byte, byte, &mut function);
        self.emit_regexp_scratch_decode_scalar(
            input_offset,
            candidate_byte,
            input_len,
            byte,
            codepoint,
            byte_advance,
            decode_temp,
            &mut function,
        );
        function.instruction(&Instruction::I64Const(1));
        utf16_advance.store(&mut function);
        unicode.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        // Non-Unicode search exposes the low UTF-16 half as the next
        // candidate, retaining the scalar's byte cursor for code-unit atoms.
        function.instruction(&Instruction::I64Const(1));
        candidate_on_low_surrogate.store(&mut function);
        function.instruction(&Instruction::Else);
        self.emit_regexp_scratch_increment_by_local(candidate_byte, byte_advance, &mut function);
        function.instruction(&Instruction::I64Const(0));
        candidate_on_low_surrogate.store(&mut function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        self.emit_regexp_scratch_increment_by_local(candidate_byte, byte_advance, &mut function);
        function.instruction(&Instruction::I64Const(0));
        candidate_on_low_surrogate.store(&mut function);
        codepoint.load(&mut function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        utf16_advance.store(&mut function);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment_by_local(candidate_utf16, utf16_advance, &mut function);
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::NoMatch,
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        self.emit_regexp_match_result(
            checkpoint,
            candidate_utf16,
            candidate_utf16,
            RegExpMatcherResult::NoMatch,
            &mut function,
        );
        parameters.input.clear(&mut function);
        parameters.program.clear(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_regexp_read_utf16_unit(
        &mut self,
        input_offset: I64Local,
        input_len: I64Local,
        cursor_byte: I64Local,
        cursor_utf16: I64Local,
        cursor_on_low_surrogate: I64Local,
        unit: I64Local,
        byte: I64Local,
        codepoint: I64Local,
        byte_advance: I64Local,
        decode_temp: I64Local,
        function: &mut Function,
    ) {
        self.emit_regexp_scratch_byte(input_offset, cursor_byte, byte, function);
        self.emit_regexp_scratch_decode_scalar(
            input_offset,
            cursor_byte,
            input_len,
            byte,
            codepoint,
            byte_advance,
            decode_temp,
            function,
        );
        cursor_on_low_surrogate.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(0xd800));
        function.instruction(&Instruction::I64Add);
        unit.store(function);
        function.instruction(&Instruction::I64Const(1));
        cursor_on_low_surrogate.store(function);
        function.instruction(&Instruction::Else);
        codepoint.load(function);
        unit.store(function);
        self.emit_regexp_scratch_increment_by_local(cursor_byte, byte_advance, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        codepoint.load(function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(0x3ff));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(0xdc00));
        function.instruction(&Instruction::I64Add);
        unit.store(function);
        self.emit_regexp_scratch_increment_by_local(cursor_byte, byte_advance, function);
        function.instruction(&Instruction::I64Const(0));
        cursor_on_low_surrogate.store(function);
        function.instruction(&Instruction::End);
        cursor_utf16.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        cursor_utf16.store(function);
    }

    /// Raw branches keep the original matcher control-frame shape. The choice
    /// owner restores its own cursor and entire live slab before fallback.
    fn emit_regexp_instruction_load(
        &self,
        address_local: I64Local,
        delta: u64,
        output_local: I64Local,
        function: &mut Function,
    ) {
        address_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(Self::memarg8(delta)));
        output_local.store(function);
    }

    fn emit_regexp_scratch_increment_by_local(
        &self,
        local: I64Local,
        delta_local: I64Local,
        function: &mut Function,
    ) {
        local.load(function);
        delta_local.load(function);
        function.instruction(&Instruction::I64Add);
        local.store(function);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_regexp_advance_matched_character(
        &self,
        match_byte: I64Local,
        match_utf16: I64Local,
        match_on_low_surrogate: I64Local,
        unicode: I64Local,
        codepoint: I64Local,
        byte_advance: I64Local,
        utf16_advance: I64Local,
        function: &mut Function,
    ) {
        match_on_low_surrogate.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        unicode.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint.load(function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        match_on_low_surrogate.store(function);
        function.instruction(&Instruction::Else);
        self.emit_regexp_scratch_increment_by_local(match_byte, byte_advance, function);
        function.instruction(&Instruction::I64Const(0));
        match_on_low_surrogate.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(1));
        utf16_advance.store(function);
        function.instruction(&Instruction::Else);
        self.emit_regexp_scratch_increment_by_local(match_byte, byte_advance, function);
        function.instruction(&Instruction::I64Const(0));
        match_on_low_surrogate.store(function);
        codepoint.load(function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        utf16_advance.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        self.emit_regexp_scratch_increment_by_local(match_byte, byte_advance, function);
        function.instruction(&Instruction::I64Const(0));
        match_on_low_surrogate.store(function);
        function.instruction(&Instruction::I64Const(1));
        utf16_advance.store(function);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment_by_local(match_utf16, utf16_advance, function);
    }

    fn emit_regexp_whitespace_mismatch(
        &self,
        opcode: I64Local,
        codepoint: I64Local,
        function: &mut Function,
    ) {
        let whitespace_members = [
            0x0009_i64, 0x000A, 0x000B, 0x000C, 0x000D, 0x0020, 0x00A0, 0x1680, 0x2000, 0x2001,
            0x2002, 0x2003, 0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200A, 0x2028, 0x2029,
            0x202F, 0x205F, 0x3000, 0xFEFF,
        ];
        for member in whitespace_members {
            codepoint.load(function);
            function.instruction(&Instruction::I64Const(member));
            function.instruction(&Instruction::I64Eq);
        }
        for _ in 1..whitespace_members.len() {
            function.instruction(&Instruction::I32Or);
        }
        opcode.load(function);
        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_WHITESPACE as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Ne);
    }

    fn emit_regexp_ascii_class_contains(
        &self,
        codepoint_local: I64Local,
        low_bitmap_local: I64Local,
        high_bitmap_local: I64Local,
        function: &mut Function,
    ) {
        codepoint_local.load(function);
        function.instruction(&Instruction::I64Const(64));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        low_bitmap_local.load(function);
        function.instruction(&Instruction::Else);
        high_bitmap_local.load(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(1));
        codepoint_local.load(function);
        function.instruction(&Instruction::I64Const(63));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Ne);
        codepoint_local.load(function);
        function.instruction(&Instruction::I64Const(128));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
    }

    fn emit_regexp_match_result(
        &self,
        checkpoint: I64Local,
        start_local: I64Local,
        end_local: I64Local,
        result: RegExpMatcherResult,
        function: &mut Function,
    ) {
        let (found, status) = match result {
            RegExpMatcherResult::Match => (1, RegExpMatcherStatus::Complete),
            RegExpMatcherResult::NoMatch => (0, RegExpMatcherStatus::Complete),
            RegExpMatcherResult::Failed(failure) => (0, RegExpMatcherStatus::Failed(failure)),
        };
        self.emit_regexp_scratch_rewind(checkpoint, function);
        function.instruction(&Instruction::I32Const(found));
        start_local.load(function);
        end_local.load(function);
        function.instruction(&Instruction::I32Const(status.abi_word() as i32));
    }
}
