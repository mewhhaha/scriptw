use super::*;
use crate::functions::ArgumentListConstruction;

impl FunctionBuilder<'_> {
    pub(super) fn emit_intl_locale_info_string_singleton(
        &mut self,
        text: &GcLocal<StringValue>,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.set_reference(text, schema, function);
        let list = ArgumentListConstruction::new(self, function);
        list.append(&value, schema, function);
        let list = list.finish(self, function);
        let array = self.emit_array_from_argument_list(&list, function)?;
        output.set_reference(&array, schema, function);
        array.clear(function);
        list.clear(function);
        value.clear(function);
        Ok(())
    }
}
