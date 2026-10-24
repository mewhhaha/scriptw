//! Framed provider bytes become a private List only after identifier checks.
use super::*;
use crate::builtins::intl_provider_wire::IntlByteArrayReader;
use scriptw_intl::IntlOperation;
#[derive(Clone, Copy)]
pub(super) enum LocaleInformationKind {
    Calendars,
    Collations,
    TimeZones,
}
impl LocaleInformationKind {
    fn operation(self) -> scriptw_intl::IntlHostOp {
        match self {
            Self::Calendars => scriptw_intl::LocaleCalendarsOperation::HOST_OP,
            Self::Collations => scriptw_intl::LocaleCollationsOperation::HOST_OP,
            Self::TimeZones => scriptw_intl::LocaleTimeZonesOperation::HOST_OP,
        }
    }
}
impl FunctionBuilder<'_> {
    fn emit_intl_locale_ascii_name_equality(
        &self,
        units: &GcLocal<CodeUnitArray>,
        length: I32Local,
        name: &str,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let index = schema.reserve_i32_local(function);
        let unit = schema.reserve_i32_local(function);
        length.load(function);
        function.instruction(&Instruction::I32Const(name.len() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(1));
        for (position, byte) in name.bytes().enumerate() {
            set_i32(index, position as i32, function);
            schema
                .array_type::<CodeUnitArray>()
                .read(units, index, schema, function)
                .store(unit, function);
            unit.load(function);
            function.instruction(&Instruction::I32Const(i32::from(byte)));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32And);
        }
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::End);
        schema.release_i32_local(unit, function);
        schema.release_i32_local(index, function);
    }
    fn emit_validate_locale_information_name(
        &mut self,
        kind: LocaleInformationKind,
        text: &GcLocal<StringValue>,
        seen: I64Local,
        previous: &GcLocal<StringValue, Nullable>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(text, schema, function)
                .reference(),
            function,
        );
        let length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let component = schema.reserve_i32_local(function);
        let unit = schema.reserve_i32_local(function);
        let matched = schema.reserve_i32_local(function);
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        length.store(function);
        set_i32(index, 0, function);
        set_i32(component, 0, function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        schema
            .array_type::<CodeUnitArray>()
            .read(&units, index, schema, function)
            .store(unit, function);
        unit.load(function);
        function.instruction(&Instruction::I32Const(
            if matches!(kind, LocaleInformationKind::TimeZones) {
                b'/' as i32
            } else {
                b'-' as i32
            },
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        match kind {
            LocaleInformationKind::Calendars | LocaleInformationKind::Collations => {
                self.emit_intl_locale_range(component, 3, 8, function);
                function.instruction(&Instruction::I32Eqz);
            }
            LocaleInformationKind::TimeZones => component.load(function),
        }
        if matches!(kind, LocaleInformationKind::TimeZones) {
            function.instruction(&Instruction::I32Eqz);
        }
        self.emit_intl_locale_provider_fault_if(function);
        set_i32(component, 0, function);
        function.instruction(&Instruction::Else);
        self.emit_intl_locale_range(unit, b'a' as i32, b'z' as i32, function);
        self.emit_intl_locale_range(unit, b'0' as i32, b'9' as i32, function);
        function.instruction(&Instruction::I32Or);
        if matches!(kind, LocaleInformationKind::TimeZones) {
            self.emit_intl_locale_range(unit, b'A' as i32, b'Z' as i32, function);
            function.instruction(&Instruction::I32Or);
            for byte in [b'_', b'-', b'+'] {
                unit.load(function);
                function.instruction(&Instruction::I32Const(i32::from(byte)));
                function.instruction(&Instruction::I32Eq);
                function.instruction(&Instruction::I32Or);
            }
        }
        function.instruction(&Instruction::I32Eqz);
        self.emit_intl_locale_provider_fault_if(function);
        component.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        component.store(function);
        function.instruction(&Instruction::End);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        match kind {
            LocaleInformationKind::Calendars | LocaleInformationKind::Collations => {
                self.emit_intl_locale_range(component, 3, 8, function);
                function.instruction(&Instruction::I32Eqz);
            }
            LocaleInformationKind::TimeZones => {
                component.load(function);
                function.instruction(&Instruction::I32Eqz);
            }
        }
        self.emit_intl_locale_provider_fault_if(function);
        match kind {
            LocaleInformationKind::Calendars => {
                set_i32(matched, 0, function);
                for (index, calendar) in scriptw_intl::DateTimeCalendar::ALL.iter().enumerate() {
                    self.emit_intl_locale_ascii_name_equality(
                        &units,
                        length,
                        calendar.as_str(),
                        function,
                    );
                    self.open_frame(ControlFrameKind::If, function);
                    let bit = (1u64 << index) as i64;
                    seen.load(function);
                    function.instruction(&Instruction::I64Const(bit));
                    function.instruction(&Instruction::I64And);
                    function.instruction(&Instruction::I64Eqz);
                    function.instruction(&Instruction::I32Eqz);
                    self.emit_intl_locale_provider_fault_if(function);
                    seen.load(function);
                    function.instruction(&Instruction::I64Const(bit));
                    function.instruction(&Instruction::I64Or);
                    seen.store(function);
                    set_i32(matched, 1, function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
                matched.load(function);
                function.instruction(&Instruction::I32Eqz);
                self.emit_intl_locale_provider_fault_if(function);
            }
            LocaleInformationKind::Collations => {
                for reserved in ["standard", "search", "searchjl"] {
                    self.emit_intl_locale_ascii_name_equality(&units, length, reserved, function);
                    self.emit_intl_locale_provider_fault_if(function);
                }
            }
            LocaleInformationKind::TimeZones => {}
        }
        if !matches!(kind, LocaleInformationKind::Calendars) {
            previous.load(schema, function).is_null(function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            let prev = schema.reserve_gc_local(function).initialize(
                previous.load(schema, function).require_non_null(function),
                function,
            );
            self.emit_string_payload_utf16_compare_i32(&prev, text, function);
            function.instruction(&Instruction::I32Const(0));
            function.instruction(&Instruction::I32GeS);
            self.emit_intl_locale_provider_fault_if(function);
            prev.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            previous.replace(text.load(schema, function).nullable(), function);
        }
        for local in [matched, unit, component, index, length] {
            schema.release_i32_local(local, function);
        }
        units.clear(function);
        Ok(())
    }
    pub(super) fn emit_intl_locale_information_array(
        &mut self,
        kind: LocaleInformationKind,
        tag: &GcLocal<StringValue>,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let response =
            self.emit_intl_locale_provider_response(kind.operation(), tag, true, function)?;
        let reader = IntlByteArrayReader::new(&response, schema, function);
        let count = schema.reserve_i64_local(function);
        let index = schema.reserve_i64_local(function);
        let seen = schema.reserve_i64_local(function);
        let word = schema.reserve_i64_local(function);
        reader.read_u64(word, schema, function);
        word.load(function);
        function.instruction(&Instruction::I64Const(
            scriptw_intl::LOCALE_INFORMATION_WIRE_VERSION as i64,
        ));
        function.instruction(&Instruction::I64Ne);
        self.emit_intl_locale_provider_fault_if(function);
        reader.read_u64(word, schema, function);
        word.load(function);
        function.instruction(&Instruction::I64Const(
            (u64::from(kind.operation().code()) * 2 + 1) as i64,
        ));
        function.instruction(&Instruction::I64Ne);
        self.emit_intl_locale_provider_fault_if(function);
        reader.read_u64(count, schema, function);
        reader.require_records(count, 9, function);
        count.load(function);
        function.instruction(&Instruction::I64Const(i32::MAX as i64));
        function.instruction(&Instruction::I64GtU);
        self.emit_intl_locale_provider_fault_if(function);
        if matches!(kind, LocaleInformationKind::Calendars) {
            count.load(function);
            function.instruction(&Instruction::I64Eqz);
            self.emit_intl_locale_provider_fault_if(function);
            count.load(function);
            function.instruction(&Instruction::I64Const(
                scriptw_intl::DateTimeCalendar::ALL.len() as i64,
            ));
            function.instruction(&Instruction::I64GtU);
            self.emit_intl_locale_provider_fault_if(function);
        }
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::I64Const(0));
        seen.store(function);
        let previous = schema
            .reserve_gc_local::<StringValue, Nullable>(function)
            .initialize_null(schema, function);
        let value = schema.reserve_value_local(function);
        let list = crate::functions::ArgumentListConstruction::new(self, function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        let text = reader.read_utf8(schema, function);
        self.emit_validate_locale_information_name(kind, &text, seen, &previous, function)?;
        value.set_reference(&text, schema, function);
        list.append(&value, schema, function);
        text.clear(function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        reader.finish(schema, function);
        let values = list.finish(self, function);
        let array = self.emit_array_from_argument_list(&values, function)?;
        output.set_reference(&array, schema, function);
        array.clear(function);
        values.clear(function);
        value.clear(function);
        previous.clear(function);
        for local in [word, seen, index, count] {
            schema.release_i64_local(local, function);
        }
        response.clear(function);
        Ok(())
    }
}
