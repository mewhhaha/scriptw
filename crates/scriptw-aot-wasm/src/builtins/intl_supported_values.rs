//! Checked native names become rooted Strings and one fresh called-Realm Array.
use super::super::*;
use crate::functions::ArgumentListConstruction;
use crate::gc_types::*;
use scriptw_intl::SupportedValuesKey;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_intl_supported_values_of(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &argument, function);
        let key = self.emit_intl_number_to_string(&argument, function)?;
        let matched = schema.reserve_i32_local(function);
        super::intl_number::set_i32(matched, 0, function);
        let output = schema.reserve_value_local(function);
        for selected in SupportedValuesKey::ALL {
            let source = self.strings.intl_supported_values_table(selected)?.source();
            let expected = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(selected.as_str(), function)?,
                function,
            );
            self.emit_string_payload_equality_i32(&key, &expected, function);
            self.open_frame(ControlFrameKind::If, function);
            super::intl_number::set_i32(matched, 1, function);
            if let Some(source) = source {
                let list = ArgumentListConstruction::new(self, function);
                let value = schema.reserve_value_local(function);
                for name in source.values() {
                    let text = schema.reserve_gc_local(function).initialize(
                        self.emit_interned_string_reference(name, function)?,
                        function,
                    );
                    value.set_reference(&text, schema, function);
                    list.append(&value, schema, function);
                    text.clear(function);
                }
                let list = list.finish(self, function);
                let array = self.emit_array_from_argument_list(&list, function)?;
                output.set_reference(&array, schema, function);
                array.clear(function);
                list.clear(function);
                value.clear(function);
            } else {
                self.emit_intl_number_type_error(
                    RuntimeErrorMessage::INTL_SUPPORTEDVALUESOF_DATA_UNAVAILABLE,
                    function,
                )?;
            }
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            expected.clear(function);
        }
        matched.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_intl_number_range_error(
            RuntimeErrorMessage::INVALID_INTL_SUPPORTEDVALUESOF_KEY,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().initialize(function);
        self.completion().value().copy_from(&output, function);
        output.clear(function);
        schema.release_i32_local(matched, function);
        key.clear(function);
        argument.clear(function);
        Ok(())
    }
}
