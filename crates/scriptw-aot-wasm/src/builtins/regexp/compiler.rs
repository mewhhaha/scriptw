use super::*;
use crate::gc_types::{
    CodeUnitArray, GcLocal, GcNullability, NonNullable, RegExpProgram, RegExpProgramConstruction,
    StringValue, StringValueSchema,
};
use crate::runtime_helpers::RegExpCompilerParameters;
use crate::runtime_helpers::RegExpCompilerStatus;
use scriptw_ir::{
    RegExpProgramWord, RegExpRepeatBoundWord, RegExpRepeatMaximumKind, REGEXP_MAX_INSTRUCTIONS,
    REGEXP_MAX_RANGE_ENTRIES, REGEXP_OPCODE_REPEAT_BEGIN, REGEXP_OPCODE_REPEAT_END,
    REGEXP_OPCODE_REPEAT_EXIT, REGEXP_OPCODE_REPEAT_GUARD, REGEXP_PROGRAM_HEADER_SIZE,
    REGEXP_PROGRAM_MAGIC_VERSION, REGEXP_REPEAT_BOUND_RECORD_SIZE,
    REGEXP_REPEAT_COUNTER_DECIMAL_DIGITS, REGEXP_REPEAT_COUNTER_LIMB_WIDTH,
    REGEXP_REPEAT_STATE_HEADER_SIZE,
};

mod captures;
mod compact_source;
mod contracts;
mod lowerer;
mod parser;
mod repeat_bounds;

use captures::CompletedCaptureInventory;
pub(super) use contracts::CompilerLocals;
use contracts::*;
use parser::CompletedRegExpPattern;

