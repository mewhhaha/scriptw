use super::*;
use crate::functions::ArgumentListConstruction;
impl FunctionBuilder<'_> {
    fn emit_complete_relative_time_inputs(
        &mut self,
        f: &mut Function,
    ) -> Result<CompletedRelativeTimeFormatInputsLocals, EmitError> {
        let schema = self.runtime_schema();
        let bits = schema.reserve_i64_local(f);
        let unit = GcI32DomainLocal::new(schema, RelativeUnit::Year, f);
        let value = schema.reserve_value_local(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        self.emit_builtin_arg_to_value(0, &value, f);
        self.emit_value_to_number_payload(&value, &pending, f)?;
        self.emit_intl_number_adopt_completion(&pending, f);
        pending.value().scalar().load(f);
        bits.store(f);
        pending.clear(f);
        self.emit_builtin_arg_to_value(1, &value, f);
        let text = self.emit_intl_number_to_string(&value, f)?;
        // Unit coercion completes before even a non-finite numeric input is rejected.
        bits.load(f);
        f.instruction(&Instruction::I64Const(0x7ff0_0000_0000_0000));
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Const(0x7ff0_0000_0000_0000));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_intl_number_range_error(RT_FINITE_ERROR, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let recognized = schema.reserve_i32_local(f);
        set_i32(recognized, 0, f);
        for candidate in RelativeUnit::ALL {
            for spelling in [
                candidate.name().to_owned(),
                format!("{}s", candidate.name()),
            ] {
                let expected = schema
                    .reserve_gc_local(f)
                    .initialize(self.emit_interned_string_reference(&spelling, f)?, f);
                self.emit_string_payload_equality_i32(&text, &expected, f);
                self.open_frame(ControlFrameKind::If, f);
                unit.set_constant(*candidate, f);
                set_i32(recognized, 1, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                expected.clear(f);
            }
        }
        recognized.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_intl_number_range_error(RT_UNIT_ERROR, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(recognized, f);
        text.clear(f);
        value.clear(f);
        Ok(CompletedRelativeTimeFormatInputsLocals {
            value_bits: bits,
            unit,
        })
    }
    pub(crate) fn emit_intl_relative_time_format(
        &mut self,
        f: &mut Function,
        mode: RelativeTimeFormatOutput,
    ) -> Result<(), EmitError> {
        let record = self.emit_relative_record_from_receiver(f)?;
        let input = self.emit_complete_relative_time_inputs(f)?;
        let response = self.emit_relative_provider_call(
            RelativeProviderRequest::Parts {
                record: &record,
                input: &input,
            },
            f,
        )?;
        let schema = self.runtime_schema();
        let reader = response.reader(schema, f);
        let count = schema.reserve_i64_local(f);
        let index = schema.reserve_i64_local(f);
        let kind = schema.reserve_i64_local(f);
        let presence = schema.reserve_i64_local(f);
        let unit = schema.reserve_i64_local(f);
        let recognized = schema.reserve_i32_local(f);
        let value = schema.reserve_value_local(f);
        let text = schema
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("", f)?, f);
        let part_type = schema
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("", f)?, f);
        let unit_text = schema
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("", f)?, f);
        reader.read_u64(count, schema, f);
        reader.require_records(count, 24, f);
        count.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let current = match mode {
            RelativeTimeFormatOutput::String => Some(
                schema
                    .reserve_gc_local(f)
                    .initialize(self.emit_interned_string_reference("", f)?, f),
            ),
            RelativeTimeFormatOutput::Parts => None,
        };
        let parts = match mode {
            RelativeTimeFormatOutput::String => None,
            RelativeTimeFormatOutput::Parts => Some(ArgumentListConstruction::new(self, f)),
        };
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, f);
        reader.read_u64(kind, schema, f);
        set_i32(recognized, 0, f);
        for candidate in [
            NumberPartKind::Literal,
            NumberPartKind::Integer,
            NumberPartKind::Group,
            NumberPartKind::Decimal,
            NumberPartKind::Fraction,
        ] {
            kind.load(f);
            f.instruction(&Instruction::I64Const(candidate.wire_code() as i64));
            f.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, f);
            part_type.replace(self.emit_interned_string_reference(candidate.name(), f)?, f);
            set_i32(recognized, 1, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        recognized.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let observed = reader.read_utf8(schema, f);
        text.replace(observed.load(schema, f), f);
        observed.clear(f);
        reader.read_u64(presence, schema, f);
        presence.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        reader.read_u64(unit, schema, f);
        unit.load(f);
        input.unit.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        for candidate in RelativeUnit::ALL {
            emit_domain_is(&input.unit, *candidate, f);
            self.open_frame(ControlFrameKind::If, f);
            unit_text.replace(self.emit_interned_string_reference(candidate.name(), f)?, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::Else);
        presence.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64Ne);
        kind.load(f);
        f.instruction(&Instruction::I64Const(
            NumberPartKind::Literal.wire_code() as i64
        ));
        f.instruction(&Instruction::I64Ne);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        match mode {
            RelativeTimeFormatOutput::String => {
                let current = current
                    .as_ref()
                    .expect("String output owns its accumulator");
                current.replace(self.emit_concat_gc_strings(current, &text, f), f);
            }
            RelativeTimeFormatOutput::Parts => {
                let object = self.emit_intl_number_result_object(f)?;
                value.set_reference(&part_type, schema, f);
                self.emit_intl_number_append_result_property(&object, "type", &value, f)?;
                value.set_reference(&text, schema, f);
                self.emit_intl_number_append_result_property(&object, "value", &value, f)?;
                presence.load(f);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Eq);
                self.open_frame(ControlFrameKind::If, f);
                value.set_reference(&unit_text, schema, f);
                self.emit_intl_number_append_result_property(&object, "unit", &value, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
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
        reader.finish(schema, f);
        self.completion().initialize(f);
        match mode {
            RelativeTimeFormatOutput::String => {
                let current = current.expect("String output owns its accumulator");
                self.completion().value().set_reference(&current, schema, f);
                current.clear(f);
            }
            RelativeTimeFormatOutput::Parts => {
                let list = parts.expect("Parts output owns its List").finish(self, f);
                let array = self.emit_array_from_argument_list(&list, f)?;
                self.completion().value().set_reference(&array, schema, f);
                array.clear(f);
                list.clear(f);
            }
        }
        unit_text.clear(f);
        part_type.clear(f);
        text.clear(f);
        value.clear(f);
        schema.release_i32_local(recognized, f);
        schema.release_i64_local(unit, f);
        schema.release_i64_local(presence, f);
        schema.release_i64_local(kind, f);
        schema.release_i64_local(index, f);
        schema.release_i64_local(count, f);
        response.clear(f);
        input.clear(schema, f);
        record.clear(f);
        Ok(())
    }
}
