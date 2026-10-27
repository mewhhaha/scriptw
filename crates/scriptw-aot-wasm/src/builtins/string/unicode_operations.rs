//! Unicode algorithms consume immutable GC UTF-16; workspace bytes hold only scalars.
use super::*;
use icu_properties::{props, CodePointSetData};

#[derive(Clone, Copy)]
pub(in crate::builtins) enum StringCaseOperation {
    Lower,
    Upper,
    LocaleLower,
    LocaleUpper,
}
impl StringCaseOperation {
    const fn upper(self) -> bool {
        matches!(self, Self::Upper | Self::LocaleUpper)
    }
    const fn localized(self) -> bool {
        matches!(self, Self::LocaleLower | Self::LocaleUpper)
    }
}
#[derive(Clone, Copy)]
pub(in crate::builtins) enum StringHtmlOperation {
    Anchor,
    Big,
    Blink,
    Bold,
    Fixed,
    Fontcolor,
    Fontsize,
    Italics,
    Link,
    Small,
    Strike,
    Sub,
    Sup,
}
impl StringHtmlOperation {
    const fn tag(self) -> &'static str {
        match self {
            Self::Anchor => "a",
            Self::Big => "big",
            Self::Blink => "blink",
            Self::Bold => "b",
            Self::Fixed => "tt",
            Self::Fontcolor | Self::Fontsize => "font",
            Self::Italics => "i",
            Self::Link => "a",
            Self::Small => "small",
            Self::Strike => "strike",
            Self::Sub => "sub",
            Self::Sup => "sup",
        }
    }
    const fn attribute(self) -> Option<&'static str> {
        match self {
            Self::Anchor => Some("name"),
            Self::Fontcolor => Some("color"),
            Self::Fontsize => Some("size"),
            Self::Link => Some("href"),
            Self::Big
            | Self::Blink
            | Self::Bold
            | Self::Fixed
            | Self::Italics
            | Self::Small
            | Self::Strike
            | Self::Sub
            | Self::Sup => None,
        }
    }
}

/// The same pure traversal first counts and then fills one exact-length String.
/// Only the completed String escapes; no mutable CodeUnitArray is published.
struct UnicodeSink<'a> {
    position: I64Local,
    construction: Option<&'a StringConstruction>,
}
impl UnicodeSink<'_> {
    fn unit(&self, unit: I32Local, s: &RuntimeSchema, f: &mut Function) {
        if let Some(construction) = self.construction {
            let index = s.reserve_i32_local(f);
            self.position.load(f);
            f.instruction(&Instruction::I32WrapI64);
            index.store(f);
            construction.write(index, unit, s, f);
            s.release_i32_local(index, f);
        }
        self.position.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        self.position.store(f);
    }
    fn literal(&self, text: &str, s: &RuntimeSchema, f: &mut Function) {
        let unit = s.reserve_i32_local(f);
        for c in text.encode_utf16() {
            f.instruction(&Instruction::I32Const(i32::from(c)));
            unit.store(f);
            self.unit(unit, s, f);
        }
        s.release_i32_local(unit, f);
    }
    fn scalar(&self, cp: I64Local, s: &RuntimeSchema, f: &mut Function) {
        let unit = s.reserve_i32_local(f);
        cp.load(f);
        f.instruction(&Instruction::I64Const(0xffff));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::If(BlockType::Empty));
        cp.load(f);
        f.instruction(&Instruction::I32WrapI64);
        unit.store(f);
        self.unit(unit, s, f);
        f.instruction(&Instruction::Else);
        cp.load(f);
        f.instruction(&Instruction::I64Const(0x10000));
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64Const(10));
        f.instruction(&Instruction::I64ShrU);
        f.instruction(&Instruction::I64Const(0xd800));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I32WrapI64);
        unit.store(f);
        self.unit(unit, s, f);
        cp.load(f);
        f.instruction(&Instruction::I64Const(0x3ff));
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Const(0xdc00));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I32WrapI64);
        unit.store(f);
        self.unit(unit, s, f);
        f.instruction(&Instruction::End);
        s.release_i32_local(unit, f);
    }
    fn hex(&self, cp: I64Local, digits: u32, s: &RuntimeSchema, f: &mut Function) {
        self.literal(if digits == 2 { "\\x" } else { "\\u" }, s, f);
        let unit = s.reserve_i32_local(f);
        for shift in (0..digits).rev().map(|n| n * 4) {
            cp.load(f);
            f.instruction(&Instruction::I64Const(i64::from(shift)));
            f.instruction(&Instruction::I64ShrU);
            f.instruction(&Instruction::I64Const(15));
            f.instruction(&Instruction::I64And);
            f.instruction(&Instruction::I32WrapI64);
            unit.store(f);
            unit.load(f);
            f.instruction(&Instruction::I32Const(10));
            f.instruction(&Instruction::I32LtU);
            f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
            unit.load(f);
            f.instruction(&Instruction::I32Const(48));
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::Else);
            unit.load(f);
            f.instruction(&Instruction::I32Const(87));
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::End);
            unit.store(f);
            self.unit(unit, s, f);
        }
        s.release_i32_local(unit, f);
    }
}