impl FunctionBuilder<'_> {
    /// Compiles admitted runtime Pattern grammar directly to the same immutable
    /// instruction descriptor used by statically compiled RegExps. The helper
    /// performs no JavaScript calls and owns everything above its entry private-byte
    /// checkpoint until publication or rollback.
    pub(crate) fn compile_regexp_compiler_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RegExpCompiler);
        let parameters = self.helper_parameters::<RegExpCompilerParameters>(&mut function);
        let compiler = self.reserve_regexp_compiler_locals(&mut function);
        function.instruction(&Instruction::GlobalGet(PRIVATE_BYTE_CURSOR_GLOBAL_INDEX));
        compiler.heap_checkpoint.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        compiler.cursor.store(&mut function);
        // Validate all flags directly from the GC String before source/workspace allocation.
        let character_mode =
            self.emit_regexp_compile_flags(&compiler, &parameters.flags, &mut function);
        self.emit_regexp_transient_text(
            &parameters.source,
            compiler.source_pointer,
            compiler.source_length,
            ScratchFailure::Compiler(&compiler),
            &mut function,
        )?;
        self.emit_regexp_compact_unicode_source(&compiler, &character_mode, &mut function);
        let prescan_flags = self.runtime_schema().reserve_i64_local(&mut function);
        function.instruction(&Instruction::I64Const(0));
        prescan_flags.store(&mut function);
        character_mode.emit_is_unicode(&mut function);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0x20));
        prescan_flags.store(&mut function);
        character_mode.emit_is_unicode_sets(&mut function);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0x40));
        prescan_flags.store(&mut function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_regexp_unicode_prescan(&compiler, prescan_flags, &mut function);
        self.runtime_schema()
            .release_i64_local(prescan_flags, &mut function);
        self.emit_regexp_compile_workspace(&compiler, &character_mode, &mut function);
        self.emit_regexp_compile_units(&compiler, &mut function);
        let result = self
            .emit_regexp_runtime_parse(&compiler, &character_mode, &mut function)
            .and_then(|pattern| {
                self.emit_regexp_runtime_lower(&compiler, &pattern, &mut function)?;
                self.emit_regexp_compile_publish(&compiler, pattern, &mut function)
            });
        self.release_regexp_compiler_character_mode(character_mode, &mut function);
        self.release_regexp_compiler_locals(compiler, &mut function);
        parameters.flags.clear(&mut function);
        parameters.source.clear(&mut function);
        function.instruction(&Instruction::End);
        result?;
        Ok(self.finish_function(function))
    }

    /// Retain the strict Unicode checks for ordinary Pattern/u class grammar.
    /// A v bracketed body is only skipped lexically here: its complete class,
    /// string, range and algebra grammar belongs to the consumed class parser.
    fn emit_regexp_unicode_prescan(
        &mut self,
        compiler: &CompilerLocals,
        seen: I64Local,
        function: &mut Function,
    ) {
        let src_ptr = self.runtime_schema().reserve_i64_local(function);
        let src_len = self.runtime_schema().reserve_i64_local(function);
        let idx = self.runtime_schema().reserve_i64_local(function);
        let depth = self.runtime_schema().reserve_i64_local(function);
        let is_v = self.runtime_schema().reserve_i64_local(function);
        let cur = self.runtime_schema().reserve_i64_local(function);
        let esc = self.runtime_schema().reserve_i64_local(function);
        let aux = self.runtime_schema().reserve_i64_local(function);
        let val = self.runtime_schema().reserve_i64_local(function);
        let hit = self.runtime_schema().reserve_i64_local(function);
        let done = self.runtime_schema().reserve_i64_local(function);
        let tmp = self.runtime_schema().reserve_i64_local(function);
        let start = self.runtime_schema().reserve_i64_local(function);
        let pdepth = self.runtime_schema().reserve_i64_local(function);
        // Gate the whole scan on the unicode flags; legacy patterns skip it.
        seen.load(function);
        function.instruction(&Instruction::I64Const(0x60));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        seen.load(function);
        function.instruction(&Instruction::I64Const(0x40));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I64ExtendI32U);
        is_v.store(function);
        compiler.source_pointer.load(function);
        src_ptr.store(function);
        compiler.source_length.load(function);
        src_len.store(function);
        for local in [idx, depth, start, pdepth] {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        idx.load(function);
        src_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_regexp_scratch_byte(src_ptr, idx, cur, function);
        cur.load(function);
        function.instruction(&Instruction::I64Const(b'[' as i64));
        function.instruction(&Instruction::I64Eq);
        is_v.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_prescan_skip_unicode_set_class(
            compiler, src_ptr, src_len, idx, depth, cur, function,
        );
        function.instruction(&Instruction::Br(1));
        function.instruction(&Instruction::End);
        // Dispatch only ordinary Pattern/u-class syntax outside that body.
        cur.load(function);
        function.instruction(&Instruction::I64Const(0x5c));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_prescan_escape(
            compiler, src_ptr, src_len, idx, depth, start, cur, esc, aux, val, hit, done, tmp,
            function,
        );
        function.instruction(&Instruction::Else);
        cur.load(function);
        function.instruction(&Instruction::I64Const(0x5b));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_prescan_open_class(idx, depth, start, function);
        function.instruction(&Instruction::Else);
        cur.load(function);
        function.instruction(&Instruction::I64Const(0x5d));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_prescan_close_class(compiler, idx, depth, function);
        function.instruction(&Instruction::Else);
        cur.load(function);
        function.instruction(&Instruction::I64Const(0x7d));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_prescan_close_brace(
            compiler, src_ptr, idx, depth, cur, aux, val, hit, function,
        );
        function.instruction(&Instruction::Else);
        cur.load(function);
        function.instruction(&Instruction::I64Const(0x7b));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_prescan_open_brace(
            compiler, src_ptr, src_len, idx, depth, cur, aux, function,
        );
        function.instruction(&Instruction::Else);
        // Only parentheses need a handler here; other ordinary characters
        // advance once. v operators are owned by the complete class parser.
        function.instruction(&Instruction::I64Const(0));
        hit.store(function);
        cur.load(function);
        function.instruction(&Instruction::I64Const(b'(' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_prescan_open_paren(idx, depth, pdepth, function);
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        function.instruction(&Instruction::End);
        cur.load(function);
        function.instruction(&Instruction::I64Const(b')' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_prescan_close_paren(compiler, idx, depth, pdepth, function);
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        function.instruction(&Instruction::End);
        hit.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_increment(idx, 1, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        // Unclosed classes and groups are invalid in every mode.
        depth.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::UnclosedClass),
            function,
        );
        function.instruction(&Instruction::End);
        pdepth.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::UnclosedGroup),
            function,
        );
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        for local in [
            pdepth, start, tmp, done, hit, val, aux, esc, cur, is_v, depth, idx, src_len, src_ptr,
        ] {
            self.runtime_schema().release_i64_local(local, function);
        }
    }

    /// Judge one `\` escape at `idx` (which points at the backslash).
    ///
    /// Sequential `done`-guarded arms instead of a nested chain: every arm
    /// either fails (which returns) or advances `idx` past the escape and
    /// sets `done`. Anything unmatched reaches the trailing `InvalidEscape`
    /// failure, which is correct because every legal unicode escape shape is
    /// claimed by an earlier arm.
    #[allow(clippy::too_many_arguments)]
    fn emit_regexp_prescan_escape(
        &mut self,
        compiler: &CompilerLocals,
        src_ptr: I64Local,
        src_len: I64Local,
        idx: I64Local,
        depth: I64Local,
        start: I64Local,
        cur: I64Local,
        esc: I64Local,
        aux: I64Local,
        val: I64Local,
        hit: I64Local,
        done: I64Local,
        tmp: I64Local,
        function: &mut Function,
    ) {
        // A trailing backslash is incomplete in every mode.
        self.emit_prescan_have(src_len, idx, 1, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_byte_at_delta(src_ptr, idx, 1, esc, function);
        function.instruction(&Instruction::I64Const(0));
        done.store(function);
        // `\c` requires an ASCII control letter.
        self.emit_prescan_arm_byte_eq(done, esc, b'c', function);
        self.emit_prescan_have(src_len, idx, 2, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_byte_at_delta(src_ptr, idx, 2, aux, function);
        self.emit_prescan_set_hit_alpha(aux, hit, function);
        hit.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 3, function);
        self.emit_prescan_arm_done(done, function);
        // `\0` must not be followed by a decimal digit (legacy octal).
        self.emit_prescan_arm_byte_eq(done, esc, b'0', function);
        self.emit_prescan_have(src_len, idx, 2, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte_at_delta(src_ptr, idx, 2, aux, function);
        self.emit_prescan_set_hit_range(aux, b'0', b'9' + 1, hit, function);
        hit.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 2, function);
        self.emit_prescan_arm_done(done, function);
        // `\x` requires exactly two hex digits.
        self.emit_prescan_arm_byte_eq(done, esc, b'x', function);
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        self.emit_prescan_require_hex_at(src_ptr, src_len, idx, 2, aux, val, hit, function);
        self.emit_prescan_require_hex_at(src_ptr, src_len, idx, 3, aux, val, hit, function);
        hit.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 4, function);
        self.emit_prescan_arm_done(done, function);
        // `\u` takes four hex digits or a complete braced code-point value.
        self.emit_prescan_arm_byte_eq(done, esc, b'u', function);
        self.emit_regexp_prescan_unicode_escape(
            compiler, src_ptr, src_len, idx, cur, esc, aux, val, hit, tmp, function,
        );
        self.emit_prescan_arm_done(done, function);
        // `\p`/`\P` must delimit one property; the full parser validates its body.
        self.emit_prescan_set_hit_bytes(esc, &[b'p', b'P'], hit, function);
        self.emit_prescan_arm_hit(done, hit, function);
        self.emit_prescan_throw_if_ranged_escape(
            compiler, src_ptr, idx, depth, start, cur, aux, hit, function,
        );
        self.emit_regexp_prescan_property_delimiters(
            compiler, src_ptr, src_len, idx, cur, aux, function,
        );
        self.emit_prescan_arm_done(done, function);
        // `\k` is atom-only; in a class it is an invalid escape. Outside a
        // class it must open a group-name run. The parser validates and resolves
        // the canonical name against the completed capture inventory.
        self.emit_prescan_arm_byte_eq(done, esc, b'k', function);
        depth.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_prescan_delimited_escape(
            compiler, src_ptr, src_len, idx, depth, cur, aux, hit, function,
        );
        self.emit_prescan_arm_done(done, function);
        // Class strings are parsed only by the full v class owner. q outside
        // that context is an invalid escape, rather than a capability shortcut.
        self.emit_prescan_arm_byte_eq(done, esc, b'q', function);
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        self.emit_prescan_arm_done(done, function);
        // `\B` is assertion-only; in a class it has no meaning.
        self.emit_prescan_arm_byte_eq(done, esc, b'B', function);
        depth.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 2, function);
        self.emit_prescan_arm_done(done, function);
        // Assertions and control escapes are shared atoms.
        self.emit_prescan_set_hit_bytes(esc, &[b'b', b'f', b'n', b'r', b't', b'v'], hit, function);
        self.emit_prescan_arm_hit(done, hit, function);
        self.emit_regexp_scratch_increment(idx, 2, function);
        self.emit_prescan_arm_done(done, function);
        // Character-class escapes are atoms, but never range endpoints.
        self.emit_prescan_set_hit_bytes(esc, &[b'd', b'D', b's', b'S', b'w', b'W'], hit, function);
        self.emit_prescan_arm_hit(done, hit, function);
        self.emit_prescan_throw_if_ranged_escape(
            compiler, src_ptr, idx, depth, start, cur, aux, hit, function,
        );
        self.emit_prescan_throw_if_dash_range(
            compiler, src_ptr, src_len, idx, depth, aux, 2, 3, function,
        );
        self.emit_regexp_scratch_increment(idx, 2, function);
        self.emit_prescan_arm_done(done, function);
        // Decimal escapes are atom-only; in a class they are illegal. Outside
        // a class they may be backreferences; group counting is punted.
        self.emit_prescan_set_hit_range(esc, b'1', b'9' + 1, hit, function);
        self.emit_prescan_arm_hit(done, hit, function);
        depth.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 2, function);
        self.emit_prescan_arm_done(done, function);
        // Every remaining ASCII letter is an illegal identity escape.
        self.emit_prescan_set_hit_alpha(esc, hit, function);
        self.emit_prescan_arm_hit(done, hit, function);
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        self.emit_prescan_arm_done(done, function);
        // Syntax characters and `/` are the legal identity escapes.
        self.emit_prescan_set_hit_bytes(
            esc,
            &[
                b'^', b'$', b'\\', b'.', b'*', b'+', b'?', b'(', b')', b'[', b']', b'{', b'}',
                b'|', b'/',
            ],
            hit,
            function,
        );
        self.emit_prescan_arm_hit(done, hit, function);
        self.emit_regexp_scratch_increment(idx, 2, function);
        self.emit_prescan_arm_done(done, function);
        // `\-` is a class escape; in an atom it is illegal in both modes.
        self.emit_prescan_arm_byte_eq(done, esc, b'-', function);
        depth.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 2, function);
        self.emit_prescan_arm_done(done, function);
        // Anything still unclaimed (including non-ASCII) is illegal here.
        done.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
    }

    /// Validate `\uHHHH` / `\u{H+}` at `idx` and advance past it.
    #[allow(clippy::too_many_arguments)]
    fn emit_regexp_prescan_unicode_escape(
        &mut self,
        compiler: &CompilerLocals,
        src_ptr: I64Local,
        src_len: I64Local,
        idx: I64Local,
        cur: I64Local,
        esc: I64Local,
        aux: I64Local,
        val: I64Local,
        hit: I64Local,
        tmp: I64Local,
        function: &mut Function,
    ) {
        // `hit` doubles as the braced-scan completion flag below.
        function.instruction(&Instruction::I64Const(0));
        hit.store(function);
        self.emit_prescan_have(src_len, idx, 2, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte_at_delta(src_ptr, idx, 2, aux, function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b'{' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        hit.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        // Braced run: one or more hex digits, closed, at most U+10FFFF.
        // `cur` is the scan cursor, `val` the accumulator, `esc` the count.
        idx.load(function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Add);
        cur.store(function);
        function.instruction(&Instruction::I64Const(0));
        val.store(function);
        function.instruction(&Instruction::I64Const(0));
        esc.store(function);
        function.instruction(&Instruction::I64Const(0));
        hit.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        hit.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::BrIf(1));
        cur.load(function);
        src_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_byte(src_ptr, cur, aux, function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b'}' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        esc.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        cur.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        idx.store(function);
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        function.instruction(&Instruction::Else);
        self.emit_prescan_set_hit_hex(aux, tmp, function);
        tmp.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_prescan_hex_val(aux, tmp, function);
        val.load(function);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        tmp.load(function);
        function.instruction(&Instruction::I64Add);
        val.store(function);
        val.load(function);
        function.instruction(&Instruction::I64Const(0x10_ffff));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(esc, 1, function);
        self.emit_regexp_scratch_increment(cur, 1, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        self.emit_prescan_require_hex_at(src_ptr, src_len, idx, 2, aux, val, hit, function);
        self.emit_prescan_require_hex_at(src_ptr, src_len, idx, 3, aux, val, hit, function);
        self.emit_prescan_require_hex_at(src_ptr, src_len, idx, 4, aux, val, hit, function);
        self.emit_prescan_require_hex_at(src_ptr, src_len, idx, 5, aux, val, hit, function);
        hit.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 6, function);
        function.instruction(&Instruction::End);
    }

    /// Properties proceed to the complete catalog/parser without capability taint.
    fn emit_regexp_prescan_property_delimiters(
        &mut self,
        compiler: &CompilerLocals,
        src_ptr: I64Local,
        src_len: I64Local,
        idx: I64Local,
        cur: I64Local,
        aux: I64Local,
        function: &mut Function,
    ) {
        self.emit_prescan_have(src_len, idx, 2, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_byte_at_delta(src_ptr, idx, 2, aux, function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b'{' as i64));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 3, function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        idx.load(function);
        src_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_byte(src_ptr, idx, cur, function);
        cur.load(function);
        function.instruction(&Instruction::I64Const(b'}' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::BrIf(1));
        self.emit_regexp_scratch_increment(idx, 1, function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 1, function);
    }

    /// Skip `open..=close` at `idx + 2`, failing when unopened or unclosed.
    ///
    /// Checks named-reference delimiters; the parser validates the name body.
    /// Ordinary Unicode class escapes remain invalid range endpoints.
    #[allow(clippy::too_many_arguments)]
    fn emit_regexp_prescan_delimited_escape(
        &mut self,
        compiler: &CompilerLocals,
        src_ptr: I64Local,
        src_len: I64Local,
        idx: I64Local,
        depth: I64Local,
        cur: I64Local,
        aux: I64Local,
        hit: I64Local,
        function: &mut Function,
    ) {
        let (open, close) = (b'<', b'>');
        self.emit_prescan_have(src_len, idx, 2, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_byte_at_delta(src_ptr, idx, 2, aux, function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(open as i64));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        idx.load(function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Add);
        cur.store(function);
        function.instruction(&Instruction::I64Const(0));
        hit.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        hit.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::BrIf(1));
        cur.load(function);
        src_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_byte(src_ptr, cur, aux, function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(close as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        cur.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        idx.store(function);
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        function.instruction(&Instruction::Else);
        self.emit_regexp_scratch_increment(cur, 1, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_prescan_throw_if_dash_range(
            compiler, src_ptr, src_len, idx, depth, aux, 0, 1, function,
        );
    }

    /// Ordinary u classes have one bracket level; raw [ inside is a member.
    fn emit_regexp_prescan_open_class(
        &mut self,
        idx: I64Local,
        depth: I64Local,
        start: I64Local,
        function: &mut Function,
    ) {
        depth.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_increment(depth, 1, function);
        idx.load(function);
        start.store(function);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 1, function);
    }

    fn emit_regexp_prescan_close_class(
        &mut self,
        compiler: &CompilerLocals,
        idx: I64Local,
        depth: I64Local,
        function: &mut Function,
    ) {
        depth.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::UnexpectedToken),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(depth, -1, function);
        self.emit_regexp_scratch_increment(idx, 1, function);
    }

    /// This lexical skip makes no syntax/capability decision inside a v class.
    /// Valid ClassSetCharacter/ClassString bodies cannot contain raw brackets;
    /// escaped brackets are skipped together, preserving real nesting depth.
    fn emit_regexp_prescan_skip_unicode_set_class(
        &mut self,
        compiler: &CompilerLocals,
        src_ptr: I64Local,
        src_len: I64Local,
        idx: I64Local,
        depth: I64Local,
        unit: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I64Const(1));
        depth.store(function);
        self.emit_regexp_scratch_increment(idx, 1, function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        depth.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::BrIf(1));
        idx.load(function);
        src_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::UnclosedClass),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_byte(src_ptr, idx, unit, function);
        unit.load(function);
        function.instruction(&Instruction::I64Const(b'\\' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_increment(idx, 2, function);
        function.instruction(&Instruction::Else);
        for (marker, delta) in [(b'[', 1), (b']', -1)] {
            unit.load(function);
            function.instruction(&Instruction::I64Const((marker as u64) as i64));
            function.instruction(&Instruction::I64Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            self.emit_regexp_scratch_increment(depth, delta, function);
            function.instruction(&Instruction::End);
        }
        self.emit_regexp_scratch_increment(idx, 1, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    /// Handle `}`: inside a class it is a literal. Outside a class it is
    /// only valid as a quantifier close, so backscan for a `{D}`, `{D,}` or
    /// `{D,D}` shape: a shape punts (the shared parser enforces the
    /// atom-before and min/max rules identically in every mode), anything
    /// else is a lone `}` and throws.
    #[allow(clippy::too_many_arguments)]
    fn emit_regexp_prescan_close_brace(
        &mut self,
        compiler: &CompilerLocals,
        src_ptr: I64Local,
        idx: I64Local,
        depth: I64Local,
        cur: I64Local,
        aux: I64Local,
        val: I64Local,
        hit: I64Local,
        function: &mut Function,
    ) {
        depth.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        hit.store(function);
        idx.load(function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        idx.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        cur.store(function);
        self.emit_regexp_scratch_byte(src_ptr, cur, aux, function);
        // A leading `,` selects the `{D,}` form: one digit run, then `{`.
        function.instruction(&Instruction::I64Const(0));
        val.store(function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b',' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        val.store(function);
        self.emit_regexp_scratch_increment(cur, -1, function);
        self.emit_regexp_scratch_byte(src_ptr, cur, aux, function);
        function.instruction(&Instruction::End);
        self.emit_prescan_push_is_digit(aux, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_prescan_consume_digit_run_back(src_ptr, cur, aux, function);
        cur.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::If(BlockType::Empty));
        val.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        aux.load(function);
        function.instruction(&Instruction::I64Const(b'{' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b'{' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        function.instruction(&Instruction::End);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b',' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        // `{D,D}` form: a second digit run, then `{`.
        self.emit_regexp_scratch_increment(cur, -1, function);
        cur.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte(src_ptr, cur, aux, function);
        self.emit_prescan_push_is_digit(aux, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_prescan_consume_digit_run_back(src_ptr, cur, aux, function);
        cur.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::If(BlockType::Empty));
        aux.load(function);
        function.instruction(&Instruction::I64Const(b'{' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        // A shape whose `{` is escaped (`\{1}`) is a lone `}`: the
        // backslash run before the `{` decides. (`cur` still points at
        // the `{` whenever `hit` is set: the `{D}` and `{D,D}` tests are
        // mutually exclusive, so no later scan clobbers it.)
        hit.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        val.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        cur.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::BrIf(1));
        self.emit_regexp_scratch_increment(cur, -1, function);
        self.emit_regexp_scratch_byte(src_ptr, cur, aux, function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b'\\' as i64));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::BrIf(1));
        val.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Xor);
        val.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        val.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        hit.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        hit.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::UnexpectedToken),
            function,
        );
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 1, function);
    }

    /// Handle `(`: outside classes it opens a group for balance tracking;
    /// inside classes it is a literal.
    fn emit_regexp_prescan_open_paren(
        &mut self,
        idx: I64Local,
        depth: I64Local,
        pdepth: I64Local,
        function: &mut Function,
    ) {
        depth.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_increment(pdepth, 1, function);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 1, function);
    }

    /// Handle `)`: a bare one outside any group is invalid in every mode;
    /// inside classes it is a literal.
    fn emit_regexp_prescan_close_paren(
        &mut self,
        compiler: &CompilerLocals,
        idx: I64Local,
        depth: I64Local,
        pdepth: I64Local,
        function: &mut Function,
    ) {
        depth.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        pdepth.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::UnexpectedToken),
            function,
        );
        function.instruction(&Instruction::End);
        pdepth.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        pdepth.store(function);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 1, function);
    }

    /// Throw when a character-class escape at `\\`(idx) starts a `-` range:
    /// `-` at `idx + dash_delta` followed by a real endpoint (present, not
    /// `]`) at `idx + end_delta`. A trailing or missing `-` leaves the `-`
    /// literal for the shared parser (or the `v` dash rule) to judge.
    #[allow(clippy::too_many_arguments)]
    fn emit_prescan_throw_if_dash_range(
        &self,
        compiler: &CompilerLocals,
        src_ptr: I64Local,
        src_len: I64Local,
        idx: I64Local,
        depth: I64Local,
        aux: I64Local,
        dash_delta: i64,
        end_delta: i64,
        function: &mut Function,
    ) {
        depth.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_prescan_have(src_len, idx, dash_delta, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte_at_delta(src_ptr, idx, dash_delta, aux, function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_prescan_have(src_len, idx, end_delta, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte_at_delta(src_ptr, idx, end_delta, aux, function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b']' as i64));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidRange),
            function,
        );
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    /// Throw when a character-class escape at `\\`(idx) ends a `-` range: a
    /// `-` immediately before it that is not class-initial. The check runs
    /// inside an ordinary Unicode class. UnicodeSets classes use the complete
    /// parser operand grammar instead of this lexical prescan.
    #[allow(clippy::too_many_arguments)]
    fn emit_prescan_throw_if_ranged_escape(
        &mut self,
        compiler: &CompilerLocals,
        src_ptr: I64Local,
        idx: I64Local,
        depth: I64Local,
        start: I64Local,
        cur: I64Local,
        aux: I64Local,
        hit: I64Local,
        function: &mut Function,
    ) {
        depth.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        idx.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte_at_delta(src_ptr, idx, -1, aux, function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        start.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        cur.store(function);
        self.emit_regexp_scratch_byte(src_ptr, cur, aux, function);
        function.instruction(&Instruction::I64Const(0));
        hit.store(function);
        idx.load(function);
        start.load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        function.instruction(&Instruction::End);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b'^' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        idx.load(function);
        start.load(function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        hit.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        hit.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidRange),
            function,
        );
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    /// Push i32 `byte - b'0' < 10` for ASCII digit tests.
    fn emit_prescan_push_is_digit(&self, byte: I64Local, function: &mut Function) {
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x30));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64LtU);
    }

    /// Consume a backwards digit run starting at `cur` (which must already
    /// hold a digit). Stops with `cur` on the first non-digit, or `-1` when
    /// the run reaches the pattern start; `aux` holds the stopping byte.
    fn emit_prescan_consume_digit_run_back(
        &self,
        src_ptr: I64Local,
        cur: I64Local,
        aux: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        self.emit_regexp_scratch_increment(cur, -1, function);
        cur.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::BrIf(1));
        self.emit_regexp_scratch_byte(src_ptr, cur, aux, function);
        self.emit_prescan_push_is_digit(aux, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    /// Handle `{`: inside a class it is a literal. Outside a class a `{`
    /// followed by digits starts a quantifier prefix, which must complete
    /// as `{D}`, `{D,}` or `{D,D}`: a prefix running into the pattern end
    /// (`{1`, `{1,2`) or into any other follower (`{1b`, `{1[`) throws. A
    /// well-formed shape punts to the `}` backscan (which punts shapes to
    /// the shared parser for the atom-before check).
    #[allow(clippy::too_many_arguments)]
    fn emit_regexp_prescan_open_brace(
        &mut self,
        compiler: &CompilerLocals,
        src_ptr: I64Local,
        src_len: I64Local,
        idx: I64Local,
        depth: I64Local,
        cur: I64Local,
        aux: I64Local,
        function: &mut Function,
    ) {
        depth.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_prescan_have(src_len, idx, 1, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte_at_delta(src_ptr, idx, 1, aux, function);
        self.emit_prescan_push_is_digit(aux, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        idx.load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Add);
        cur.store(function);
        self.emit_prescan_consume_digit_run_forward(src_ptr, src_len, cur, aux, function);
        cur.load(function);
        src_len.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte(src_ptr, cur, aux, function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b',' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_increment(cur, 1, function);
        self.emit_prescan_consume_digit_run_forward(src_ptr, src_len, cur, aux, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        cur.load(function);
        src_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::UnexpectedToken),
            function,
        );
        function.instruction(&Instruction::End);
        // A non-`}` follower also breaks the quantifier: `{1b`, `{1[`.
        cur.load(function);
        src_len.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte(src_ptr, cur, aux, function);
        aux.load(function);
        function.instruction(&Instruction::I64Const(b'}' as i64));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::UnexpectedToken),
            function,
        );
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(idx, 1, function);
    }

    /// Consume a forwards digit run starting at `cur`, stopping at the
    /// first non-digit or the pattern end.
    fn emit_prescan_consume_digit_run_forward(
        &self,
        src_ptr: I64Local,
        src_len: I64Local,
        cur: I64Local,
        aux: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        cur.load(function);
        src_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_regexp_scratch_byte(src_ptr, cur, aux, function);
        self.emit_prescan_push_is_digit(aux, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        self.emit_regexp_scratch_increment(cur, 1, function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    /// Push i32 `idx + delta < len` for a bounded lookahead.
    fn emit_prescan_have(&self, len: I64Local, idx: I64Local, delta: i64, function: &mut Function) {
        idx.load(function);
        function.instruction(&Instruction::I64Const(delta));
        function.instruction(&Instruction::I64Add);
        len.load(function);
        function.instruction(&Instruction::I64LtU);
    }

    /// Open `if (!done && byte == expected) {`.
    fn emit_prescan_arm_byte_eq(
        &self,
        done: I64Local,
        byte: I64Local,
        expected: u8,
        function: &mut Function,
    ) {
        done.load(function);
        function.instruction(&Instruction::I64Eqz);
        byte.load(function);
        function.instruction(&Instruction::I64Const(expected as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
    }

    /// Open `if (!done && hit) {` for a precomputed membership local.
    fn emit_prescan_arm_hit(&self, done: I64Local, hit: I64Local, function: &mut Function) {
        done.load(function);
        function.instruction(&Instruction::I64Eqz);
        hit.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
    }

    /// Close an arm body by marking it claimed.
    fn emit_prescan_arm_done(&self, done: I64Local, function: &mut Function) {
        function.instruction(&Instruction::I64Const(1));
        done.store(function);
        function.instruction(&Instruction::End);
    }

    /// Set `hit = lo <= byte < hi` as i64 0/1.
    fn emit_prescan_set_hit_range(
        &self,
        byte: I64Local,
        lo: u8,
        hi_exclusive: u8,
        hit: I64Local,
        function: &mut Function,
    ) {
        byte.load(function);
        function.instruction(&Instruction::I64Const(lo as i64));
        function.instruction(&Instruction::I64GeU);
        byte.load(function);
        function.instruction(&Instruction::I64Const(hi_exclusive as i64));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I64ExtendI32U);
        hit.store(function);
    }

    /// Set `hit` for ASCII letters.
    fn emit_prescan_set_hit_alpha(&self, byte: I64Local, hit: I64Local, function: &mut Function) {
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x41));
        function.instruction(&Instruction::I64GeU);
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x5b));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x61));
        function.instruction(&Instruction::I64GeU);
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x7b));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I64ExtendI32U);
        hit.store(function);
    }

    /// Set `hit` for ASCII hex digits.
    fn emit_prescan_set_hit_hex(&self, byte: I64Local, hit: I64Local, function: &mut Function) {
        self.emit_prescan_set_hit_range(byte, b'0', b'9' + 1, hit, function);
        hit.load(function);
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x41));
        function.instruction(&Instruction::I64GeU);
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x47));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Or);
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x61));
        function.instruction(&Instruction::I64GeU);
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x67));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Or);
        hit.store(function);
    }

    /// Set `hit` for membership in an explicit byte list.
    fn emit_prescan_set_hit_bytes(
        &self,
        byte: I64Local,
        expected: &[u8],
        hit: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I64Const(0));
        hit.store(function);
        for candidate in expected {
            self.emit_regexp_scratch_or_byte_flag(byte, *candidate, hit, function);
        }
    }

    /// Set `val` to the hex value of `byte` (caller must validate hex first).
    fn emit_prescan_hex_val(&self, byte: I64Local, val: I64Local, function: &mut Function) {
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x30));
        function.instruction(&Instruction::I64Sub);
        val.store(function);
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x41));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        val.load(function);
        function.instruction(&Instruction::I64Const(7));
        function.instruction(&Instruction::I64Sub);
        val.store(function);
        function.instruction(&Instruction::End);
        byte.load(function);
        function.instruction(&Instruction::I64Const(0x61));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        val.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Sub);
        val.store(function);
        function.instruction(&Instruction::End);
    }

    /// Clear `hit` unless `idx + at` holds a hex digit. Uses `aux`/`val`.
    #[allow(clippy::too_many_arguments)]
    fn emit_prescan_require_hex_at(
        &self,
        src_ptr: I64Local,
        src_len: I64Local,
        idx: I64Local,
        at: i64,
        aux: I64Local,
        val: I64Local,
        hit: I64Local,
        function: &mut Function,
    ) {
        self.emit_prescan_have(src_len, idx, at, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        hit.store(function);
        function.instruction(&Instruction::End);
        self.emit_prescan_have(src_len, idx, at, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_scratch_byte_at_delta(src_ptr, idx, at, aux, function);
        self.emit_prescan_set_hit_hex(aux, val, function);
        val.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        hit.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    fn emit_regexp_compile_workspace(
        &mut self,
        compiler: &CompilerLocals,
        character_mode: &CompilerCharacterMode,
        function: &mut Function,
    ) {
        let end = self.runtime_schema().reserve_i64_local(function);
        compiler.source_length.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        compiler.unit_capacity.store(function);
        compiler.unit_capacity.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        compiler.node_capacity.store(function);
        compiler.node_capacity.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Const(
            (8 * REGEXP_MAX_INSTRUCTIONS + 32) as i64,
        ));
        function.instruction(&Instruction::I64Add);
        compiler.task_capacity.store(function);
        let mut workspace = self.begin_regexp_workspace(compiler, end, function);
        // All products fit u64: each capacity is linear in a u32 byte length.
        // Addressability is checked before memory.grow or any workspace write.
        for (pointer, count, stride) in [
            (compiler.units, compiler.unit_capacity, 8),
            (compiler.nodes, compiler.node_capacity, NODE_BYTES),
            (compiler.tasks, compiler.task_capacity, TASK_BYTES),
            (
                compiler.name_rows,
                compiler.unit_capacity,
                captures::NAME_ROW_BYTES,
            ),
            (compiler.name_bytes, compiler.unit_capacity, 4),
            (compiler.repeat_rows, compiler.node_capacity, 8),
        ] {
            workspace.reserve_elements(pointer, count, stride, function);
        }
        for (pointer, length) in [
            (compiler.graph_visited, REGEXP_MAX_INSTRUCTIONS as u64 * 8),
            (compiler.graph_work, REGEXP_MAX_INSTRUCTIONS as u64 * 8),
            (compiler.graph_colors, REGEXP_MAX_INSTRUCTIONS as u64 * 8),
            (compiler.graph_edges, REGEXP_MAX_INSTRUCTIONS as u64 * 8),
            (compiler.descriptor, REGEXP_PROGRAM_HEADER_SIZE as u64),
            (
                compiler.instructions,
                REGEXP_MAX_INSTRUCTIONS as u64 * REGEXP_INSTRUCTION_WIDTH as u64,
            ),
            (
                compiler.ranges,
                REGEXP_MAX_RANGE_ENTRIES as u64 * REGEXP_RANGE_ENTRY_WIDTH as u64,
            ),
        ] {
            workspace.reserve_bytes(pointer, length, function);
        }
        workspace.reserve_character_bitmap(compiler.class_bitmap, character_mode, function);
        workspace.reserve_character_bitmap(compiler.folded_bitmap, character_mode, function);
        workspace.reserve_character_bitmap(compiler.property_bitmap, character_mode, function);
        character_mode.emit_is_unicode_sets(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        workspace.reserve_elements(
            compiler.class_set_frames,
            compiler.unit_capacity,
            CLASS_SET_FRAME_BYTES,
            function,
        );
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        compiler.class_set_frames.store(function);
        function.instruction(&Instruction::End);
        let region = workspace.finish(self, compiler, function);
        self.emit_regexp_commit_workspace(compiler, region, function);
        self.runtime_schema().release_i64_local(end, function);
    }

    fn emit_regexp_compile_units(&mut self, compiler: &CompilerLocals, function: &mut Function) {
        let index = self.runtime_schema().reserve_i64_local(function);
        let byte = self.runtime_schema().reserve_i64_local(function);
        let codepoint = self.runtime_schema().reserve_i64_local(function);
        let advance = self.runtime_schema().reserve_i64_local(function);
        let temp = self.runtime_schema().reserve_i64_local(function);
        for local in [index, compiler.unit_count] {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        compiler.source_length.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_regexp_scratch_byte_at_delta(compiler.source_pointer, index, 0, byte, function);
        self.emit_regexp_scratch_decode_scalar(
            compiler.source_pointer,
            index,
            compiler.source_length,
            byte,
            codepoint,
            advance,
            temp,
            function,
        );
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
        temp.store(function);
        self.emit_regexp_compile_store_unit(compiler, temp, function);
        codepoint.load(function);
        function.instruction(&Instruction::I64Const(0x3ff));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(0xdc00));
        function.instruction(&Instruction::I64Add);
        codepoint.store(function);
        function.instruction(&Instruction::End);
        self.emit_regexp_compile_store_unit(compiler, codepoint, function);
        self.emit_regexp_scratch_increment_by_local(index, advance, function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        for local in [temp, advance, codepoint, byte, index] {
            self.runtime_schema().release_i64_local(local, function);
        }
    }

    fn emit_regexp_compile_store_unit(
        &self,
        compiler: &CompilerLocals,
        unit: I64Local,
        function: &mut Function,
    ) {
        compiler.units.load(function);
        compiler.unit_count.load(function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        unit.load(function);
        function.instruction(&Instruction::I64Store(Self::memarg8(0)));
        self.emit_regexp_scratch_increment(compiler.unit_count, 1, function);
    }

    pub(super) fn emit_regexp_compile_corrupt_program_failure(
        &self,
        compiler: &CompilerLocals,
        function: &mut Function,
    ) {
        self.emit_regexp_compile_failure(compiler, CompileFailure::Corrupt, function);
    }

    pub(super) fn emit_regexp_compile_scratch_failure(
        &self,
        compiler: &CompilerLocals,
        function: &mut Function,
    ) {
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Resource(CompileResource::MemoryGrowth),
            function,
        );
    }

    fn emit_regexp_compile_publish(
        &mut self,
        compiler: &CompilerLocals,
        pattern: CompletedRegExpPattern,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let length = self.runtime_schema().reserve_i64_local(function);
        let ranges = self.runtime_schema().reserve_i64_local(function);
        let named_offset = self.runtime_schema().reserve_i64_local(function);
        let schema = self.runtime_schema();
        compiler.instructions.load(function);
        compiler.instruction_count.load(function);
        function.instruction(&Instruction::I64Const(REGEXP_INSTRUCTION_WIDTH as i64));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        ranges.store(function);
        ranges.load(function);
        function.instruction(&Instruction::I32WrapI64);
        compiler.ranges.load(function);
        function.instruction(&Instruction::I32WrapI64);
        compiler.range_count.load(function);
        function.instruction(&Instruction::I64Const(REGEXP_RANGE_ENTRY_WIDTH as i64));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        });
        ranges.load(function);
        compiler.descriptor.load(function);
        function.instruction(&Instruction::I64Sub);
        compiler.range_count.load(function);
        function.instruction(&Instruction::I64Const(REGEXP_RANGE_ENTRY_WIDTH as i64));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        length.store(function);
        self.emit_regexp_compile_repeat_bounds(compiler, length, function);
        self.emit_regexp_capture_table(
            compiler,
            pattern.into_capture_inventory(),
            length,
            named_offset,
            function,
        );
        for word in RegExpProgramWord::ALL {
            match word {
                RegExpProgramWord::MagicVersion => self.emit_regexp_scratch_store_word_const(
                    compiler.descriptor,
                    word.offset(),
                    REGEXP_PROGRAM_MAGIC_VERSION,
                    function,
                ),
                RegExpProgramWord::NamedGroupTableOffset => self.emit_regexp_scratch_store_word(
                    compiler.descriptor,
                    word.offset(),
                    named_offset,
                    function,
                ),
                RegExpProgramWord::ByteLength => self.emit_regexp_scratch_store_word(
                    compiler.descriptor,
                    word.offset(),
                    length,
                    function,
                ),
                RegExpProgramWord::InstructionCount => self.emit_regexp_scratch_store_word(
                    compiler.descriptor,
                    word.offset(),
                    compiler.instruction_count,
                    function,
                ),
                RegExpProgramWord::CaptureCount => self.emit_regexp_scratch_store_word(
                    compiler.descriptor,
                    word.offset(),
                    compiler.capture_count,
                    function,
                ),
                RegExpProgramWord::RangeCount => self.emit_regexp_scratch_store_word(
                    compiler.descriptor,
                    word.offset(),
                    compiler.range_count,
                    function,
                ),
                RegExpProgramWord::SplitCount => self.emit_regexp_scratch_store_word(
                    compiler.descriptor,
                    word.offset(),
                    compiler.split_count,
                    function,
                ),
                RegExpProgramWord::RepeatSlotCount => self.emit_regexp_scratch_store_word(
                    compiler.descriptor,
                    word.offset(),
                    compiler.repeat_slot_count,
                    function,
                ),
                RegExpProgramWord::RepeatStateByteLength => self.emit_regexp_scratch_store_word(
                    compiler.descriptor,
                    word.offset(),
                    compiler.repeat_state_byte_length,
                    function,
                ),
                RegExpProgramWord::RepeatableSplitCount => self.emit_regexp_scratch_store_word(
                    compiler.descriptor,
                    word.offset(),
                    compiler.repeatable_split_count,
                    function,
                ),
            }
        }
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let byte = schema.reserve_i32_local(function);
        length.load(function);
        function.instruction(&Instruction::I32WrapI64);
        count.store(function);
        let construction = RegExpProgramConstruction::allocate(schema, count, function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let copied = self.open_frame(ControlFrameKind::Block, function);
        let copying = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(copied, function);
        compiler.descriptor.load(function);
        function.instruction(&Instruction::I32WrapI64);
        index.load(function);
        function.instruction(&Instruction::I32Add);
        function.instruction(&Instruction::I32Load8U(Self::memarg8(0)));
        byte.store(function);
        construction.write(index, byte, schema, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(copying, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let program = schema
            .reserve_gc_local::<RegExpProgram, NonNullable>(function)
            .initialize(construction.publish(schema, function), function);
        let pending_layout = self.reserve_regexp_program_layout(function);
        let layout = self.emit_validate_regexp_program_layout(
            &program,
            pending_layout,
            RegExpProgramLayoutFailure::Compiler(compiler),
            function,
        )?;
        self.release_regexp_program_layout(layout, function);
        self.emit_regexp_compile_release_tail(compiler.heap_checkpoint, function);
        program.load(schema, function);
        function.instruction(&Instruction::I32Const(
            RegExpCompilerStatus::Compiled.abi_word() as i32,
        ));
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Const(0));
        program.clear(function);
        schema.release_i32_local(byte, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        for local in [named_offset, ranges, length] {
            self.runtime_schema().release_i64_local(local, function);
        }
        Ok(())
    }
}
