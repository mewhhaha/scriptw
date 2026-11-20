//! Count and fill EscapeRegExpPattern without copying a growing string.
use super::*;

struct SourceCursor {
    index: I32Local,
    length: I32Local,
    unit: I32Local,
    escaped: I32Local,
    in_class: I32Local,
    replaced: I32Local,
    changed: I32Local,
    output_length: I64Local,
    ordinal: I32Local,
    output_unit: I32Local,
}

impl SourceCursor {
    fn append_unit(
        &self,
        unit: I32Local,
        construction: Option<&StringConstruction>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        if let Some(construction) = construction {
            self.output_length.load(function);
            function.instruction(&Instruction::I32WrapI64);
            self.ordinal.store(function);
            construction.write(self.ordinal, unit, schema, function);
        }
        self.output_length.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        self.output_length.store(function);
    }

    fn append_literal(
        &self,
        text: &str,
        construction: Option<&StringConstruction>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        if construction.is_some() {
            for unit in text.encode_utf16() {
                self.output_unit.set_constant(i32::from(unit), function);
                self.append_unit(self.output_unit, construction, schema, function);
            }
        } else {
            self.output_length.load(function);
            function.instruction(&Instruction::I64Const(text.encode_utf16().count() as i64));
            function.instruction(&Instruction::I64Add);
            self.output_length.store(function);
        }
    }
}

impl FunctionBuilder<'_> {
    /// The caller has handled the empty pattern. Immutable source units stay
    /// rooted across allocation; only the fresh construction permits writes.
    pub(super) fn emit_escaped_regexp_source(
        &mut self,
        source: &GcLocal<StringValue>,
        completion: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let schema = self.runtime_schema();
        let units = schema
            .reserve_gc_local::<CodeUnitArray, NonNullable>(function)
            .initialize(
                schema
                    .field(StringValueSchema::CODE_UNITS)
                    .read(source, schema, function)
                    .reference(),
                function,
            );
        let cursor = SourceCursor {
            index: schema.reserve_i32_local(function),
            length: schema.reserve_i32_local(function),
            unit: schema.reserve_i32_local(function),
            escaped: schema.reserve_i32_local(function),
            in_class: schema.reserve_i32_local(function),
            replaced: schema.reserve_i32_local(function),
            changed: schema.reserve_i32_local(function),
            output_length: schema.reserve_i64_local(function),
            ordinal: schema.reserve_i32_local(function),
            output_unit: schema.reserve_i32_local(function),
        };
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        cursor.length.store(function);
        self.emit_regexp_source_pass(&units, &cursor, None, function);
        let result = schema
            .reserve_gc_local(function)
            .initialize(source.load(schema, function), function);
        cursor.changed.load(function);
        self.open_frame(ControlFrameKind::If, function);
        cursor.output_length.load(function);
        function.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
        function.instruction(&Instruction::I64GtU);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::STRING_LENGTH_EXCEEDS_IMPLEMENTATION_LIMIT,
            NativeErrorKind::RangeError,
            completion,
            exit,
            function,
        )?;
        let length = schema.reserve_i32_local(function);
        cursor.output_length.load(function);
        function.instruction(&Instruction::I32WrapI64);
        length.store(function);
        let construction = StringConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        self.emit_regexp_source_pass(&units, &cursor, Some(&construction), function);
        result.replace(construction.publish(schema, function), function);
        schema.release_i32_local(length, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for local in [
            cursor.output_unit,
            cursor.ordinal,
            cursor.changed,
            cursor.replaced,
            cursor.in_class,
            cursor.escaped,
            cursor.unit,
            cursor.length,
            cursor.index,
        ] {
            schema.release_i32_local(local, function);
        }
        schema.release_i64_local(cursor.output_length, function);
        units.clear(function);
        Ok(result)
    }

    fn emit_regexp_source_pass(
        &mut self,
        units: &GcLocal<CodeUnitArray, NonNullable>,
        cursor: &SourceCursor,
        construction: Option<&StringConstruction>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        for local in [
            cursor.index,
            cursor.escaped,
            cursor.in_class,
            cursor.changed,
        ] {
            local.set_constant(0, function);
        }
        function.instruction(&Instruction::I64Const(0));
        cursor.output_length.store(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        cursor.index.load(function);
        cursor.length.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, function);
        schema
            .array_type::<CodeUnitArray>()
            .read(units, cursor.index, schema, function)
            .store(cursor.unit, function);
        cursor.replaced.set_constant(0, function);
        for (code, replacement) in scriptw_ir::REGEXP_SOURCE_LINE_ESCAPES {
            cursor.unit.load(function);
            function.instruction(&Instruction::I32Const(i32::from(code)));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            cursor.replaced.set_constant(1, function);
            cursor.changed.set_constant(1, function);
            cursor.escaped.load(function);
            self.open_frame(ControlFrameKind::If, function);
            cursor.append_literal(&replacement[1..], construction, schema, function);
            function.instruction(&Instruction::Else);
            cursor.append_literal(replacement, construction, schema, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        cursor.unit.load(function);
        function.instruction(&Instruction::I32Const(47));
        function.instruction(&Instruction::I32Eq);
        cursor.escaped.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        cursor.in_class.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        cursor.replaced.set_constant(1, function);
        cursor.changed.set_constant(1, function);
        cursor.append_literal(
            scriptw_ir::REGEXP_SOURCE_SLASH_ESCAPE,
            construction,
            schema,
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        cursor.replaced.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        cursor.append_unit(cursor.unit, construction, schema, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // State follows original units, including after a line replacement.
        cursor.escaped.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        for (code, inside) in [(91, true), (93, false)] {
            cursor.unit.load(function);
            function.instruction(&Instruction::I32Const(code));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            cursor.in_class.set_constant(i32::from(inside), function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        cursor.unit.load(function);
        function.instruction(&Instruction::I32Const(92));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        cursor.escaped.load(function);
        function.instruction(&Instruction::I32Eqz);
        cursor.escaped.store(function);
        function.instruction(&Instruction::Else);
        cursor.escaped.set_constant(0, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        cursor.index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        cursor.index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
    }
}
