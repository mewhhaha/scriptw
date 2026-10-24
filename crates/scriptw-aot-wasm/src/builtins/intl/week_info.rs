use super::*;
use crate::builtins::intl_provider_wire::IntlByteArrayReader;
use scriptw_intl::IntlOperation;
impl FunctionBuilder<'_> {
    pub(crate) fn emit_intl_locale_get_week_info(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record = self.emit_intl_locale_record_from_receiver(function)?;
        let tag = self.emit_intl_locale_tag(&record, function);
        let response = self.emit_intl_locale_provider_response(
            scriptw_intl::LocaleWeekInfoOperation::HOST_OP,
            &tag,
            false,
            function,
        )?;
        let reader = IntlByteArrayReader::new(&response, schema, function);
        let word = schema.reserve_i64_local(function);
        let first = schema.reserve_i64_local(function);
        let mask = schema.reserve_i64_local(function);
        let number = schema.reserve_i64_local(function);
        reader.read_u64(word, schema, function);
        reader.finish(schema, function);
        word.load(function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        first.store(function);
        word.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        mask.store(function);
        for (local, max) in [(first, 7), (mask, 0x7f)] {
            local.load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Sub);
            function.instruction(&Instruction::I64Const(max - 1));
            function.instruction(&Instruction::I64GtU);
            self.emit_intl_locale_provider_fault_if(function);
        }
        let value = schema.reserve_value_local(function);
        let list = crate::functions::ArgumentListConstruction::new(self, function);
        for day in 1..=7 {
            mask.load(function);
            function.instruction(&Instruction::I64Const(1 << (day - 1)));
            function.instruction(&Instruction::I64And);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const((day as f64).to_bits() as i64));
            number.store(function);
            value.set_number(number, function);
            list.append(&value, schema, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        let values = list.finish(self, function);
        let array = self.emit_array_from_argument_list(&values, function)?;
        let object = self.emit_intl_number_result_object(function)?;
        first.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        number.store(function);
        value.set_number(number, function);
        self.emit_intl_number_append_result_property(&object, "firstDay", &value, function)?;
        value.set_reference(&array, schema, function);
        self.emit_intl_number_append_result_property(&object, "weekend", &value, function)?;
        self.completion().initialize(function);
        self.completion()
            .value()
            .set_reference(&object, schema, function);
        object.clear(function);
        array.clear(function);
        values.clear(function);
        value.clear(function);
        for local in [number, mask, first, word] {
            schema.release_i64_local(local, function);
        }
        response.clear(function);
        tag.clear(function);
        record.clear(function);
        Ok(())
    }
}
