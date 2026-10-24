use super::*;
use crate::functions::ArgumentListConstruction;
impl FunctionBuilder<'_> {
    fn emit_relative_resolved_choice<V: GcI32Constant + Copy>(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        property: &str,
        selected: &GcI32DomainLocal<V>,
        variants: impl IntoIterator<Item = (&'static str, V)>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(f);
        let recognized = schema.reserve_i32_local(f);
        set_i32(recognized, 0, f);
        for (spelling, variant) in variants {
            emit_domain_is(selected, variant, f);
            self.open_frame(ControlFrameKind::If, f);
            let text = schema
                .reserve_gc_local(f)
                .initialize(self.emit_interned_string_reference(spelling, f)?, f);
            value.set_reference(&text, schema, f);
            text.clear(f);
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
        self.emit_intl_number_append_result_property(object, property, &value, f)?;
        schema.release_i32_local(recognized, f);
        value.clear(f);
        Ok(())
    }
    pub(crate) fn emit_intl_relative_time_resolved_options(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let record = self.emit_relative_record_from_receiver(f)?;
        let schema = self.runtime_schema();
        let rt = schema.struct_type::<IntlRelativeTimeFormatObject>();
        let object = self.emit_intl_number_result_object(f)?;
        let value = schema.reserve_value_local(f);
        let locale = schema.reserve_gc_local(f).initialize(
            rt.field(IntlRelativeTimeFormatObjectSchema::LOCALE)
                .read(&record, schema, f)
                .reference(),
            f,
        );
        value.set_reference(&locale, schema, f);
        self.emit_intl_number_append_result_property(&object, "locale", &value, f)?;
        locale.clear(f);
        let style = GcI32DomainLocal::new(schema, RelativeStyle::Long, f);
        rt.field(IntlRelativeTimeFormatObjectSchema::STYLE)
            .read(&record, schema, f)
            .store_domain(&style, f);
        self.emit_relative_resolved_choice(
            &object,
            "style",
            &style,
            RelativeStyle::ALL.iter().map(|v| (v.name(), *v)),
            f,
        )?;
        style.clear(schema, f);
        let numeric = GcI32DomainLocal::new(schema, RelativeNumeric::Always, f);
        rt.field(IntlRelativeTimeFormatObjectSchema::NUMERIC)
            .read(&record, schema, f)
            .store_domain(&numeric, f);
        self.emit_relative_resolved_choice(
            &object,
            "numeric",
            &numeric,
            RelativeNumeric::ALL.iter().map(|v| (v.name(), *v)),
            f,
        )?;
        numeric.clear(schema, f);
        let numbering = schema.reserve_gc_local(f).initialize(
            rt.field(IntlRelativeTimeFormatObjectSchema::NUMBERING_SYSTEM)
                .read(&record, schema, f)
                .reference(),
            f,
        );
        value.set_reference(&numbering, schema, f);
        self.emit_intl_number_append_result_property(&object, "numberingSystem", &value, f)?;
        numbering.clear(f);
        self.completion().initialize(f);
        self.completion().value().set_reference(&object, schema, f);
        value.clear(f);
        object.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_intl_relative_time_supported_locales_of(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &value, f);
        let locales = self.emit_intl_canonical_locale_list(&value, f)?;
        self.emit_builtin_arg_to_value(1, &value, f);
        self.emit_intl_number_options_object(&value, f)?;
        let matcher = GcI32DomainLocal::new(schema, LocaleMatcher::BestFit, f);
        self.emit_intl_number_choice_option(
            &value,
            IntlErrorOption::LocaleMatcher,
            LocaleMatcher::ALL.iter().map(|v| (v.name(), *v)),
            LocaleMatcher::BestFit,
            &matcher,
            f,
        )?;
        let response = self.emit_relative_provider_call(
            RelativeProviderRequest::Supported {
                locales: &locales,
                matcher: &matcher,
            },
            f,
        )?;
        let reader = response.reader(schema, f);
        let count = schema.reserve_i64_local(f);
        let index = schema.reserve_i64_local(f);
        reader.read_u64(count, schema, f);
        reader.require_records(count, 8, f);
        let list = ArgumentListConstruction::new(self, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        let text = reader.read_utf8(schema, f);
        value.set_reference(&text, schema, f);
        list.append(&value, schema, f);
        text.clear(f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        reader.finish(schema, f);
        let list = list.finish(self, f);
        let array = self.emit_array_from_argument_list(&list, f)?;
        self.completion().initialize(f);
        self.completion().value().set_reference(&array, schema, f);
        array.clear(f);
        list.clear(f);
        schema.release_i64_local(index, f);
        schema.release_i64_local(count, f);
        response.clear(f);
        matcher.clear(schema, f);
        locales.clear(f);
        value.clear(f);
        Ok(())
    }
}
