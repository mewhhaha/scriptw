use super::escapes::CharacterContext;
use super::*;
use crate::data::{
    RegExpUnicodePropertyImageKind as ImageKind,
    RegExpUnicodePropertyImageStringWord as ImageStringWord,
    RegExpUnicodePropertyImageWord as ImageWord,
};

#[derive(Clone, Copy)]
#[repr(u64)]
enum OperandKind {
    Character,
    Builtin,
    CodePoints,
    Strings,
}

/// Only the mode-owned decoder can mint an operand. A property never becomes
/// a character range endpoint; string properties retain exact finite keys.
#[must_use]
pub(super) struct ParsedCharacterSetOperand {
    kind: I64Local,
    character: I64Local,
    builtin: I64Local,
    row: I64Local,
    complement: I64Local,
    source_offset: I64Local,
}

impl ParsedCharacterSetOperand {
    pub(super) fn emit_is_character(&self, function: &mut Function) {
        eq(function, self.kind, OperandKind::Character as u64);
    }
    pub(super) fn emit_character_to(&self, destination: I64Local, function: &mut Function) {
        copy(function, destination, self.character);
    }
    pub(super) fn emit_range(
        &self,
        last: &Self,
        emitter: &mut FunctionBuilder<'_>,
        compiler: &CompilerLocals,
        mode: &CompilerCharacterMode,
        destination: I64Local,
        function: &mut Function,
    ) {
        self.emit_is_character(function);
        last.emit_is_character(function);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        emitter.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidRange),
            function,
        );
        function.instruction(&Instruction::End);
        self.character.load(function);
        last.character.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        emitter.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidRange),
            function,
        );
        function.instruction(&Instruction::End);
        emitter.emit_regexp_parser_bitmap_range(
            compiler,
            mode,
            destination,
            self.character,
            last.character,
            function,
        );
    }
    pub(super) fn emit_add(
        &self,
        emitter: &mut FunctionBuilder<'_>,
        compiler: &CompilerLocals,
        mode: &CompilerCharacterMode,
        modifiers: &ModifierLocals,
        destination: I64Local,
        strings: &finite::FiniteStringSet,
        may_contain_strings: I64Local,
        function: &mut Function,
    ) {
        eq(function, self.kind, OperandKind::Strings as u64);
        function.instruction(&Instruction::If(BlockType::Empty));
        set(function, may_contain_strings, 1);
        emitter.emit_regexp_parser_add_string_property(
            self, compiler, mode, modifiers, strings, function,
        );
        function.instruction(&Instruction::Else);
        eq(function, self.kind, OperandKind::CodePoints as u64);
        function.instruction(&Instruction::If(BlockType::Empty));
        emitter.emit_regexp_parser_add_property(
            self,
            compiler,
            mode,
            modifiers,
            destination,
            function,
        );
        function.instruction(&Instruction::Else);
        emitter.emit_regexp_parser_add_character_set(
            compiler,
            mode,
            modifiers,
            destination,
            self.character,
            self.builtin,
            function,
        );
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }
    pub(super) fn emit_atom(
        self,
        emitter: &mut FunctionBuilder<'_>,
        compiler: &CompilerLocals,
        mode: &CompilerCharacterMode,
        parser: &ParserLocals,
        function: &mut Function,
    ) {
        let may_contain_strings = emitter.runtime_schema().reserve_i64_local(function);
        let strings = finite::FiniteStringSet::reserve(emitter, function);
        set(function, may_contain_strings, 0);
        bitmap::clear_bitmap(function, compiler.class_bitmap, mode);
        self.emit_add(
            emitter,
            compiler,
            mode,
            &parser.modifiers,
            compiler.class_bitmap,
            &strings,
            may_contain_strings,
            function,
        );
        emitter.emit_regexp_finish_finite_class_set(
            compiler,
            mode,
            parser.address,
            &parser.modifiers,
            &strings,
            function,
        );
        strings.release(emitter, function);
        emitter
            .runtime_schema()
            .release_i64_local(may_contain_strings, function);
        self.release(emitter, function);
    }
    pub(super) fn release(self, emitter: &mut FunctionBuilder<'_>, function: &mut Function) {
        for local in [
            self.source_offset,
            self.complement,
            self.row,
            self.builtin,
            self.character,
            self.kind,
        ] {
            emitter.runtime_schema().release_i64_local(local, function);
        }
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_regexp_parser_character_set_operand(
        &mut self,
        compiler: &CompilerLocals,
        mode: &CompilerCharacterMode,
        context: CharacterContext,
        function: &mut Function,
    ) -> ParsedCharacterSetOperand {
        let operand = ParsedCharacterSetOperand {
            kind: self.runtime_schema().reserve_i64_local(function),
            character: self.runtime_schema().reserve_i64_local(function),
            builtin: self.runtime_schema().reserve_i64_local(function),
            row: self.runtime_schema().reserve_i64_local(function),
            complement: self.runtime_schema().reserve_i64_local(function),
            source_offset: self.runtime_schema().reserve_i64_local(function),
        };
        let unit = self.runtime_schema().reserve_i64_local(function);
        let escape = self.runtime_schema().reserve_i64_local(function);
        copy(function, operand.source_offset, compiler.cursor);
        for local in [
            operand.kind,
            operand.character,
            operand.builtin,
            operand.row,
            operand.complement,
        ] {
            set(function, local, 0);
        }
        peek(compiler, function, compiler.cursor, 0, unit);
        peek(compiler, function, compiler.cursor, 1, escape);
        eq(function, unit, b'\\' as u64);
        eq(function, escape, b'p' as u64);
        eq(function, escape, b'P' as u64);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32And);
        mode.emit_is_unicode(function);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        eq(function, escape, b'P' as u64);
        function.instruction(&Instruction::I64ExtendI32U);
        operand.complement.store(function);
        self.emit_regexp_parser_property(compiler, mode, &operand, function);
        function.instruction(&Instruction::Else);
        self.emit_regexp_parser_character(
            compiler,
            mode,
            context,
            operand.character,
            operand.builtin,
            function,
        );
        eq(function, operand.builtin, 0);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I64ExtendI32U);
        operand.kind.store(function);
        function.instruction(&Instruction::End);
        self.runtime_schema().release_i64_local(escape, function);
        self.runtime_schema().release_i64_local(unit, function);
        operand
    }

    /// Exact image-key comparison validates the entire nonempty normative
    /// property grammar and aliases. No provider loose spelling reaches here.
    fn emit_regexp_parser_property(
        &mut self,
        compiler: &CompilerLocals,
        mode: &CompilerCharacterMode,
        operand: &ParsedCharacterSetOperand,
        function: &mut Function,
    ) {
        let start = self.runtime_schema().reserve_i64_local(function);
        let length = self.runtime_schema().reserve_i64_local(function);
        let index = self.runtime_schema().reserve_i64_local(function);
        let row = self.runtime_schema().reserve_i64_local(function);
        let name = self.runtime_schema().reserve_i64_local(function);
        let name_length = self.runtime_schema().reserve_i64_local(function);
        let position = self.runtime_schema().reserve_i64_local(function);
        let cursor = self.runtime_schema().reserve_i64_local(function);
        let unit = self.runtime_schema().reserve_i64_local(function);
        let byte = self.runtime_schema().reserve_i64_local(function);
        let matched = self.runtime_schema().reserve_i64_local(function);
        let kind = self.runtime_schema().reserve_i64_local(function);
        let image = self.strings.regexp_unicode_property_image();
        self.runtime_schema().ensure_regexp_unicode_properties(
            self.strings.regexp_unicode_property_region(),
            function,
        );
        self.emit_regexp_scratch_increment(compiler.cursor, 2, function);
        peek(compiler, function, compiler.cursor, 0, unit);
        eq(function, unit, b'{' as u64);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(compiler.cursor, 1, function);
        copy(function, start, compiler.cursor);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        peek(compiler, function, compiler.cursor, 0, unit);
        eq(function, unit, u64::MAX);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        eq(function, unit, b'}' as u64);
        function.instruction(&Instruction::BrIf(1));
        self.emit_regexp_scratch_increment(compiler.cursor, 1, function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        compiler.cursor.load(function);
        start.load(function);
        function.instruction(&Instruction::I64Sub);
        length.store(function);
        self.emit_regexp_scratch_increment(compiler.cursor, 1, function);
        set(function, index, 0);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        function.instruction(&Instruction::I64Const(image.count() as i64));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        function.instruction(&Instruction::I64Const(image.rows() as i64));
        index.load(function);
        function.instruction(&Instruction::I64Const(ImageWord::ROW_BYTES as i64));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        row.store(function);
        load(function, row, ImageWord::NamePointer as u64, name);
        load(function, row, ImageWord::NameLength as u64, name_length);
        length.load(function);
        name_length.load(function);
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        set(function, position, 0);
        set(function, matched, 1);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        position.load(function);
        length.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        start.load(function);
        position.load(function);
        function.instruction(&Instruction::I64Add);
        cursor.store(function);
        peek(compiler, function, cursor, 0, unit);
        name.load(function);
        position.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load8U(MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }));
        byte.store(function);
        unit.load(function);
        byte.load(function);
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        set(function, matched, 0);
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(position, 1, function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        eq(function, matched, 1);
        function.instruction(&Instruction::If(BlockType::Empty));
        copy(function, operand.row, row);
        function.instruction(&Instruction::Br(3));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_regexp_scratch_increment(index, 1, function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        eq(function, operand.row, 0);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        load(function, operand.row, ImageWord::Kind as u64, kind);
        eq(function, kind, ImageKind::Strings as u64);
        function.instruction(&Instruction::If(BlockType::Empty));
        mode.emit_is_unicode_sets(function);
        function.instruction(&Instruction::I32Eqz);
        eq(function, operand.complement, 1);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(
            compiler,
            CompileFailure::Syntax(CompileSyntax::InvalidEscape),
            function,
        );
        function.instruction(&Instruction::End);
        set(function, operand.kind, OperandKind::Strings as u64);
        function.instruction(&Instruction::Else);
        eq(function, kind, ImageKind::CodePoints as u64);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_compile_failure(compiler, CompileFailure::Corrupt, function);
        function.instruction(&Instruction::End);
        set(function, operand.kind, OperandKind::CodePoints as u64);
        function.instruction(&Instruction::End);
        for local in [
            kind,
            matched,
            byte,
            unit,
            cursor,
            position,
            name_length,
            name,
            row,
            index,
            length,
            start,
        ] {
            self.runtime_schema().release_i64_local(local, function);
        }
    }

    fn emit_regexp_parser_add_string_property(
        &mut self,
        operand: &ParsedCharacterSetOperand,
        compiler: &CompilerLocals,
        mode: &CompilerCharacterMode,
        modifiers: &ModifierLocals,
        strings: &finite::FiniteStringSet,
        function: &mut Function,
    ) {
        let pointer = self.runtime_schema().reserve_i64_local(function);
        let count = self.runtime_schema().reserve_i64_local(function);
        let index = self.runtime_schema().reserve_i64_local(function);
        let row = self.runtime_schema().reserve_i64_local(function);
        let points = self.runtime_schema().reserve_i64_local(function);
        let length = self.runtime_schema().reserve_i64_local(function);
        load(
            function,
            operand.row,
            ImageWord::PayloadPointer as u64,
            pointer,
        );
        load(function, operand.row, ImageWord::PayloadCount as u64, count);
        set(function, index, 0);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        pointer.load(function);
        index.load(function);
        function.instruction(&Instruction::I64Const(ImageStringWord::ROW_BYTES as i64));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        row.store(function);
        load(
            function,
            row,
            ImageStringWord::CodePointsPointer as u64,
            points,
        );
        load(
            function,
            row,
            ImageStringWord::CodePointCount as u64,
            length,
        );
        strings.emit_insert_image(self, compiler, mode, modifiers, points, length, function);
        self.emit_regexp_scratch_increment(index, 1, function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        for local in [length, points, row, index, count, pointer] {
            self.runtime_schema().release_i64_local(local, function);
        }
    }

    fn emit_regexp_parser_add_property(
        &mut self,
        operand: &ParsedCharacterSetOperand,
        compiler: &CompilerLocals,
        mode: &CompilerCharacterMode,
        modifiers: &ModifierLocals,
        destination: I64Local,
        function: &mut Function,
    ) {
        let pointer = self.runtime_schema().reserve_i64_local(function);
        let count = self.runtime_schema().reserve_i64_local(function);
        let index = self.runtime_schema().reserve_i64_local(function);
        let address = self.runtime_schema().reserve_i64_local(function);
        let first = self.runtime_schema().reserve_i64_local(function);
        let last = self.runtime_schema().reserve_i64_local(function);
        bitmap::clear_bitmap(function, compiler.property_bitmap, mode);
        load(
            function,
            operand.row,
            ImageWord::PayloadPointer as u64,
            pointer,
        );
        load(function, operand.row, ImageWord::PayloadCount as u64, count);
        set(function, index, 0);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        pointer.load(function);
        index.load(function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        address.store(function);
        for (offset, target) in [(0, first), (4, last)] {
            address.load(function);
            function.instruction(&Instruction::I32WrapI64);
            function.instruction(&Instruction::I64Load32U(MemArg {
                offset,
                align: 2,
                memory_index: 0,
            }));
            target.store(function);
        }
        self.emit_regexp_parser_bitmap_range(
            compiler,
            mode,
            compiler.property_bitmap,
            first,
            last,
            function,
        );
        self.emit_regexp_scratch_increment(index, 1, function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        // v complements a case-closed operand. u complements raw membership;
        // the containing union is case-closed only when its ranges are emitted.
        mode.emit_is_unicode_sets(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_parser_fold_bitmap_operand(
            compiler,
            mode,
            modifiers,
            compiler.property_bitmap,
            function,
        );
        function.instruction(&Instruction::End);
        eq(function, operand.complement, 1);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_regexp_parser_complement_bitmap(mode, compiler.property_bitmap, function);
        function.instruction(&Instruction::End);
        self.emit_regexp_parser_union_bitmap(mode, compiler.property_bitmap, destination, function);
        for local in [last, first, address, index, count, pointer] {
            self.runtime_schema().release_i64_local(local, function);
        }
    }
}