impl FunctionBuilder<'_> {
    fn emit_unicode_publish_string(
        &mut self,
        f: &mut Function,
        mut traverse: impl FnMut(&mut Self, &UnicodeSink<'_>, &mut Function) -> Result<(), EmitError>,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let s = self.runtime_schema();
        let count = s.reserve_i64_local(f);
        let length = s.reserve_i32_local(f);
        f.instruction(&Instruction::I64Const(0));
        count.store(f);
        traverse(
            self,
            &UnicodeSink {
                position: count,
                construction: None,
            },
            f,
        )?;
        // The selected GC runtime has a signed I32 array extent. Fail before
        // narrowing, rather than truncating a counted output or workspace.
        count.load(f);
        f.instruction(&Instruction::I64Const(i32::MAX as i64));
        f.instruction(&Instruction::I64GtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        count.load(f);
        f.instruction(&Instruction::I32WrapI64);
        length.store(f);
        let construction = StringConstruction::allocate(s, s.reserve_gc_local(f), length, f);
        f.instruction(&Instruction::I64Const(0));
        count.store(f);
        traverse(
            self,
            &UnicodeSink {
                position: count,
                construction: Some(&construction),
            },
            f,
        )?;
        let result = s
            .reserve_gc_local(f)
            .initialize(construction.publish(s, f), f);
        s.release_i32_local(length, f);
        s.release_i64_local(count, f);
        Ok(result)
    }
    fn emit_unicode_units(
        &self,
        input: &GcLocal<StringValue>,
        f: &mut Function,
    ) -> GcLocal<CodeUnitArray> {
        let s = self.runtime_schema();
        s.reserve_gc_local(f).initialize(
            s.struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(input, s, f)
                .reference(),
            f,
        )
    }
    fn emit_unicode_unit(
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
    fn emit_unicode_decode(
        &self,
        units: &GcLocal<CodeUnitArray>,
        length: I64Local,
        index: I64Local,
        cp: I64Local,
        width: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let next = s.reserve_i64_local(f);
        let low = s.reserve_i64_local(f);
        self.emit_unicode_unit(units, index, cp, f);
        f.instruction(&Instruction::I64Const(1));
        width.store(f);
        cp.load(f);
        f.instruction(&Instruction::I64Const(0xd800));
        f.instruction(&Instruction::I64GeU);
        cp.load(f);
        f.instruction(&Instruction::I64Const(0xdbff));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        next.store(f);
        next.load(f);
        length.load(f);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.emit_unicode_unit(units, next, low, f);
        low.load(f);
        f.instruction(&Instruction::I64Const(0xdc00));
        f.instruction(&Instruction::I64GeU);
        low.load(f);
        f.instruction(&Instruction::I64Const(0xdfff));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::If(BlockType::Empty));
        cp.load(f);
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
        f.instruction(&Instruction::I64Const(2));
        width.store(f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        s.release_i64_local(low, f);
        s.release_i64_local(next, f);
    }
    fn emit_unicode_increment(&self, index: I64Local, width: I64Local, f: &mut Function) {
        index.load(f);
        width.load(f);
        f.instruction(&Instruction::I64Add);
        index.store(f);
    }
    fn emit_unicode_copy(
        &self,
        units: &GcLocal<CodeUnitArray>,
        sink: &UnicodeSink<'_>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let length = s.reserve_i32_local(f);
        let index = s.reserve_i32_local(f);
        let unit = s.reserve_i32_local(f);
        s.array_type::<CodeUnitArray>().length(units, s, f);
        length.store(f);
        f.instruction(&Instruction::I32Const(0));
        index.store(f);
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::BrIf(1));
        s.array_type::<CodeUnitArray>()
            .read(units, index, s, f)
            .store(unit, f);
        sink.unit(unit, s, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        f.instruction(&Instruction::Br(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        s.release_i32_local(unit, f);
        s.release_i32_local(index, f);
        s.release_i32_local(length, f);
    }

    pub(in crate::builtins) fn emit_string_html_builtin(
        &mut self,
        operation: StringHtmlOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f, |b, input, output, exit, f| {
            let s = b.runtime_schema();
            let input_units = b.emit_unicode_units(input, f);
            let argument = s.reserve_value_local(f);
            let pending = s.reserve_completion(f);
            let attribute = s
                .reserve_gc_local::<StringValue, Nullable>(f)
                .initialize_null(s, f);
            if operation.attribute().is_some() {
                b.emit_builtin_arg_to_value(0, &argument, f);
                b.emit_value_to_string_payload(&argument, &pending, f)?;
                b.emit_native_string_abrupt_exit(&pending, output, exit, f);
                attribute.replace(
                    pending
                        .value()
                        .cast_reference::<StringValue>(s, f)
                        .nullable(),
                    f,
                );
            }
            let result = b.emit_unicode_publish_string(f, |b, sink, f| {
                sink.literal("<", s, f);
                sink.literal(operation.tag(), s, f);
                if let Some(name) = operation.attribute() {
                    sink.literal(" ", s, f);
                    sink.literal(name, s, f);
                    sink.literal("=\"", s, f);
                    let text = s
                        .reserve_gc_local(f)
                        .initialize(attribute.load(s, f).require_non_null(f), f);
                    let units = b.emit_unicode_units(&text, f);
                    let count = s.reserve_i32_local(f);
                    let index = s.reserve_i32_local(f);
                    let unit = s.reserve_i32_local(f);
                    s.array_type::<CodeUnitArray>().length(&units, s, f);
                    count.store(f);
                    f.instruction(&Instruction::I32Const(0));
                    index.store(f);
                    f.instruction(&Instruction::Block(BlockType::Empty));
                    f.instruction(&Instruction::Loop(BlockType::Empty));
                    index.load(f);
                    count.load(f);
                    f.instruction(&Instruction::I32GeU);
                    f.instruction(&Instruction::BrIf(1));
                    s.array_type::<CodeUnitArray>()
                        .read(&units, index, s, f)
                        .store(unit, f);
                    unit.load(f);
                    f.instruction(&Instruction::I32Const(34));
                    f.instruction(&Instruction::I32Eq);
                    f.instruction(&Instruction::If(BlockType::Empty));
                    sink.literal("&quot;", s, f);
                    f.instruction(&Instruction::Else);
                    sink.unit(unit, s, f);
                    f.instruction(&Instruction::End);
                    index.load(f);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Add);
                    index.store(f);
                    f.instruction(&Instruction::Br(0));
                    f.instruction(&Instruction::End);
                    f.instruction(&Instruction::End);
                    s.release_i32_local(unit, f);
                    s.release_i32_local(index, f);
                    s.release_i32_local(count, f);
                    units.clear(f);
                    text.clear(f);
                    sink.literal("\"", s, f);
                }
                sink.literal(">", s, f);
                b.emit_unicode_copy(&input_units, sink, f);
                sink.literal("</", s, f);
                sink.literal(operation.tag(), s, f);
                sink.literal(">", s, f);
                Ok(())
            })?;
            b.emit_native_string_normal_reference(&result, output, f);
            result.clear(f);
            attribute.clear(f);
            pending.clear(f);
            argument.clear(f);
            input_units.clear(f);
            Ok(())
        })
    }

    pub(in crate::builtins) fn emit_regexp_escape_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let input = s.reserve_value_local(f);
        let output = s.reserve_completion(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        input.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Ne);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::REGEXP_ESCAPE_INPUT_MUST_BE_A_STRING,
            NativeErrorKind::TypeError,
            &output,
            exit,
            f,
        )?;
        let text = s
            .reserve_gc_local(f)
            .initialize(input.cast_reference::<StringValue>(s, f), f);
        let units = self.emit_unicode_units(&text, f);
        let length = s.reserve_i64_local(f);
        self.emit_native_gc_string_length(&text, length, f);
        let result = self.emit_unicode_publish_string(f, |b, sink, f| {
            b.emit_unicode_regexp_escape_pass(&units, length, sink, f)
        })?;
        self.emit_native_string_normal_reference(&result, &output, f);
        result.clear(f);
        s.release_i64_local(length, f);
        units.clear(f);
        text.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        input.clear(f);
        Ok(())
    }
    fn emit_unicode_regexp_escape_pass(
        &self,
        units: &GcLocal<CodeUnitArray>,
        length: I64Local,
        sink: &UnicodeSink<'_>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i64_local(f);
        let cp = s.reserve_i64_local(f);
        let width = s.reserve_i64_local(f);
        let control = s.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        f.instruction(&Instruction::BrIf(1));
        self.emit_unicode_decode(units, length, index, cp, width, f);
        f.instruction(&Instruction::I32Const(0));
        for (lo, hi) in [(48, 57), (65, 90), (97, 122)] {
            cp.load(f);
            f.instruction(&Instruction::I64Const(lo));
            f.instruction(&Instruction::I64GeU);
            cp.load(f);
            f.instruction(&Instruction::I64Const(hi));
            f.instruction(&Instruction::I64LeU);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::I32Or);
        }
        index.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::If(BlockType::Empty));
        sink.hex(cp, 2, s, f);
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I64Const(0));
        control.store(f);
        for (c, suffix) in [(9, b't'), (10, b'n'), (11, b'v'), (12, b'f'), (13, b'r')] {
            cp.load(f);
            f.instruction(&Instruction::I64Const(c));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::If(BlockType::Empty));
            f.instruction(&Instruction::I64Const(i64::from(suffix)));
            control.store(f);
            f.instruction(&Instruction::End);
        }
        control.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        sink.literal("\\", s, f);
        sink.scalar(control, s, f);
        f.instruction(&Instruction::Else);
        self.emit_unicode_member(cp, "^$\\.*+?()[]{}|/".chars().map(|c| c as i64), f);
        f.instruction(&Instruction::If(BlockType::Empty));
        sink.literal("\\", s, f);
        sink.scalar(cp, s, f);
        f.instruction(&Instruction::Else);
        self.emit_unicode_member(
            cp,
            ",-=<>#&!%:;@~'`\"".chars().map(|c| c as i64).chain([
                0x20, 0xa0, 0x1680, 0x2028, 0x2029, 0x202f, 0x205f, 0x3000, 0xfeff,
            ]),
            f,
        );
        cp.load(f);
        f.instruction(&Instruction::I64Const(0x2000));
        f.instruction(&Instruction::I64GeU);
        cp.load(f);
        f.instruction(&Instruction::I64Const(0x200a));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Or);
        cp.load(f);
        f.instruction(&Instruction::I64Const(0xd800));
        f.instruction(&Instruction::I64GeU);
        cp.load(f);
        f.instruction(&Instruction::I64Const(0xdfff));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::If(BlockType::Empty));
        cp.load(f);
        f.instruction(&Instruction::I64Const(0xff));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::If(BlockType::Empty));
        sink.hex(cp, 2, s, f);
        f.instruction(&Instruction::Else);
        sink.hex(cp, 4, s, f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        sink.scalar(cp, s, f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        self.emit_unicode_increment(index, width, f);
        f.instruction(&Instruction::Br(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        s.release_i64_local(control, f);
        s.release_i64_local(width, f);
        s.release_i64_local(cp, f);
        s.release_i64_local(index, f);
        Ok(())
    }
    fn emit_unicode_member(
        &self,
        cp: I64Local,
        points: impl IntoIterator<Item = i64>,
        f: &mut Function,
    ) {
        f.instruction(&Instruction::I32Const(0));
        for point in points {
            cp.load(f);
            f.instruction(&Instruction::I64Const(point));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::I32Or);
        }
    }
}

impl FunctionBuilder<'_> {
    fn emit_unicode_codepoint_table_lookup(
        &self,
        codepoint_local: I64Local,
        table_ptr: u32,
        mapping_count: u32,
        mapping_address_local: I64Local,
        mapping_len_local: I64Local,
        function: &mut Function,
    ) {
        let low_local = self.runtime_schema().reserve_i64_local(function);
        let high_local = self.runtime_schema().reserve_i64_local(function);
        let middle_local = self.runtime_schema().reserve_i64_local(function);
        let entry_address_local = self.runtime_schema().reserve_i64_local(function);
        let entry_codepoint_local = self.runtime_schema().reserve_i64_local(function);

        function.instruction(&Instruction::I64Const(0));
        mapping_address_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        mapping_len_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        low_local.store(function);
        function.instruction(&Instruction::I64Const(mapping_count as i64));
        high_local.store(function);

        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        low_local.load(function);
        high_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));

        low_local.load(function);
        high_local.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        middle_local.store(function);
        function.instruction(&Instruction::I64Const(table_ptr as i64));
        middle_local.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        entry_address_local.store(function);
        entry_address_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load(Self::memarg32(0)));
        function.instruction(&Instruction::I64ExtendI32U);
        entry_codepoint_local.store(function);

        codepoint_local.load(function);
        entry_codepoint_local.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        middle_local.load(function);
        high_local.store(function);
        function.instruction(&Instruction::Else);
        codepoint_local.load(function);
        entry_codepoint_local.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        middle_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        low_local.store(function);
        function.instruction(&Instruction::Else);
        entry_address_local.load(function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Add);
        mapping_address_local.store(function);
        entry_address_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load(Self::memarg32(4)));
        function.instruction(&Instruction::I64ExtendI32U);
        mapping_len_local.store(function);
        high_local.load(function);
        low_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        self.runtime_schema()
            .release_i64_local(entry_codepoint_local, function);
        self.runtime_schema()
            .release_i64_local(entry_address_local, function);
        self.runtime_schema()
            .release_i64_local(middle_local, function);
        self.runtime_schema()
            .release_i64_local(high_local, function);
        self.runtime_schema().release_i64_local(low_local, function);
    }
    fn emit_unicode_codepoint_in_range_table(
        &self,
        codepoint_local: I64Local,
        table_ptr: u32,
        range_count: u32,
        result_local: I64Local,
        function: &mut Function,
    ) {
        let low_local = self.runtime_schema().reserve_i64_local(function);
        let high_local = self.runtime_schema().reserve_i64_local(function);
        let middle_local = self.runtime_schema().reserve_i64_local(function);
        let entry_address_local = self.runtime_schema().reserve_i64_local(function);
        let range_boundary_local = self.runtime_schema().reserve_i64_local(function);

        function.instruction(&Instruction::I64Const(0));
        result_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        low_local.store(function);
        function.instruction(&Instruction::I64Const(range_count as i64));
        high_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        low_local.load(function);
        high_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        low_local.load(function);
        high_local.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        middle_local.store(function);
        function.instruction(&Instruction::I64Const(table_ptr as i64));
        middle_local.load(function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        entry_address_local.store(function);
        entry_address_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load(Self::memarg32(0)));
        function.instruction(&Instruction::I64ExtendI32U);
        range_boundary_local.store(function);
        codepoint_local.load(function);
        range_boundary_local.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        middle_local.load(function);
        high_local.store(function);
        function.instruction(&Instruction::Else);
        entry_address_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load(Self::memarg32(4)));
        function.instruction(&Instruction::I64ExtendI32U);
        range_boundary_local.store(function);
        codepoint_local.load(function);
        range_boundary_local.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        middle_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        low_local.store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        result_local.store(function);
        high_local.load(function);
        low_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        self.runtime_schema()
            .release_i64_local(range_boundary_local, function);
        self.runtime_schema()
            .release_i64_local(entry_address_local, function);
        self.runtime_schema()
            .release_i64_local(middle_local, function);
        self.runtime_schema()
            .release_i64_local(high_local, function);
        self.runtime_schema().release_i64_local(low_local, function);
    }
    fn emit_unicode_normalization_decomposition_lookup(
        &self,
        codepoint_local: I64Local,
        form: &StringNormalizationForm,
        sequence_ptr_local: I64Local,
        sequence_len_local: I64Local,
        function: &mut Function,
    ) {
        let (table_ptr, table_count) = form.decomposition_table(self.strings);
        let low_local = self.runtime_schema().reserve_i64_local(function);
        let high_local = self.runtime_schema().reserve_i64_local(function);
        let middle_local = self.runtime_schema().reserve_i64_local(function);
        let entry_local = self.runtime_schema().reserve_i64_local(function);
        let candidate_local = self.runtime_schema().reserve_i64_local(function);

        for local in [sequence_ptr_local, sequence_len_local, low_local] {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }
        function.instruction(&Instruction::I64Const(table_count as i64));
        high_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        low_local.load(function);
        high_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        low_local.load(function);
        high_local.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        middle_local.store(function);
        function.instruction(&Instruction::I64Const(table_ptr as i64));
        middle_local.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        entry_local.store(function);
        entry_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load(Self::memarg32(0)));
        function.instruction(&Instruction::I64ExtendI32U);
        candidate_local.store(function);
        codepoint_local.load(function);
        candidate_local.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        middle_local.load(function);
        high_local.store(function);
        function.instruction(&Instruction::Else);
        codepoint_local.load(function);
        candidate_local.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        middle_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        low_local.store(function);
        function.instruction(&Instruction::Else);
        self.emit_unicode_static_u32(entry_local, 4, sequence_ptr_local, function);
        sequence_ptr_local.load(function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        sequence_ptr_local.store(function);
        self.emit_unicode_static_u32(entry_local, 8, sequence_len_local, function);
        sequence_len_local.load(function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        sequence_len_local.store(function);
        high_local.load(function);
        low_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        self.runtime_schema()
            .release_i64_local(candidate_local, function);
        self.runtime_schema()
            .release_i64_local(entry_local, function);
        self.runtime_schema()
            .release_i64_local(middle_local, function);
        self.runtime_schema()
            .release_i64_local(high_local, function);
        self.runtime_schema().release_i64_local(low_local, function);
    }
    fn emit_unicode_normalization_combining_class_lookup(
        &self,
        codepoint_local: I64Local,
        result_local: I64Local,
        function: &mut Function,
    ) {
        let low_local = self.runtime_schema().reserve_i64_local(function);
        let high_local = self.runtime_schema().reserve_i64_local(function);
        let middle_local = self.runtime_schema().reserve_i64_local(function);
        let entry_local = self.runtime_schema().reserve_i64_local(function);
        let candidate_local = self.runtime_schema().reserve_i64_local(function);

        for local in [result_local, low_local] {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }
        function.instruction(&Instruction::I64Const(
            self.strings.combining_class_count as i64,
        ));
        high_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        low_local.load(function);
        high_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        low_local.load(function);
        high_local.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        middle_local.store(function);
        function.instruction(&Instruction::I64Const(
            self.strings.combining_class_table_ptr as i64,
        ));
        middle_local.load(function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        entry_local.store(function);
        entry_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load(Self::memarg32(0)));
        function.instruction(&Instruction::I64ExtendI32U);
        candidate_local.store(function);
        codepoint_local.load(function);
        candidate_local.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        middle_local.load(function);
        high_local.store(function);
        function.instruction(&Instruction::Else);
        codepoint_local.load(function);
        candidate_local.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        middle_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        low_local.store(function);
        function.instruction(&Instruction::Else);
        self.emit_unicode_static_u32(entry_local, 4, result_local, function);
        result_local.load(function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        result_local.store(function);
        high_local.load(function);
        low_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        self.runtime_schema()
            .release_i64_local(candidate_local, function);
        self.runtime_schema()
            .release_i64_local(entry_local, function);
        self.runtime_schema()
            .release_i64_local(middle_local, function);
        self.runtime_schema()
            .release_i64_local(high_local, function);
        self.runtime_schema().release_i64_local(low_local, function);
    }
    fn emit_unicode_normalization_composition_lookup(
        &self,
        first_local: I64Local,
        second_local: I64Local,
        result_local: I64Local,
        function: &mut Function,
    ) {
        let low_local = self.runtime_schema().reserve_i64_local(function);
        let high_local = self.runtime_schema().reserve_i64_local(function);
        let middle_local = self.runtime_schema().reserve_i64_local(function);
        let entry_local = self.runtime_schema().reserve_i64_local(function);
        let candidate_first_local = self.runtime_schema().reserve_i64_local(function);
        let candidate_second_local = self.runtime_schema().reserve_i64_local(function);

        for local in [result_local, low_local] {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }
        function.instruction(&Instruction::I64Const(
            self.strings.composition_count as i64,
        ));
        high_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        low_local.load(function);
        high_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        low_local.load(function);
        high_local.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64ShrU);
        middle_local.store(function);
        function.instruction(&Instruction::I64Const(
            self.strings.composition_table_ptr as i64,
        ));
        middle_local.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        entry_local.store(function);
        self.emit_unicode_static_u32(entry_local, 0, candidate_first_local, function);
        candidate_first_local.load(function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        candidate_first_local.store(function);
        self.emit_unicode_static_u32(entry_local, 4, candidate_second_local, function);
        candidate_second_local.load(function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        candidate_second_local.store(function);

        first_local.load(function);
        candidate_first_local.load(function);
        function.instruction(&Instruction::I64LtU);
        first_local.load(function);
        candidate_first_local.load(function);
        function.instruction(&Instruction::I64Eq);
        second_local.load(function);
        candidate_second_local.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        middle_local.load(function);
        high_local.store(function);
        function.instruction(&Instruction::Else);
        first_local.load(function);
        candidate_first_local.load(function);
        function.instruction(&Instruction::I64GtU);
        first_local.load(function);
        candidate_first_local.load(function);
        function.instruction(&Instruction::I64Eq);
        second_local.load(function);
        candidate_second_local.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        middle_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        low_local.store(function);
        function.instruction(&Instruction::Else);
        self.emit_unicode_static_u32(entry_local, 8, result_local, function);
        result_local.load(function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        result_local.store(function);
        high_local.load(function);
        low_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        self.runtime_schema()
            .release_i64_local(candidate_second_local, function);
        self.runtime_schema()
            .release_i64_local(candidate_first_local, function);
        self.runtime_schema()
            .release_i64_local(entry_local, function);
        self.runtime_schema()
            .release_i64_local(middle_local, function);
        self.runtime_schema()
            .release_i64_local(high_local, function);
        self.runtime_schema().release_i64_local(low_local, function);
    }
    fn emit_unicode_static_u32(
        &self,
        address: I64Local,
        offset: u64,
        out: I64Local,
        f: &mut Function,
    ) {
        address.load(f);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::I32Load(Self::memarg32(offset)));
        f.instruction(&Instruction::I64ExtendI32U);
        out.store(f);
    }
    fn emit_unicode_soft_dotted(&self, cp: I64Local, out: I64Local, f: &mut Function) {
        f.instruction(&Instruction::I32Const(0));
        for range in CodePointSetData::new::<props::SoftDotted>().iter_ranges() {
            cp.load(f);
            f.instruction(&Instruction::I64Const(i64::from(*range.start())));
            f.instruction(&Instruction::I64GeU);
            cp.load(f);
            f.instruction(&Instruction::I64Const(i64::from(*range.end())));
            f.instruction(&Instruction::I64LeU);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::I32Or);
        }
        f.instruction(&Instruction::I64ExtendI32U);
        out.store(f);
    }
}

#[derive(Clone, Copy)]
enum UnicodeCaseContext {
    FollowingCased,
    MoreAbove,
    BeforeDot,
    AfterI,
    AfterSoftDotted,
}
impl FunctionBuilder<'_> {
    fn emit_unicode_case_context(
        &self,
        kind: UnicodeCaseContext,
        units: &GcLocal<CodeUnitArray>,
        length: I64Local,
        index: I64Local,
        width: I64Local,
        out: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        // Locale-sensitive casing shares canonical combining classes with
        // normalization. These scanners may be the first consumer in a Store.
        if !matches!(kind, UnicodeCaseContext::FollowingCased) {
            s.ensure_normalization_tables(
                self.strings
                    .normalization_region
                    .expect("locale combining classes require the sealed normalization group"),
                f,
            );
        }
        let scan = s.reserve_i64_local(f);
        let cp = s.reserve_i64_local(f);
        let step = s.reserve_i64_local(f);
        let class = s.reserve_i64_local(f);
        let property = s.reserve_i64_local(f);
        let previous = s.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        out.store(f);
        let backward = matches!(
            kind,
            UnicodeCaseContext::AfterI | UnicodeCaseContext::AfterSoftDotted
        );
        index.load(f);
        if !backward {
            width.load(f);
            f.instruction(&Instruction::I64Add);
        }
        scan.store(f);
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        if backward {
            scan.load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::BrIf(1));
            scan.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Sub);
            scan.store(f);
            self.emit_unicode_unit(units, scan, cp, f);
            cp.load(f);
            f.instruction(&Instruction::I64Const(0xdc00));
            f.instruction(&Instruction::I64GeU);
            cp.load(f);
            f.instruction(&Instruction::I64Const(0xdfff));
            f.instruction(&Instruction::I64LeU);
            f.instruction(&Instruction::I32And);
            scan.load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::I32Eqz);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::If(BlockType::Empty));
            scan.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Sub);
            previous.store(f);
            self.emit_unicode_unit(units, previous, cp, f);
            cp.load(f);
            f.instruction(&Instruction::I64Const(0xd800));
            f.instruction(&Instruction::I64GeU);
            cp.load(f);
            f.instruction(&Instruction::I64Const(0xdbff));
            f.instruction(&Instruction::I64LeU);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::If(BlockType::Empty));
            previous.load(f);
            scan.store(f);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::End);
        } else {
            scan.load(f);
            length.load(f);
            f.instruction(&Instruction::I64GeU);
            f.instruction(&Instruction::BrIf(1));
        }
        self.emit_unicode_decode(units, length, scan, cp, step, f);
        match kind {
            UnicodeCaseContext::FollowingCased => {
                self.emit_unicode_codepoint_in_range_table(
                    cp,
                    self.strings.case_ignorable_range_table_ptr,
                    self.strings.case_ignorable_range_count,
                    property,
                    f,
                );
                property.load(f);
                f.instruction(&Instruction::I64Eqz);
                f.instruction(&Instruction::If(BlockType::Empty));
                self.emit_unicode_codepoint_in_range_table(
                    cp,
                    self.strings.cased_range_table_ptr,
                    self.strings.cased_range_count,
                    out,
                    f,
                );
                f.instruction(&Instruction::Br(2));
                f.instruction(&Instruction::End);
            }
            UnicodeCaseContext::MoreAbove
            | UnicodeCaseContext::BeforeDot
            | UnicodeCaseContext::AfterI
            | UnicodeCaseContext::AfterSoftDotted => {
                self.emit_unicode_normalization_combining_class_lookup(cp, class, f);
                match kind {
                    UnicodeCaseContext::MoreAbove => {
                        class.load(f);
                        f.instruction(&Instruction::I64Const(230));
                        f.instruction(&Instruction::I64Eq);
                    }
                    UnicodeCaseContext::BeforeDot => {
                        cp.load(f);
                        f.instruction(&Instruction::I64Const(0x307));
                        f.instruction(&Instruction::I64Eq);
                    }
                    UnicodeCaseContext::AfterI => {
                        cp.load(f);
                        f.instruction(&Instruction::I64Const(0x49));
                        f.instruction(&Instruction::I64Eq);
                    }
                    UnicodeCaseContext::AfterSoftDotted => {
                        self.emit_unicode_soft_dotted(cp, property, f);
                        property.load(f);
                        f.instruction(&Instruction::I64Eqz);
                        f.instruction(&Instruction::I32Eqz);
                    }
                    UnicodeCaseContext::FollowingCased => unreachable!(),
                }
                f.instruction(&Instruction::If(BlockType::Empty));
                f.instruction(&Instruction::I64Const(1));
                out.store(f);
                f.instruction(&Instruction::Br(2));
                f.instruction(&Instruction::End);
                class.load(f);
                f.instruction(&Instruction::I64Eqz);
                if !matches!(kind, UnicodeCaseContext::MoreAbove) {
                    class.load(f);
                    f.instruction(&Instruction::I64Const(230));
                    f.instruction(&Instruction::I64Eq);
                    f.instruction(&Instruction::I32Or);
                }
                f.instruction(&Instruction::BrIf(1));
            }
        }
        if !backward {
            self.emit_unicode_increment(scan, step, f);
        }
        f.instruction(&Instruction::Br(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        s.release_i64_local(previous, f);
        s.release_i64_local(property, f);
        s.release_i64_local(class, f);
        s.release_i64_local(step, f);
        s.release_i64_local(cp, f);
        s.release_i64_local(scan, f);
    }
    fn emit_unicode_static_utf8(
        &self,
        address: I64Local,
        index: I64Local,
        cp: I64Local,
        width: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let byte = s.reserve_i64_local(f);
        let prefix = s.reserve_i64_local(f);
        address.load(f);
        index.load(f);
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::I32Load8U(Self::memarg8(0)));
        f.instruction(&Instruction::I64ExtendI32U);
        byte.store(f);
        byte.load(f);
        cp.store(f);
        f.instruction(&Instruction::I64Const(1));
        width.store(f);
        for (minimum, mask, n) in [(0xc0, 0x1f, 2), (0xe0, 0xf, 3), (0xf0, 7, 4)] {
            byte.load(f);
            f.instruction(&Instruction::I64Const(minimum));
            f.instruction(&Instruction::I64GeU);
            f.instruction(&Instruction::If(BlockType::Empty));
            byte.load(f);
            f.instruction(&Instruction::I64Const(mask));
            f.instruction(&Instruction::I64And);
            cp.store(f);
            f.instruction(&Instruction::I64Const(n));
            width.store(f);
            for offset in 1..n {
                address.load(f);
                index.load(f);
                f.instruction(&Instruction::I64Add);
                f.instruction(&Instruction::I64Const(offset));
                f.instruction(&Instruction::I64Add);
                f.instruction(&Instruction::I32WrapI64);
                f.instruction(&Instruction::I32Load8U(Self::memarg8(0)));
                f.instruction(&Instruction::I64ExtendI32U);
                f.instruction(&Instruction::I64Const(0x3f));
                f.instruction(&Instruction::I64And);
                prefix.store(f);
                cp.load(f);
                f.instruction(&Instruction::I64Const(6));
                f.instruction(&Instruction::I64Shl);
                prefix.load(f);
                f.instruction(&Instruction::I64Or);
                cp.store(f);
            }
            f.instruction(&Instruction::End);
        }
        s.release_i64_local(prefix, f);
        s.release_i64_local(byte, f);
    }
    pub(in crate::builtins) fn emit_string_case_builtin(
        &mut self,
        operation: StringCaseOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f, |b, input, output, _exit, f| {
            let s = b.runtime_schema();
            let locale = s.reserve_i64_local(f);
            f.instruction(&Instruction::I64Const(0));
            locale.store(f);
            if operation.localized() {
                let argument = s.reserve_value_local(f);
                b.emit_builtin_arg_to_value(0, &argument, f);
                let locales = b.emit_intl_canonical_locale_list(&argument, f)?;
                let selected = s
                    .reserve_gc_local::<StringValue, Nullable>(f)
                    .initialize_null(s, f);
                let count = s.reserve_i32_local(f);
                s.array_type::<ValueArray>().length(locales.values(), s, f);
                count.store(f);
                count.load(f);
                f.instruction(&Instruction::I32Eqz);
                f.instruction(&Instruction::If(BlockType::Empty));
                let default_locale = b.strings.intl_default_locale()?.as_str().to_owned();
                selected.replace(
                    b.emit_native_string_static(&default_locale, f).nullable(),
                    f,
                );
                f.instruction(&Instruction::Else);
                let index = s.reserve_i32_local(f);
                f.instruction(&Instruction::I32Const(0));
                index.store(f);
                let stored = s.reserve_gc_local(f).initialize(
                    s.array_type::<ValueArray>()
                        .read(locales.values(), index, s, f)
                        .reference(),
                    f,
                );
                let value = s.reserve_value_local(f);
                s.struct_type::<StoredValue>()
                    .read_into(&stored, &value, s, f);
                selected.replace(value.cast_reference::<StringValue>(s, f).nullable(), f);
                value.clear(f);
                stored.clear(f);
                s.release_i32_local(index, f);
                f.instruction(&Instruction::End);
                let text = s
                    .reserve_gc_local(f)
                    .initialize(selected.load(s, f).require_non_null(f), f);
                let units = b.emit_unicode_units(&text, f);
                let length = s.reserve_i64_local(f);
                let zero = s.reserve_i64_local(f);
                let first = s.reserve_i64_local(f);
                let second = s.reserve_i64_local(f);
                let third = s.reserve_i64_local(f);
                b.emit_native_gc_string_length(&text, length, f);
                f.instruction(&Instruction::I64Const(0));
                zero.store(f);
                length.load(f);
                f.instruction(&Instruction::I64Const(2));
                f.instruction(&Instruction::I64GeU);
                f.instruction(&Instruction::If(BlockType::Empty));
                b.emit_unicode_unit(&units, zero, first, f);
                f.instruction(&Instruction::I64Const(1));
                zero.store(f);
                b.emit_unicode_unit(&units, zero, second, f);
                // Prefix lookup over the actual UCD language-sensitive locales.
                length.load(f);
                f.instruction(&Instruction::I64Const(2));
                f.instruction(&Instruction::I64Eq);
                f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I64Const(2));
                zero.store(f);
                b.emit_unicode_unit(&units, zero, third, f);
                third.load(f);
                f.instruction(&Instruction::I64Const(45));
                f.instruction(&Instruction::I64Eq);
                f.instruction(&Instruction::End);
                f.instruction(&Instruction::If(BlockType::Empty));
                for (a, z, kind) in [(b't', b'r', 1), (b'a', b'z', 1), (b'l', b't', 2)] {
                    first.load(f);
                    f.instruction(&Instruction::I64Const(i64::from(a)));
                    f.instruction(&Instruction::I64Eq);
                    second.load(f);
                    f.instruction(&Instruction::I64Const(i64::from(z)));
                    f.instruction(&Instruction::I64Eq);
                    f.instruction(&Instruction::I32And);
                    f.instruction(&Instruction::If(BlockType::Empty));
                    f.instruction(&Instruction::I64Const(kind));
                    locale.store(f);
                    f.instruction(&Instruction::End);
                }
                f.instruction(&Instruction::End);
                f.instruction(&Instruction::End);
                s.release_i64_local(third, f);
                s.release_i64_local(second, f);
                s.release_i64_local(first, f);
                s.release_i64_local(zero, f);
                s.release_i64_local(length, f);
                units.clear(f);
                text.clear(f);
                s.release_i32_local(count, f);
                selected.clear(f);
                locales.clear(f);
                argument.clear(f);
            }
            let units = b.emit_unicode_units(input, f);
            let length = s.reserve_i64_local(f);
            b.emit_native_gc_string_length(input, length, f);
            let result = b.emit_unicode_publish_string(f, |b, sink, f| {
                b.emit_unicode_case_pass(operation, locale, &units, length, sink, f)
            })?;
            b.emit_native_string_normal_reference(&result, output, f);
            result.clear(f);
            s.release_i64_local(length, f);
            units.clear(f);
            s.release_i64_local(locale, f);
            Ok(())
        })
    }
    fn emit_unicode_case_pass(
        &self,
        operation: StringCaseOperation,
        locale: I64Local,
        units: &GcLocal<CodeUnitArray>,
        length: I64Local,
        sink: &UnicodeSink<'_>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i64_local(f);
        let cp = s.reserve_i64_local(f);
        let width = s.reserve_i64_local(f);
        let handled = s.reserve_i32_local(f);
        let context = s.reserve_i64_local(f);
        let preceding = s.reserve_i64_local(f);
        let property = s.reserve_i64_local(f);
        let mapping = s.reserve_i64_local(f);
        let mapping_len = s.reserve_i64_local(f);
        let cursor = s.reserve_i64_local(f);
        let mapped = s.reserve_i64_local(f);
        let mapped_width = s.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        f.instruction(&Instruction::I64Const(0));
        preceding.store(f);
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        f.instruction(&Instruction::BrIf(1));
        self.emit_unicode_decode(units, length, index, cp, width, f);
        f.instruction(&Instruction::I32Const(0));
        handled.store(f);
        if operation.upper() {
            locale.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Eq);
            cp.load(f);
            f.instruction(&Instruction::I64Const(0x69));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::If(BlockType::Empty));
            sink.literal("İ", s, f);
            f.instruction(&Instruction::I32Const(1));
            handled.store(f);
            f.instruction(&Instruction::End);
            locale.load(f);
            f.instruction(&Instruction::I64Const(2));
            f.instruction(&Instruction::I64Eq);
            cp.load(f);
            f.instruction(&Instruction::I64Const(0x307));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::If(BlockType::Empty));
            self.emit_unicode_case_context(
                UnicodeCaseContext::AfterSoftDotted,
                units,
                length,
                index,
                width,
                context,
                f,
            );
            context.load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::I32Eqz);
            handled.store(f);
            f.instruction(&Instruction::End);
        } else {
            locale.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::If(BlockType::Empty));
            for (point, text) in [(0x130, "i")] {
                cp.load(f);
                f.instruction(&Instruction::I64Const(point));
                f.instruction(&Instruction::I64Eq);
                f.instruction(&Instruction::If(BlockType::Empty));
                sink.literal(text, s, f);
                f.instruction(&Instruction::I32Const(1));
                handled.store(f);
                f.instruction(&Instruction::End);
            }
            cp.load(f);
            f.instruction(&Instruction::I64Const(0x49));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::If(BlockType::Empty));
            self.emit_unicode_case_context(
                UnicodeCaseContext::BeforeDot,
                units,
                length,
                index,
                width,
                context,
                f,
            );
            context.load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::If(BlockType::Empty));
            sink.literal("ı", s, f);
            f.instruction(&Instruction::I32Const(1));
            handled.store(f);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::End);
            cp.load(f);
            f.instruction(&Instruction::I64Const(0x307));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::If(BlockType::Empty));
            self.emit_unicode_case_context(
                UnicodeCaseContext::AfterI,
                units,
                length,
                index,
                width,
                context,
                f,
            );
            context.load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::I32Eqz);
            handled.store(f);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::End);
            locale.load(f);
            f.instruction(&Instruction::I64Const(2));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::If(BlockType::Empty));
            for (point, text, above) in [
                (0x49, "i\u{307}", true),
                (0x4a, "j\u{307}", true),
                (0x12e, "į\u{307}", true),
                (0xcc, "i\u{307}\u{300}", false),
                (0xcd, "i\u{307}\u{301}", false),
                (0x128, "i\u{307}\u{303}", false),
            ] {
                cp.load(f);
                f.instruction(&Instruction::I64Const(point));
                f.instruction(&Instruction::I64Eq);
                f.instruction(&Instruction::If(BlockType::Empty));
                if above {
                    self.emit_unicode_case_context(
                        UnicodeCaseContext::MoreAbove,
                        units,
                        length,
                        index,
                        width,
                        context,
                        f,
                    );
                    context.load(f);
                    f.instruction(&Instruction::I64Eqz);
                    f.instruction(&Instruction::I32Eqz);
                    f.instruction(&Instruction::If(BlockType::Empty));
                }
                sink.literal(text, s, f);
                f.instruction(&Instruction::I32Const(1));
                handled.store(f);
                if above {
                    f.instruction(&Instruction::End);
                }
                f.instruction(&Instruction::End);
            }
            f.instruction(&Instruction::End);
            cp.load(f);
            f.instruction(&Instruction::I64Const(0x3a3));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::If(BlockType::Empty));
            self.emit_unicode_case_context(
                UnicodeCaseContext::FollowingCased,
                units,
                length,
                index,
                width,
                context,
                f,
            );
            preceding.load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::I32Eqz);
            context.load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::If(BlockType::Empty));
            sink.literal("ς", s, f);
            f.instruction(&Instruction::I32Const(1));
            handled.store(f);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::End);
        }
        handled.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        let (table, count) = if operation.upper() {
            (
                self.strings.uppercase_mapping_table_ptr,
                self.strings.uppercase_mapping_count,
            )
        } else {
            (
                self.strings.lowercase_mapping_table_ptr,
                self.strings.lowercase_mapping_count,
            )
        };
        self.emit_unicode_codepoint_table_lookup(cp, table, count, mapping, mapping_len, f);
        mapping_len.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        sink.scalar(cp, s, f);
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I64Const(0));
        cursor.store(f);
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        cursor.load(f);
        mapping_len.load(f);
        f.instruction(&Instruction::I64GeU);
        f.instruction(&Instruction::BrIf(1));
        self.emit_unicode_static_utf8(mapping, cursor, mapped, mapped_width, f);
        sink.scalar(mapped, s, f);
        self.emit_unicode_increment(cursor, mapped_width, f);
        f.instruction(&Instruction::Br(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        self.emit_unicode_codepoint_in_range_table(
            cp,
            self.strings.case_ignorable_range_table_ptr,
            self.strings.case_ignorable_range_count,
            property,
            f,
        );
        property.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.emit_unicode_codepoint_in_range_table(
            cp,
            self.strings.cased_range_table_ptr,
            self.strings.cased_range_count,
            preceding,
            f,
        );
        f.instruction(&Instruction::End);
        self.emit_unicode_increment(index, width, f);
        f.instruction(&Instruction::Br(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        for local in [
            mapped_width,
            mapped,
            cursor,
            mapping_len,
            mapping,
            property,
            preceding,
            context,
        ] {
            s.release_i64_local(local, f);
        }
        s.release_i32_local(handled, f);
        s.release_i64_local(width, f);
        s.release_i64_local(cp, f);
        s.release_i64_local(index, f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_string_normalize_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f, |b, input, output, exit, f| {
            let s = b.runtime_schema();
            let argument = s.reserve_value_local(f);
            let pending = s.reserve_completion(f);
            let form = s
                .reserve_gc_local::<StringValue, Nullable>(f)
                .initialize_null(s, f);
            let found = s.reserve_i32_local(f);
            let equal = s.reserve_i32_local(f);
            let fold = s.reserve_i32_local(f);
            b.emit_builtin_arg_to_value(0, &argument, f);
            argument.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            f.instruction(&Instruction::I32Eq);
            b.open_frame(ControlFrameKind::If, f);
            form.replace(b.emit_native_string_static("NFC", f).nullable(), f);
            f.instruction(&Instruction::Else);
            b.emit_value_to_string_payload(&argument, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            form.replace(
                pending
                    .value()
                    .cast_reference::<StringValue>(s, f)
                    .nullable(),
                f,
            );
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            let actual = s
                .reserve_gc_local(f)
                .initialize(form.load(s, f).require_non_null(f), f);
            f.instruction(&Instruction::I32Const(0));
            found.store(f);
            f.instruction(&Instruction::I32Const(0));
            fold.store(f);
            for candidate in StringNormalizationForm::ALL {
                let expected = s
                    .reserve_gc_local(f)
                    .initialize(b.emit_native_string_static(candidate.spelling(), f), f);
                b.emit_gc_string_equality(&actual, &expected, fold, equal, f);
                equal.load(f);
                b.open_frame(ControlFrameKind::If, f);
                let result = b.emit_unicode_normalized_string(input, candidate, f)?;
                b.emit_native_string_normal_reference(&result, output, f);
                result.clear(f);
                f.instruction(&Instruction::I32Const(1));
                found.store(f);
                b.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                expected.clear(f);
            }
            found.load(f);
            f.instruction(&Instruction::I32Eqz);
            b.emit_native_string_error_if(
                RuntimeErrorMessage::STRING_PROTOTYPE_NORMALIZE_FORM_MUST_BE_NFC_NFD_NFKC_OR_NFKD,
                NativeErrorKind::RangeError,
                output,
                exit,
                f,
            )?;
            s.release_i32_local(fold, f);
            s.release_i32_local(equal, f);
            s.release_i32_local(found, f);
            actual.clear(f);
            form.clear(f);
            pending.clear(f);
            argument.clear(f);
            Ok(())
        })
    }
    fn emit_unicode_workspace_read(
        &self,
        bytes: &GcLocal<ByteArray>,
        offset: I64Local,
        delta: u64,
        out: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let index = s.reserve_i32_local(f);
        let byte = s.reserve_i32_local(f);
        f.instruction(&Instruction::I64Const(0));
        out.store(f);
        for n in 0..8 {
            offset.load(f);
            f.instruction(&Instruction::I64Const(delta as i64 + n));
            f.instruction(&Instruction::I64Add);
            f.instruction(&Instruction::I32WrapI64);
            index.store(f);
            s.array_type::<ByteArray>()
                .read(bytes, index, s, f)
                .store(byte, f);
            out.load(f);
            byte.load(f);
            f.instruction(&Instruction::I64ExtendI32U);
            f.instruction(&Instruction::I64Const(n * 8));
            f.instruction(&Instruction::I64Shl);
            f.instruction(&Instruction::I64Or);
            out.store(f);
        }
        s.release_i32_local(byte, f);
        s.release_i32_local(index, f);
    }
    fn emit_unicode_workspace_write(
        &self,
        bytes: &GcLocal<ByteArray>,
        offset: I64Local,
        delta: u64,
        value: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let index = s.reserve_i32_local(f);
        let byte = s.reserve_i32_local(f);
        for n in 0..8 {
            offset.load(f);
            f.instruction(&Instruction::I64Const(delta as i64 + n));
            f.instruction(&Instruction::I64Add);
            f.instruction(&Instruction::I32WrapI64);
            index.store(f);
            value.load(f);
            f.instruction(&Instruction::I64Const(n * 8));
            f.instruction(&Instruction::I64ShrU);
            f.instruction(&Instruction::I32WrapI64);
            byte.store(f);
            s.array_type::<ByteArray>()
                .write(bytes, index, GcOperand::i32_local(byte), s, f);
        }
        s.release_i32_local(byte, f);
        s.release_i32_local(index, f);
    }
    fn emit_unicode_normalized_string(
        &mut self,
        input: &GcLocal<StringValue>,
        form: StringNormalizationForm,
        function: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let s = self.runtime_schema();
        s.ensure_normalization_tables(
            self.strings
                .normalization_region
                .expect("normalization requires its sealed R group"),
            function,
        );
        let units = self.emit_unicode_units(input, function);
        let src_len_local = s.reserve_i64_local(function);
        let src_index_local = s.reserve_i64_local(function);
        let codepoint_local = s.reserve_i64_local(function);
        let advance_local = s.reserve_i64_local(function);
        let sequence_ptr_local = s.reserve_i64_local(function);
        let sequence_len_local = s.reserve_i64_local(function);
        let decomposed_count_local = s.reserve_i64_local(function);
        let slots_len_local = s.reserve_i64_local(function);
        let slots_local = s.reserve_i64_local(function);
        let output_index_local = s.reserve_i64_local(function);
        let sequence_index_local = s.reserve_i64_local(function);
        let sequence_codepoint_local = s.reserve_i64_local(function);
        let combining_class_local = s.reserve_i64_local(function);
        let slot_local = s.reserve_i64_local(function);
        let sort_index_local = s.reserve_i64_local(function);
        let insertion_index_local = s.reserve_i64_local(function);
        let previous_slot_local = s.reserve_i64_local(function);
        let previous_codepoint_local = s.reserve_i64_local(function);
        let previous_class_local = s.reserve_i64_local(function);
        let current_codepoint_local = s.reserve_i64_local(function);
        let current_class_local = s.reserve_i64_local(function);
        let starter_index_local = s.reserve_i64_local(function);
        let starter_codepoint_local = s.reserve_i64_local(function);
        let last_class_local = s.reserve_i64_local(function);
        let composed_codepoint_local = s.reserve_i64_local(function);
        let composed_count_local = s.reserve_i64_local(function);

        self.emit_native_gc_string_length(input, src_len_local, function);
        for local in [src_index_local, decomposed_count_local] {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }

        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        src_index_local.load(function);
        src_len_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_unicode_decode(
            &units,
            src_len_local,
            src_index_local,
            codepoint_local,
            advance_local,
            function,
        );
        self.emit_unicode_normalization_decomposition_lookup(
            codepoint_local,
            &form,
            sequence_ptr_local,
            sequence_len_local,
            function,
        );
        decomposed_count_local.load(function);
        sequence_len_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::Else);
        sequence_len_local.load(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Add);
        decomposed_count_local.store(function);
        src_index_local.load(function);
        advance_local.load(function);
        function.instruction(&Instruction::I64Add);
        src_index_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        decomposed_count_local.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Shl);
        slots_len_local.store(function);
        slots_len_local.load(function);
        function.instruction(&Instruction::I64Const(i32::MAX as i64));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        let capacity = s.reserve_i32_local(function);
        slots_len_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        capacity.store(function);
        let workspace = s.reserve_gc_local(function).initialize(
            s.array_type::<ByteArray>()
                .filled(GcOperand::i32(0), capacity, function),
            function,
        );
        function.instruction(&Instruction::I64Const(0));
        slots_local.store(function);
        for local in [src_index_local, output_index_local] {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }

        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        src_index_local.load(function);
        src_len_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_unicode_decode(
            &units,
            src_len_local,
            src_index_local,
            codepoint_local,
            advance_local,
            function,
        );
        self.emit_unicode_normalization_decomposition_lookup(
            codepoint_local,
            &form,
            sequence_ptr_local,
            sequence_len_local,
            function,
        );
        sequence_len_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        codepoint_local.load(function);
        sequence_codepoint_local.store(function);
        self.emit_unicode_normalization_combining_class_lookup(
            sequence_codepoint_local,
            combining_class_local,
            function,
        );
        slots_local.load(function);
        output_index_local.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        slot_local.store(function);
        self.emit_unicode_workspace_write(
            &workspace,
            slot_local,
            0,
            sequence_codepoint_local,
            function,
        );
        self.emit_unicode_workspace_write(
            &workspace,
            slot_local,
            8,
            combining_class_local,
            function,
        );
        output_index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        output_index_local.store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        sequence_index_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        sequence_index_local.load(function);
        sequence_len_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        sequence_ptr_local.load(function);
        sequence_index_local.load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Load(Self::memarg32(0)));
        function.instruction(&Instruction::I64ExtendI32U);
        sequence_codepoint_local.store(function);
        self.emit_unicode_normalization_combining_class_lookup(
            sequence_codepoint_local,
            combining_class_local,
            function,
        );
        slots_local.load(function);
        output_index_local.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        slot_local.store(function);
        self.emit_unicode_workspace_write(
            &workspace,
            slot_local,
            0,
            sequence_codepoint_local,
            function,
        );
        self.emit_unicode_workspace_write(
            &workspace,
            slot_local,
            8,
            combining_class_local,
            function,
        );
        output_index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        output_index_local.store(function);
        sequence_index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        sequence_index_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        src_index_local.load(function);
        advance_local.load(function);
        function.instruction(&Instruction::I64Add);
        src_index_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        function.instruction(&Instruction::I64Const(1));
        sort_index_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        sort_index_local.load(function);
        decomposed_count_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        slots_local.load(function);
        sort_index_local.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        slot_local.store(function);
        self.emit_unicode_workspace_read(
            &workspace,
            slot_local,
            0,
            current_codepoint_local,
            function,
        );
        self.emit_unicode_workspace_read(&workspace, slot_local, 8, current_class_local, function);
        current_class_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Else);
        sort_index_local.load(function);
        insertion_index_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        insertion_index_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::BrIf(1));
        slots_local.load(function);
        insertion_index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        previous_slot_local.store(function);
        self.emit_unicode_workspace_read(
            &workspace,
            previous_slot_local,
            8,
            previous_class_local,
            function,
        );
        previous_class_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        previous_class_local.load(function);
        current_class_local.load(function);
        function.instruction(&Instruction::I64LeU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::BrIf(1));
        self.emit_unicode_workspace_read(
            &workspace,
            previous_slot_local,
            0,
            previous_codepoint_local,
            function,
        );
        slots_local.load(function);
        insertion_index_local.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        slot_local.store(function);
        self.emit_unicode_workspace_write(
            &workspace,
            slot_local,
            0,
            previous_codepoint_local,
            function,
        );
        self.emit_unicode_workspace_write(
            &workspace,
            slot_local,
            8,
            previous_class_local,
            function,
        );
        insertion_index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        insertion_index_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        slots_local.load(function);
        insertion_index_local.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        slot_local.store(function);
        self.emit_unicode_workspace_write(
            &workspace,
            slot_local,
            0,
            current_codepoint_local,
            function,
        );
        self.emit_unicode_workspace_write(&workspace, slot_local, 8, current_class_local, function);
        function.instruction(&Instruction::End);
        sort_index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        sort_index_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        if form.composes() {
            decomposed_count_local.load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::I64Const(0));
            composed_count_local.store(function);
            function.instruction(&Instruction::Else);
            function.instruction(&Instruction::I64Const(0));
            starter_index_local.store(function);
            function.instruction(&Instruction::I64Const(0));
            last_class_local.store(function);
            function.instruction(&Instruction::I64Const(1));
            composed_count_local.store(function);
            function.instruction(&Instruction::I64Const(1));
            sort_index_local.store(function);
            slots_local.load(function);
            slot_local.store(function);
            self.emit_unicode_workspace_read(
                &workspace,
                slot_local,
                0,
                starter_codepoint_local,
                function,
            );
            function.instruction(&Instruction::Block(BlockType::Empty));
            function.instruction(&Instruction::Loop(BlockType::Empty));
            sort_index_local.load(function);
            decomposed_count_local.load(function);
            function.instruction(&Instruction::I64GeU);
            function.instruction(&Instruction::BrIf(1));
            slots_local.load(function);
            sort_index_local.load(function);
            function.instruction(&Instruction::I64Const(4));
            function.instruction(&Instruction::I64Shl);
            function.instruction(&Instruction::I64Add);
            slot_local.store(function);
            self.emit_unicode_workspace_read(
                &workspace,
                slot_local,
                0,
                current_codepoint_local,
                function,
            );
            self.emit_unicode_workspace_read(
                &workspace,
                slot_local,
                8,
                current_class_local,
                function,
            );
            self.emit_unicode_normalization_composition_lookup(
                starter_codepoint_local,
                current_codepoint_local,
                composed_codepoint_local,
                function,
            );
            self.emit_unicode_normalization_combining_class_lookup(
                starter_codepoint_local,
                combining_class_local,
                function,
            );
            combining_class_local.load(function);
            function.instruction(&Instruction::I64Eqz);
            composed_codepoint_local.load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            last_class_local.load(function);
            current_class_local.load(function);
            function.instruction(&Instruction::I64LtU);
            last_class_local.load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32Or);
            function.instruction(&Instruction::I32And);
            function.instruction(&Instruction::If(BlockType::Empty));
            slots_local.load(function);
            starter_index_local.load(function);
            function.instruction(&Instruction::I64Const(4));
            function.instruction(&Instruction::I64Shl);
            function.instruction(&Instruction::I64Add);
            slot_local.store(function);
            self.emit_unicode_workspace_write(
                &workspace,
                slot_local,
                0,
                composed_codepoint_local,
                function,
            );
            composed_codepoint_local.load(function);
            starter_codepoint_local.store(function);
            function.instruction(&Instruction::Else);
            slots_local.load(function);
            composed_count_local.load(function);
            function.instruction(&Instruction::I64Const(4));
            function.instruction(&Instruction::I64Shl);
            function.instruction(&Instruction::I64Add);
            slot_local.store(function);
            self.emit_unicode_workspace_write(
                &workspace,
                slot_local,
                0,
                current_codepoint_local,
                function,
            );
            self.emit_unicode_workspace_write(
                &workspace,
                slot_local,
                8,
                current_class_local,
                function,
            );
            current_class_local.load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
            composed_count_local.load(function);
            starter_index_local.store(function);
            current_codepoint_local.load(function);
            starter_codepoint_local.store(function);
            function.instruction(&Instruction::End);
            current_class_local.load(function);
            last_class_local.store(function);
            composed_count_local.load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Add);
            composed_count_local.store(function);
            function.instruction(&Instruction::End);
            sort_index_local.load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Add);
            sort_index_local.store(function);
            function.instruction(&Instruction::Br(0));
            function.instruction(&Instruction::End);
            function.instruction(&Instruction::End);
            function.instruction(&Instruction::End);
        } else {
            decomposed_count_local.load(function);
            composed_count_local.store(function);
        }

        let result = self.emit_unicode_publish_string(function, |b, sink, function| {
            function.instruction(&Instruction::I64Const(0));
            output_index_local.store(function);
            function.instruction(&Instruction::Block(BlockType::Empty));
            function.instruction(&Instruction::Loop(BlockType::Empty));
            output_index_local.load(function);
            composed_count_local.load(function);
            function.instruction(&Instruction::I64GeU);
            function.instruction(&Instruction::BrIf(1));
            output_index_local.load(function);
            function.instruction(&Instruction::I64Const(4));
            function.instruction(&Instruction::I64Shl);
            slot_local.store(function);
            b.emit_unicode_workspace_read(&workspace, slot_local, 0, codepoint_local, function);
            sink.scalar(codepoint_local, s, function);
            output_index_local.load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Add);
            output_index_local.store(function);
            function.instruction(&Instruction::Br(0));
            function.instruction(&Instruction::End);
            function.instruction(&Instruction::End);
            Ok(())
        })?;
        workspace.clear(function);
        s.release_i32_local(capacity, function);
        for local in [
            composed_count_local,
            composed_codepoint_local,
            last_class_local,
            starter_codepoint_local,
            starter_index_local,
            current_class_local,
            current_codepoint_local,
            previous_class_local,
            previous_codepoint_local,
            previous_slot_local,
            insertion_index_local,
            sort_index_local,
            slot_local,
            combining_class_local,
            sequence_codepoint_local,
            sequence_index_local,
            output_index_local,
            slots_local,
            slots_len_local,
            decomposed_count_local,
            sequence_len_local,
            sequence_ptr_local,
            advance_local,
            codepoint_local,
            src_index_local,
            src_len_local,
        ] {
            s.release_i64_local(local, function);
        }
        units.clear(function);
        Ok(result)
    }
}
