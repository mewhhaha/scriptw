//! Six global codecs own one immutable UTF-16 input and a whole completion.
use super::super::*;
use crate::gc_types::*;

// The scope is private to the complete native family. Both directions consume
// it exhaustively; there is no public raw codec or string-payload adapter.
enum UriCodecKind {
    Uri,
    Component,
}
enum UriBuiltin {
    Escape,
    Unescape,
    Encode(UriCodecKind),
    Decode(UriCodecKind),
}

/// Count and fill traverse the same immutable input. The first pass validates
/// every malformed URI route before the exact-length result is allocated.
struct UriSink<'a> {
    position: I64Local,
    construction: Option<&'a StringConstruction>,
}
impl UriSink<'_> {
    fn unit(&self, unit: I64Local, s: &RuntimeSchema, f: &mut Function) {
        if let Some(construction) = self.construction {
            let index = s.reserve_i32_local(f);
            let value = s.reserve_i32_local(f);
            self.position.load(f);
            f.instruction(&Instruction::I32WrapI64);
            index.store(f);
            unit.load(f);
            f.instruction(&Instruction::I32WrapI64);
            value.store(f);
            construction.write(index, value, s, f);
            s.release_i32_local(value, f);
            s.release_i32_local(index, f);
        }
        self.position.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        self.position.store(f);
    }
    fn literal(&self, text: &str, s: &RuntimeSchema, f: &mut Function) {
        let unit = s.reserve_i64_local(f);
        for c in text.encode_utf16() {
            f.instruction(&Instruction::I64Const(i64::from(c)));
            unit.store(f);
            self.unit(unit, s, f);
        }
        s.release_i64_local(unit, f);
    }
    fn hex(&self, value: I64Local, digits: u32, s: &RuntimeSchema, f: &mut Function) {
        let digit = s.reserve_i64_local(f);
        for shift in (0..digits).rev().map(|n| n * 4) {
            value.load(f);
            f.instruction(&Instruction::I64Const(i64::from(shift)));
            f.instruction(&Instruction::I64ShrU);
            f.instruction(&Instruction::I64Const(15));
            f.instruction(&Instruction::I64And);
            digit.store(f);
            digit.load(f);
            f.instruction(&Instruction::I64Const(10));
            f.instruction(&Instruction::I64LtU);
            f.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
            digit.load(f);
            f.instruction(&Instruction::I64Const(48));
            f.instruction(&Instruction::I64Add);
            f.instruction(&Instruction::Else);
            digit.load(f);
            f.instruction(&Instruction::I64Const(55));
            f.instruction(&Instruction::I64Add);
            f.instruction(&Instruction::End);
            digit.store(f);
            self.unit(digit, s, f);
        }
        s.release_i64_local(digit, f);
    }
    fn octet(&self, octet: I64Local, s: &RuntimeSchema, f: &mut Function) {
        self.literal("%", s, f);
        self.hex(octet, 2, s, f);
    }
    fn scalar(&self, scalar: I64Local, s: &RuntimeSchema, f: &mut Function) {
        let unit = s.reserve_i64_local(f);
        scalar.load(f);
        f.instruction(&Instruction::I64Const(0xffff));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.unit(scalar, s, f);
        f.instruction(&Instruction::Else);
        scalar.load(f);
        f.instruction(&Instruction::I64Const(0x10000));
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64Const(10));
        f.instruction(&Instruction::I64ShrU);
        f.instruction(&Instruction::I64Const(0xd800));
        f.instruction(&Instruction::I64Add);
        unit.store(f);
        self.unit(unit, s, f);
        scalar.load(f);
        f.instruction(&Instruction::I64Const(0x3ff));
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Const(0xdc00));
        f.instruction(&Instruction::I64Add);
        unit.store(f);
        self.unit(unit, s, f);
        f.instruction(&Instruction::End);
        s.release_i64_local(unit, f);
    }
}

