use super::extension_options::LocaleExtensionOption;
use super::*;
use crate::builtins::intl_provider_wire::IntlByteArrayReader;
use scriptw_intl::IntlOperation;
impl FunctionBuilder<'_> {
    fn emit_intl_locale_hour_cycles_array(
        &mut self,
        tag: &GcLocal<StringValue>,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let response = self.emit_intl_locale_provider_response(
            scriptw_intl::LocaleHourCyclesOperation::HOST_OP,
            tag,
            false,
            function,
        )?;
        let reader = IntlByteArrayReader::new(&response, schema, function);
        let word = schema.reserve_i64_local(function);
        let count = schema.reserve_i64_local(function);
        let packed = schema.reserve_i64_local(function);
        let code = schema.reserve_i64_local(function);
        let bit = schema.reserve_i64_local(function);
        let seen = schema.reserve_i64_local(function);
        reader.read_u64(word, schema, function);
        reader.finish(schema, function);
        word.load(function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        count.store(function);
        word.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        packed.store(function);
        count.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64GtU);
        self.emit_intl_locale_provider_fault_if(function);
        function.instruction(&Instruction::I64Const(0));
        seen.store(function);
        let value = schema.reserve_value_local(function);
        let list = crate::functions::ArgumentListConstruction::new(self, function);
        for index in 0..4 {
            packed.load(function);
            function.instruction(&Instruction::I64Const(index * 8));
            function.instruction(&Instruction::I64ShrU);
            function.instruction(&Instruction::I64Const(0xff));
            function.instruction(&Instruction::I64And);
            code.store(function);
            count.load(function);
            function.instruction(&Instruction::I64Const(index));
            function.instruction(&Instruction::I64GtU);
            self.open_frame(ControlFrameKind::If, function);
            code.load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Sub);
            function.instruction(&Instruction::I64Const(3));
            function.instruction(&Instruction::I64GtU);
            self.emit_intl_locale_provider_fault_if(function);
            function.instruction(&Instruction::I64Const(1));
            code.load(function);
            function.instruction(&Instruction::I64Shl);
            bit.store(function);
            seen.load(function);
            bit.load(function);
            function.instruction(&Instruction::I64And);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32Eqz);
            self.emit_intl_locale_provider_fault_if(function);
            seen.load(function);
            bit.load(function);
            function.instruction(&Instruction::I64Or);
            seen.store(function);
            for (selected, name) in [(1, "h11"), (2, "h12"), (3, "h23"), (4, "h24")] {
                code.load(function);
                function.instruction(&Instruction::I64Const(selected));
                function.instruction(&Instruction::I64Eq);
                self.open_frame(ControlFrameKind::If, function);
                let text = schema.reserve_gc_local(function).initialize(
                    self.emit_interned_string_reference(name, function)?,
                    function,
                );
                value.set_reference(&text, schema, function);
                list.append(&value, schema, function);
                text.clear(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            function.instruction(&Instruction::Else);
            code.load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32Eqz);
            self.emit_intl_locale_provider_fault_if(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        let values = list.finish(self, function);
        let array = self.emit_array_from_argument_list(&values, function)?;
        output.set_reference(&array, schema, function);
        array.clear(function);
        values.clear(function);
        value.clear(function);
        for local in [seen, bit, code, packed, count, word] {
            schema.release_i64_local(local, function);
        }
        response.clear(function);
        Ok(())
    }
    pub(crate) fn emit_intl_locale_get_hour_cycles(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record = self.emit_intl_locale_record_from_receiver(function)?;
        let tag = self.emit_intl_locale_tag(&record, function);
        let explicit =
            self.emit_intl_locale_extension_value(&tag, LocaleExtensionOption::HourCycle, function);
        let output = schema.reserve_value_local(function);
        explicit.load(schema, function).is_null(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_intl_locale_hour_cycles_array(&tag, &output, function)?;
        function.instruction(&Instruction::Else);
        let text = schema.reserve_gc_local(function).initialize(
            explicit.load(schema, function).require_non_null(function),
            function,
        );
        self.emit_intl_locale_info_string_singleton(&text, &output, function)?;
        text.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().initialize(function);
        self.completion().value().copy_from(&output, function);
        output.clear(function);
        explicit.clear(function);
        tag.clear(function);
        record.clear(function);
        Ok(())
    }
}
