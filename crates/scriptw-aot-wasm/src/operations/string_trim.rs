use super::*;

/// TrimString admits exactly these three boundary policies.
#[derive(Clone, Copy)]
pub(crate) enum EcmaTrimMode {
    Start,
    End,
    Both,
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_gc_string_trim_range(
        &mut self,
        string: &GcLocal<StringValue>,
        start: I64Local,
        end: I64Local,
        function: &mut Function,
    ) {
        self.emit_gc_string_trim_range_mode(string, start, end, EcmaTrimMode::Both, function);
    }

    pub(crate) fn emit_gc_string_trim_range_mode(
        &mut self,
        string: &GcLocal<StringValue>,
        start: I64Local,
        end: I64Local,
        mode: EcmaTrimMode,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let cursor = schema.reserve_i64_local(function);
        let unit = schema.reserve_i64_local(function);
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(string, schema, function)
                .reference(),
            function,
        );
        function.instruction(&Instruction::I64Const(0));
        start.store(function);
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        function.instruction(&Instruction::I64ExtendI32U);
        end.store(function);
        if !matches!(mode, EcmaTrimMode::End) {
            self.open_frame(ControlFrameKind::Block, function);
            self.open_frame(ControlFrameKind::Loop, function);
            start.load(function);
            end.load(function);
            function.instruction(&Instruction::I64GeU);
            function.instruction(&Instruction::BrIf(1));
            self.emit_gc_string_code_unit_i32(string, start, function);
            function.instruction(&Instruction::I64ExtendI32U);
            unit.store(function);
            self.emit_ecmascript_whitespace_i32(unit, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::BrIf(1));
            start.load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Add);
            start.store(function);
            function.instruction(&Instruction::Br(0));
            self.pop_control(ControlFrameKind::Loop);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
        }
        if !matches!(mode, EcmaTrimMode::Start) {
            self.open_frame(ControlFrameKind::Block, function);
            self.open_frame(ControlFrameKind::Loop, function);
            start.load(function);
            end.load(function);
            function.instruction(&Instruction::I64GeU);
            function.instruction(&Instruction::BrIf(1));
            end.load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Sub);
            cursor.store(function);
            self.emit_gc_string_code_unit_i32(string, cursor, function);
            function.instruction(&Instruction::I64ExtendI32U);
            unit.store(function);
            self.emit_ecmascript_whitespace_i32(unit, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::BrIf(1));
            cursor.load(function);
            end.store(function);
            function.instruction(&Instruction::Br(0));
            self.pop_control(ControlFrameKind::Loop);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
        }
        units.clear(function);
        schema.release_i64_local(unit, function);
        schema.release_i64_local(cursor, function);
    }
}