impl FunctionBuilder<'_> {
    fn emit_uri_error_if(
        &mut self,
        message: RuntimeErrorMessage,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_uri_error(message, output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_uri_unit(
        &self,
        units: &GcLocal<CodeUnitArray>,
        index: I64Local,
        out: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let i = s.reserve_i32_local(f);
        let u = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        i.store(f);
        s.array_type::<CodeUnitArray>()
            .read(units, i, s, f)
            .store(u, f);
        u.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        out.store(f);
        s.release_i32_local(u, f);
        s.release_i32_local(i, f);
    }
    fn emit_uri_membership(
        &self,
        unit: I64Local,
        punctuation: &[u8],
        ascii_word: bool,
        f: &mut Function,
    ) {
        f.instruction(&Instruction::I32Const(0));
        if ascii_word {
            for (lo, hi) in [(b'A', b'Z'), (b'a', b'z'), (b'0', b'9')] {
                unit.load(f);
                f.instruction(&Instruction::I64Const(i64::from(lo)));
                f.instruction(&Instruction::I64GeU);
                unit.load(f);
                f.instruction(&Instruction::I64Const(i64::from(hi)));
                f.instruction(&Instruction::I64LeU);
                f.instruction(&Instruction::I32And);
                f.instruction(&Instruction::I32Or);
            }
            unit.load(f);
            f.instruction(&Instruction::I64Const(95));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::I32Or);
        }
        for c in punctuation {
            unit.load(f);
            f.instruction(&Instruction::I64Const(i64::from(*c)));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::I32Or);
        }
    }
    fn emit_uri_hex_digit(&self, unit: I64Local, digit: I64Local, f: &mut Function) {
        f.instruction(&Instruction::I64Const(-1));
        digit.store(f);
        for (lo, hi, bias) in [(48, 57, 48), (65, 70, 55), (97, 102, 87)] {
            unit.load(f);
            f.instruction(&Instruction::I64Const(lo));
            f.instruction(&Instruction::I64GeU);
            unit.load(f);
            f.instruction(&Instruction::I64Const(hi));
            f.instruction(&Instruction::I64LeU);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::If(BlockType::Empty));
            unit.load(f);
            f.instruction(&Instruction::I64Const(bias));
            f.instruction(&Instruction::I64Sub);
            digit.store(f);
            f.instruction(&Instruction::End);
        }
    }
    fn emit_uri_parse_octet(
        &mut self,
        units: &GcLocal<CodeUnitArray>,
        length: I64Local,
        index: I64Local,
        octet: I64Local,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let next = s.reserve_i64_local(f);
        let unit = s.reserve_i64_local(f);
        let first = s.reserve_i64_local(f);
        let last = s.reserve_i64_local(f);
        index.load(f);
        f.instruction(&Instruction::I64Const(3));
        f.instruction(&Instruction::I64Add);
        length.load(f);
        f.instruction(&Instruction::I64GtU);
        self.emit_uri_error_if(
            RuntimeErrorMessage::URI_CONTAINS_AN_INCOMPLETE_PERCENT_ESCAPE,
            output,
            exit,
            f,
        )?;
        for (delta, digit) in [(1, first), (2, last)] {
            index.load(f);
            f.instruction(&Instruction::I64Const(delta));
            f.instruction(&Instruction::I64Add);
            next.store(f);
            self.emit_uri_unit(units, next, unit, f);
            self.emit_uri_hex_digit(unit, digit, f);
        }
        first.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        last.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        f.instruction(&Instruction::I32Or);
        self.emit_uri_error_if(
            RuntimeErrorMessage::URI_PERCENT_ESCAPE_CONTAINS_A_NON_HEX_DIGIT,
            output,
            exit,
            f,
        )?;
        first.load(f);
        f.instruction(&Instruction::I64Const(4));
        f.instruction(&Instruction::I64Shl);
        last.load(f);
        f.instruction(&Instruction::I64Or);
        octet.store(f);
        for local in [last, first, unit, next] {
            s.release_i64_local(local, f);
        }
        Ok(())
    }

    fn emit_uri_encode_units(
        &mut self,
        kind: &UriCodecKind,
        units: &GcLocal<CodeUnitArray>,
        length: I64Local,
        index: I64Local,
        unit: I64Local,
        sink: &UriSink<'_>,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let next = s.reserve_i64_local(f);
        let low = s.reserve_i64_local(f);
        let cp = s.reserve_i64_local(f);
        let octet = s.reserve_i64_local(f);
        let punctuation = match kind {
            UriCodecKind::Uri => b"-.!~*'();/?:@&=+$,#".as_slice(),
            UriCodecKind::Component => b"-.!~*'()".as_slice(),
        };
        self.emit_uri_membership(unit, punctuation, true, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        sink.unit(unit, s, f);
        self.emit_increment_local(index, 1, f);
        f.instruction(&Instruction::Else);
        unit.load(f);
        cp.store(f);
        unit.load(f);
        f.instruction(&Instruction::I64Const(0xd800));
        f.instruction(&Instruction::I64GeU);
        unit.load(f);
        f.instruction(&Instruction::I64Const(0xdbff));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::If(BlockType::Empty));
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        next.store(f);
        next.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_uri_error_if(
            RuntimeErrorMessage::URI_CONTAINS_A_TRAILING_HIGH_SURROGATE,
            output,
            exit,
            f,
        )?;
        self.emit_uri_unit(units, next, low, f);
        low.load(f);
        f.instruction(&Instruction::I64Const(0xdc00));
        f.instruction(&Instruction::I64LtU);
        low.load(f);
        f.instruction(&Instruction::I64Const(0xdfff));
        f.instruction(&Instruction::I64GtU);
        f.instruction(&Instruction::I32Or);
        self.emit_uri_error_if(
            RuntimeErrorMessage::URI_CONTAINS_A_HIGH_SURROGATE_WITHOUT_A_FOLLOWING_LOW_SURROGATE,
            output,
            exit,
            f,
        )?;
        unit.load(f);
        f.instruction(&Instruction::I64Const(0xd800));
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64Const(10));
        f.instruction(&Instruction::I64Shl);
        low.load(f);
        f.instruction(&Instruction::I64Const(0xdc00));
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64Const(0x10000));
        f.instruction(&Instruction::I64Add);
        cp.store(f);
        self.emit_increment_local(index, 1, f);
        f.instruction(&Instruction::Else);
        unit.load(f);
        f.instruction(&Instruction::I64Const(0xdc00));
        f.instruction(&Instruction::I64GeU);
        unit.load(f);
        f.instruction(&Instruction::I64Const(0xdfff));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        self.emit_uri_error_if(
            RuntimeErrorMessage::URI_CONTAINS_AN_UNPAIRED_LOW_SURROGATE,
            output,
            exit,
            f,
        )?;
        f.instruction(&Instruction::End);
        // A valid scalar selects one of the four exact UTF-8 widths.
        for (limit, bytes, leading) in [(0x80, 1, 0), (0x800, 2, 0xc0), (0x10000, 3, 0xe0)] {
            cp.load(f);
            f.instruction(&Instruction::I64Const(limit));
            f.instruction(&Instruction::I64LtU);
            f.instruction(&Instruction::If(BlockType::Empty));
            self.emit_uri_scalar_octets(cp, bytes, leading, octet, sink, f);
            f.instruction(&Instruction::Else);
        }
        self.emit_uri_scalar_octets(cp, 4, 0xf0, octet, sink, f);
        for _ in 0..3 {
            f.instruction(&Instruction::End);
        }
        self.emit_increment_local(index, 1, f);
        f.instruction(&Instruction::End);
        for local in [octet, cp, low, next] {
            s.release_i64_local(local, f);
        }
        Ok(())
    }
    fn emit_uri_scalar_octets(
        &self,
        cp: I64Local,
        width: u32,
        leading: i64,
        octet: I64Local,
        sink: &UriSink<'_>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        cp.load(f);
        if width > 1 {
            f.instruction(&Instruction::I64Const(i64::from(6 * (width - 1))));
            f.instruction(&Instruction::I64ShrU);
            f.instruction(&Instruction::I64Const(leading));
            f.instruction(&Instruction::I64Or);
        }
        octet.store(f);
        sink.octet(octet, s, f);
        for remainder in (0..width.saturating_sub(1)).rev() {
            cp.load(f);
            f.instruction(&Instruction::I64Const(i64::from(6 * remainder)));
            f.instruction(&Instruction::I64ShrU);
            f.instruction(&Instruction::I64Const(0x3f));
            f.instruction(&Instruction::I64And);
            f.instruction(&Instruction::I64Const(0x80));
            f.instruction(&Instruction::I64Or);
            octet.store(f);
            sink.octet(octet, s, f);
        }
    }

    fn emit_uri_decode_units(
        &mut self,
        kind: &UriCodecKind,
        units: &GcLocal<CodeUnitArray>,
        length: I64Local,
        index: I64Local,
        unit: I64Local,
        sink: &UriSink<'_>,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let first = s.reserve_i64_local(f);
        let cp = s.reserve_i64_local(f);
        let width = s.reserve_i64_local(f);
        let minimum = s.reserve_i64_local(f);
        let part = s.reserve_i64_local(f);
        let j = s.reserve_i64_local(f);
        let raw_index = s.reserve_i64_local(f);
        unit.load(f);
        f.instruction(&Instruction::I64Const(37));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.emit_uri_parse_octet(units, length, index, first, output, exit, f)?;
        first.load(f);
        f.instruction(&Instruction::I64Const(0x80));
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        match kind {
            UriCodecKind::Uri => {
                self.emit_uri_membership(first, b";/?:@&=+$,#", false, f);
                f.instruction(&Instruction::If(BlockType::Empty));
                for delta in 0..3 {
                    index.load(f);
                    f.instruction(&Instruction::I64Const(delta));
                    f.instruction(&Instruction::I64Add);
                    raw_index.store(f);
                    self.emit_uri_unit(units, raw_index, part, f);
                    sink.unit(part, s, f);
                }
                f.instruction(&Instruction::Else);
                sink.unit(first, s, f);
                f.instruction(&Instruction::End);
            }
            UriCodecKind::Component => sink.unit(first, s, f),
        }
        self.emit_increment_local(index, 3, f);
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I64Const(0));
        width.store(f);
        for (lo, hi, n, mask, min) in [
            (0xc2, 0xdf, 2, 0x1f, 0x80),
            (0xe0, 0xef, 3, 0x0f, 0x800),
            (0xf0, 0xf4, 4, 0x07, 0x10000),
        ] {
            first.load(f);
            f.instruction(&Instruction::I64Const(lo));
            f.instruction(&Instruction::I64GeU);
            first.load(f);
            f.instruction(&Instruction::I64Const(hi));
            f.instruction(&Instruction::I64LeU);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::If(BlockType::Empty));
            f.instruction(&Instruction::I64Const(n));
            width.store(f);
            f.instruction(&Instruction::I64Const(min));
            minimum.store(f);
            first.load(f);
            f.instruction(&Instruction::I64Const(mask));
            f.instruction(&Instruction::I64And);
            cp.store(f);
            f.instruction(&Instruction::End);
        }
        width.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_uri_error_if(
            RuntimeErrorMessage::URI_PERCENT_ENCODING_STARTS_WITH_AN_INVALID_UTF_8_BYTE,
            output,
            exit,
            f,
        )?;
        self.emit_increment_local(index, 3, f);
        f.instruction(&Instruction::I64Const(1));
        j.store(f);
        let complete = self.open_frame(ControlFrameKind::Block, f);
        let continuation = self.open_frame(ControlFrameKind::Loop, f);
        j.load(f);
        width.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(complete, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_uri_error_if(
            RuntimeErrorMessage::URI_CONTAINS_AN_INCOMPLETE_PERCENT_ESCAPE,
            output,
            exit,
            f,
        )?;
        self.emit_uri_unit(units, index, part, f);
        part.load(f);
        f.instruction(&Instruction::I64Const(37));
        f.instruction(&Instruction::I64Ne);
        self.emit_uri_error_if(
            RuntimeErrorMessage::URI_UTF_8_CONTINUATION_BYTE_IS_NOT_PERCENT_ESCAPED,
            output,
            exit,
            f,
        )?;
        self.emit_uri_parse_octet(units, length, index, part, output, exit, f)?;
        part.load(f);
        f.instruction(&Instruction::I64Const(0x80));
        f.instruction(&Instruction::I64LtU);
        part.load(f);
        f.instruction(&Instruction::I64Const(0xbf));
        f.instruction(&Instruction::I64GtU);
        f.instruction(&Instruction::I32Or);
        self.emit_uri_error_if(
            RuntimeErrorMessage::URI_PERCENT_ENCODING_CONTAINS_AN_INVALID_UTF_8_CONTINUATION_BYTE,
            output,
            exit,
            f,
        )?;
        cp.load(f);
        f.instruction(&Instruction::I64Const(6));
        f.instruction(&Instruction::I64Shl);
        part.load(f);
        f.instruction(&Instruction::I64Const(0x3f));
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Or);
        cp.store(f);
        self.emit_increment_local(index, 3, f);
        self.emit_increment_local(j, 1, f);
        self.emit_branch_to_target(continuation, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        cp.load(f);
        minimum.load(f);
        f.instruction(&Instruction::I64LtU);
        cp.load(f);
        f.instruction(&Instruction::I64Const(0x10ffff));
        f.instruction(&Instruction::I64GtU);
        f.instruction(&Instruction::I32Or);
        cp.load(f);
        f.instruction(&Instruction::I64Const(0xd800));
        f.instruction(&Instruction::I64GeU);
        cp.load(f);
        f.instruction(&Instruction::I64Const(0xdfff));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Or);
        self.emit_uri_error_if(
            RuntimeErrorMessage::URI_PERCENT_ENCODING_IS_NOT_A_SHORTEST_FORM_UNICODE_SCALAR,
            output,
            exit,
            f,
        )?;
        sink.scalar(cp, s, f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        sink.unit(unit, s, f);
        self.emit_increment_local(index, 1, f);
        f.instruction(&Instruction::End);
        for local in [raw_index, j, part, minimum, width, cp, first] {
            s.release_i64_local(local, f);
        }
        Ok(())
    }

    fn emit_uri_annexb_unescape_units(
        &mut self,
        units: &GcLocal<CodeUnitArray>,
        length: I64Local,
        index: I64Local,
        unit: I64Local,
        sink: &UriSink<'_>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let start = s.reserve_i64_local(f);
        let digits = s.reserve_i64_local(f);
        let advance = s.reserve_i64_local(f);
        let j = s.reserve_i64_local(f);
        let position = s.reserve_i64_local(f);
        let value = s.reserve_i64_local(f);
        let digit = s.reserve_i64_local(f);
        let raw = s.reserve_i64_local(f);
        let valid = s.reserve_i32_local(f);
        f.instruction(&Instruction::I64Const(0));
        advance.store(f);
        unit.load(f);
        f.instruction(&Instruction::I64Const(37));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::If(BlockType::Empty));
        index.load(f);
        f.instruction(&Instruction::I64Const(3));
        f.instruction(&Instruction::I64Add);
        length.load(f);
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::If(BlockType::Empty));
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        start.store(f);
        f.instruction(&Instruction::I64Const(2));
        digits.store(f);
        self.emit_uri_unit(units, start, raw, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(6));
        f.instruction(&Instruction::I64Add);
        length.load(f);
        f.instruction(&Instruction::I64LeU);
        raw.load(f);
        f.instruction(&Instruction::I64Const(117));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.emit_increment_local(start, 1, f);
        f.instruction(&Instruction::I64Const(4));
        digits.store(f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I64Const(0));
        j.store(f);
        f.instruction(&Instruction::I64Const(0));
        value.store(f);
        f.instruction(&Instruction::I32Const(1));
        valid.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        j.load(f);
        digits.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        start.load(f);
        j.load(f);
        f.instruction(&Instruction::I64Add);
        position.store(f);
        self.emit_uri_unit(units, position, raw, f);
        self.emit_uri_hex_digit(raw, digit, f);
        valid.load(f);
        digit.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64GeS);
        f.instruction(&Instruction::I32And);
        valid.store(f);
        value.load(f);
        f.instruction(&Instruction::I64Const(4));
        f.instruction(&Instruction::I64Shl);
        digit.load(f);
        f.instruction(&Instruction::I64Or);
        value.store(f);
        self.emit_increment_local(j, 1, f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        valid.load(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        value.load(f);
        unit.store(f);
        start.load(f);
        digits.load(f);
        f.instruction(&Instruction::I64Add);
        index.load(f);
        f.instruction(&Instruction::I64Sub);
        advance.store(f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        sink.unit(unit, s, f);
        advance.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.emit_increment_local(index, 1, f);
        f.instruction(&Instruction::Else);
        index.load(f);
        advance.load(f);
        f.instruction(&Instruction::I64Add);
        index.store(f);
        f.instruction(&Instruction::End);
        s.release_i32_local(valid, f);
        for local in [raw, digit, value, position, j, advance, digits, start] {
            s.release_i64_local(local, f);
        }
    }

    fn emit_uri_traverse(
        &mut self,
        builtin: &UriBuiltin,
        units: &GcLocal<CodeUnitArray>,
        length: I64Local,
        sink: &UriSink<'_>,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i64_local(f);
        let unit = s.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let traversal = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        self.emit_uri_unit(units, index, unit, f);
        match builtin {
            UriBuiltin::Escape => {
                self.emit_uri_membership(unit, b"@*+-./", true, f);
                f.instruction(&Instruction::If(BlockType::Empty));
                sink.unit(unit, s, f);
                f.instruction(&Instruction::Else);
                unit.load(f);
                f.instruction(&Instruction::I64Const(256));
                f.instruction(&Instruction::I64LtU);
                f.instruction(&Instruction::If(BlockType::Empty));
                sink.literal("%", s, f);
                sink.hex(unit, 2, s, f);
                f.instruction(&Instruction::Else);
                sink.literal("%u", s, f);
                sink.hex(unit, 4, s, f);
                f.instruction(&Instruction::End);
                f.instruction(&Instruction::End);
                self.emit_increment_local(index, 1, f);
            }
            UriBuiltin::Unescape => {
                self.emit_uri_annexb_unescape_units(units, length, index, unit, sink, f)
            }
            UriBuiltin::Encode(kind) => {
                self.emit_uri_encode_units(kind, units, length, index, unit, sink, output, exit, f)?
            }
            UriBuiltin::Decode(kind) => {
                self.emit_uri_decode_units(kind, units, length, index, unit, sink, output, exit, f)?
            }
        }
        self.emit_branch_to_target(traversal, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(unit, f);
        s.release_i64_local(index, f);
        Ok(())
    }

    fn emit_uri_builtin(&mut self, builtin: UriBuiltin, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        self.emit_builtin_arg_to_value(0, &argument, f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_value_to_string_payload(&argument, &pending, f)?;
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        output.copy_from(&pending, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let input = s
            .reserve_gc_local::<StringValue, NonNullable>(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        let units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(&input, s, f)
                .reference(),
            f,
        );
        let length = s.reserve_i64_local(f);
        s.array_type::<CodeUnitArray>().length(&units, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        // Decode transforms only '%' escape sequences and can throw only on
        // them; a '%'-free string returns the original immutable StringValue.
        if matches!(builtin, UriBuiltin::Decode(_)) {
            let start = s.reserve_i32_local(f);
            let end = s.reserve_i32_local(f);
            let percent = s.reserve_i32_local(f);
            start.set_constant(0, f);
            length.load(f);
            f.instruction(&Instruction::I32WrapI64);
            end.store(f);
            percent.set_constant(0x25, f);
            s.call_helper(
                crate::runtime_helpers::CodeUnitFindArguments::new(&units, start, end, percent),
                self.runtime_helper_base()?,
                f,
            )
            .store(start, f);
            start.load(f);
            end.load(f);
            f.instruction(&Instruction::I32GeU);
            self.open_frame(ControlFrameKind::If, f);
            output.value().set_reference(&input, s, f);
            output.set_kind(CompletionKind::Normal, f);
            self.emit_branch_to_target(exit, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            s.release_i32_local(percent, f);
            s.release_i32_local(end, f);
            s.release_i32_local(start, f);
        }
        let count = s.reserve_i64_local(f);
        let extent = s.reserve_i32_local(f);
        f.instruction(&Instruction::I64Const(0));
        count.store(f);
        self.emit_uri_traverse(
            &builtin,
            &units,
            length,
            &UriSink {
                position: count,
                construction: None,
            },
            &output,
            exit,
            f,
        )?;
        count.load(f);
        f.instruction(&Instruction::I64Const(i32::MAX as i64));
        f.instruction(&Instruction::I64GtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        count.load(f);
        f.instruction(&Instruction::I32WrapI64);
        extent.store(f);
        let construction = StringConstruction::allocate(s, s.reserve_gc_local(f), extent, f);
        f.instruction(&Instruction::I64Const(0));
        count.store(f);
        self.emit_uri_traverse(
            &builtin,
            &units,
            length,
            &UriSink {
                position: count,
                construction: Some(&construction),
            },
            &output,
            exit,
            f,
        )?;
        let result = s
            .reserve_gc_local(f)
            .initialize(construction.publish(s, f), f);
        output.value().set_reference(&result, s, f);
        output.set_kind(CompletionKind::Normal, f);
        result.clear(f);
        s.release_i32_local(extent, f);
        s.release_i64_local(count, f);
        s.release_i64_local(length, f);
        units.clear(f);
        input.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        argument.clear(f);
        Ok(())
    }
    pub(super) fn emit_escape_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_uri_builtin(UriBuiltin::Escape, f)
    }
    pub(super) fn emit_unescape_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_uri_builtin(UriBuiltin::Unescape, f)
    }
    pub(super) fn emit_encode_uri_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_uri_builtin(UriBuiltin::Encode(UriCodecKind::Uri), f)
    }
    pub(super) fn emit_encode_uri_component_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_uri_builtin(UriBuiltin::Encode(UriCodecKind::Component), f)
    }
    pub(super) fn emit_decode_uri_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_uri_builtin(UriBuiltin::Decode(UriCodecKind::Uri), f)
    }
    pub(super) fn emit_decode_uri_component_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_uri_builtin(UriBuiltin::Decode(UriCodecKind::Component), f)
    }
}
