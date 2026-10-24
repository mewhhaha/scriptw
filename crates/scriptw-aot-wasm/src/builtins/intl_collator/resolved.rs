use super::*;
use crate::functions::ArgumentListConstruction;
impl FunctionBuilder<'_> {
    pub(crate) fn emit_intl_collator_supported_locales_of(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &value, function);
        let locales = self.emit_intl_canonical_locale_list(&value, function)?;
        self.emit_builtin_arg_to_value(1, &value, function);
        self.emit_intl_number_options_object(&value, function)?;
        let matcher = GcI32DomainLocal::new(schema, LocaleMatcher::BestFit, function);
        self.emit_intl_number_choice_option(
            &value,
            IntlErrorOption::LocaleMatcher,
            LocaleMatcher::ALL.iter().map(|v| (v.name(), *v)),
            LocaleMatcher::BestFit,
            &matcher,
            function,
        )?;
        let response = self.emit_collator_provider_call(
            CollatorProviderRequest::Supported {
                locales: &locales,
                matcher: &matcher,
            },
            function,
        )?;
        let reader = response.reader(schema, function);
        let count = schema.reserve_i64_local(function);
        let index = schema.reserve_i64_local(function);
        reader.read_u64(count, schema, function);
        reader.require_records(count, 8, function);
        let list = ArgumentListConstruction::new(self, function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, function);
        let text = reader.read_utf8(schema, function);
        value.set_reference(&text, schema, function);
        list.append(&value, schema, function);
        text.clear(function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        reader.finish(schema, function);
        let list = list.finish(self, function);
        let array = self.emit_array_from_argument_list(&list, function)?;
        self.completion().initialize(function);
        self.completion()
            .value()
            .set_reference(&array, schema, function);
        array.clear(function);
        list.clear(function);
        schema.release_i64_local(index, function);
        schema.release_i64_local(count, function);
        response.clear(function);
        matcher.clear(schema, function);
        locales.clear(function);
        value.clear(function);
        Ok(())
    }
    fn emit_collator_resolved_domain<V: GcI32Constant + Copy>(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        property: &str,
        selected: &GcI32DomainLocal<V>,
        variants: impl IntoIterator<Item = (V, &'static str)>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let recognized = schema.reserve_i32_local(function);
        set_i32(recognized, 0, function);
        for (variant, spelling) in variants {
            emit_domain_is(selected, variant, function);
            self.open_frame(ControlFrameKind::If, function);
            let text = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(spelling, function)?,
                function,
            );
            value.set_reference(&text, schema, function);
            text.clear(function);
            set_i32(recognized, 1, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        recognized.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_intl_number_append_result_property(object, property, &value, function)?;
        schema.release_i32_local(recognized, function);
        value.clear(function);
        Ok(())
    }
    pub(crate) fn emit_intl_collator_resolved_options(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let record = self.emit_collator_record_from_receiver(function)?;
        let schema = self.runtime_schema();
        let co = schema.struct_type::<IntlCollatorObject>();
        let object = self.emit_intl_number_result_object(function)?;
        let value = schema.reserve_value_local(function);
        let locale = schema.reserve_gc_local(function).initialize(
            co.field(IntlCollatorObjectSchema::LOCALE)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        value.set_reference(&locale, schema, function);
        self.emit_intl_number_append_result_property(&object, "locale", &value, function)?;
        locale.clear(function);
        let usage = GcI32DomainLocal::new(schema, CollatorUsage::Sort, function);
        co.field(IntlCollatorObjectSchema::USAGE)
            .read(&record, schema, function)
            .store_domain(&usage, function);
        self.emit_collator_resolved_domain(
            &object,
            "usage",
            &usage,
            CollatorUsage::ALL.iter().map(|v| (*v, v.name())),
            function,
        )?;
        usage.clear(schema, function);
        let sensitivity = GcI32DomainLocal::new(schema, CollatorSensitivity::Variant, function);
        co.field(IntlCollatorObjectSchema::SENSITIVITY)
            .read(&record, schema, function)
            .store_domain(&sensitivity, function);
        self.emit_collator_resolved_domain(
            &object,
            "sensitivity",
            &sensitivity,
            CollatorSensitivity::ALL.iter().map(|v| (*v, v.name())),
            function,
        )?;
        sensitivity.clear(schema, function);
        let boolean = schema.reserve_i32_local(function);
        co.field(IntlCollatorObjectSchema::IGNORE_PUNCTUATION)
            .read(&record, schema, function)
            .store(boolean, function);
        value.set_boolean(boolean, function);
        self.emit_intl_number_append_result_property(
            &object,
            "ignorePunctuation",
            &value,
            function,
        )?;
        let kind = GcI32DomainLocal::new(schema, CollatorCollationKind::Default, function);
        co.field(IntlCollatorObjectSchema::COLLATION_KIND)
            .read(&record, schema, function)
            .store_domain(&kind, function);
        let collation = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("default", function)?,
            function,
        );
        emit_domain_is(&kind, CollatorCollationKind::UnicodeType, function);
        self.open_frame(ControlFrameKind::If, function);
        collation.replace(
            co.field(IntlCollatorObjectSchema::COLLATION)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.set_reference(&collation, schema, function);
        self.emit_intl_number_append_result_property(&object, "collation", &value, function)?;
        collation.clear(function);
        kind.clear(schema, function);
        co.field(IntlCollatorObjectSchema::NUMERIC)
            .read(&record, schema, function)
            .store(boolean, function);
        value.set_boolean(boolean, function);
        self.emit_intl_number_append_result_property(&object, "numeric", &value, function)?;
        let case = GcI32DomainLocal::new(schema, CollatorCaseFirst::False, function);
        co.field(IntlCollatorObjectSchema::CASE_FIRST)
            .read(&record, schema, function)
            .store_domain(&case, function);
        self.emit_collator_resolved_domain(
            &object,
            "caseFirst",
            &case,
            CollatorCaseFirst::ALL.iter().map(|v| (*v, v.name())),
            function,
        )?;
        case.clear(schema, function);
        self.completion().initialize(function);
        self.completion()
            .value()
            .set_reference(&object, schema, function);
        schema.release_i32_local(boolean, function);
        value.clear(function);
        object.clear(function);
        record.clear(function);
        Ok(())
    }
}
