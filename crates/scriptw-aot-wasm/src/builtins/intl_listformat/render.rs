use super::*;
use crate::functions::ArgumentListConstruction;
/// Only the request producer couples native indexed parts to their completed input.
struct ObservedListPartsResponse<'a> {
    response: ListProviderResponse,
    input: &'a ObservedStringListLocals,
}
impl FunctionBuilder<'_> {
    fn emit_observed_list_parts_response<'a>(
        &mut self,
        record: &GcLocal<IntlListFormatObject>,
        input: &'a ObservedStringListLocals,
        f: &mut Function,
    ) -> Result<ObservedListPartsResponse<'a>, EmitError> {
        let response =
            self.emit_list_provider_call(ListProviderRequest::Parts { record, input }, f)?;
        Ok(ObservedListPartsResponse { response, input })
    }
    pub(super) fn emit_list_format_partition(
        &mut self,
        record: &GcLocal<IntlListFormatObject>,
        input: &ObservedStringListLocals,
        mode: ListFormatOutput,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let observed = self.emit_observed_list_parts_response(record, input, f)?;
        let input = observed.input;
        let schema = self.runtime_schema();
        let reader = observed.response.reader(schema, f);
        let count = schema.reserve_i64_local(f);
        let index = schema.reserve_i64_local(f);
        let element = schema.reserve_i32_local(f);
        let selected = schema.reserve_i64_local(f);
        let kind = schema.reserve_i64_local(f);
        let value = schema.reserve_value_local(f);
        let text = schema
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("", f)?, f);
        let part_type = schema
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("", f)?, f);
        reader.read_u64(count, schema, f);
        reader.require_records(count, 16, f);
        input.count().load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        count.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let current = match mode {
            ListFormatOutput::String => Some(
                schema
                    .reserve_gc_local(f)
                    .initialize(self.emit_interned_string_reference("", f)?, f),
            ),
            ListFormatOutput::Parts => None,
        };
        let parts = match mode {
            ListFormatOutput::String => None,
            ListFormatOutput::Parts => Some(ArgumentListConstruction::new(self, f)),
        };
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        set_i32(element, 0, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, f);
        reader.read_u64(kind, schema, f);
        kind.load(f);
        f.instruction(&Instruction::I64Const(
            ListPartKind::Element.wire_code() as i64
        ));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        reader.read_u64(selected, schema, f);
        selected.load(f);
        element.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Ne);
        element.load(f);
        input.count().load(f);
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_argument_vector_entry_to_value(input.values(), element, &value, f);
        text.replace(value.cast_reference::<StringValue>(schema, f), f);
        part_type.replace(
            self.emit_interned_string_reference(ListPartKind::Element.name(), f)?,
            f,
        );
        element.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        element.store(f);
        f.instruction(&Instruction::Else);
        kind.load(f);
        f.instruction(&Instruction::I64Const(
            ListPartKind::Literal.wire_code() as i64
        ));
        f.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let literal = reader.read_utf16(schema, f);
        let units = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(&literal, schema, f)
                .reference(),
            f,
        );
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        units.clear(f);
        text.replace(literal.load(schema, f), f);
        literal.clear(f);
        part_type.replace(
            self.emit_interned_string_reference(ListPartKind::Literal.name(), f)?,
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        match mode {
            ListFormatOutput::String => {
                let current = current
                    .as_ref()
                    .expect("String output owns its accumulator");
                current.replace(self.emit_concat_gc_strings(current, &text, f), f);
            }
            ListFormatOutput::Parts => {
                let object = self.emit_intl_number_result_object(f)?;
                value.set_reference(&part_type, schema, f);
                self.emit_intl_number_append_result_property(&object, "type", &value, f)?;
                value.set_reference(&text, schema, f);
                self.emit_intl_number_append_result_property(&object, "value", &value, f)?;
                value.set_reference(&object, schema, f);
                parts
                    .as_ref()
                    .expect("Parts output owns its List")
                    .append(&value, schema, f);
                object.clear(f);
            }
        }
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        element.load(f);
        input.count().load(f);
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        reader.finish(schema, f);
        self.completion().initialize(f);
        match mode {
            ListFormatOutput::String => {
                let current = current.expect("String output owns its accumulator");
                self.completion().value().set_reference(&current, schema, f);
                current.clear(f);
            }
            ListFormatOutput::Parts => {
                let list = parts.expect("Parts output owns its List").finish(self, f);
                let array = self.emit_array_from_argument_list(&list, f)?;
                self.completion().value().set_reference(&array, schema, f);
                array.clear(f);
                list.clear(f);
            }
        }
        part_type.clear(f);
        text.clear(f);
        value.clear(f);
        schema.release_i64_local(kind, f);
        schema.release_i64_local(selected, f);
        schema.release_i32_local(element, f);
        schema.release_i64_local(index, f);
        schema.release_i64_local(count, f);
        observed.response.clear(f);
        Ok(())
    }
}
