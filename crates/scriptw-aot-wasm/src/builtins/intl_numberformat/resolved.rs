use super::*;
use crate::builtins::intl_provider_wire::IntlNumberProviderRequest;
use crate::functions::ArgumentListConstruction;

impl FunctionBuilder<'_> {
    fn emit_nf_resolved_choice<V: GcI32Constant + Copy>(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        property: &str,
        code: &GcI32DomainLocal<V>,
        choices: impl IntoIterator<Item = (&'static str, V)>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let recognized = schema.reserve_i32_local(function);
        set_i32(recognized, 0, function);
        for (spelling, expected) in choices {
            emit_domain_is(code, expected, function);
            self.open_frame(ControlFrameKind::If, function);
            let text = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(spelling, function)?,
                function,
            );
            value.set_reference(&text, schema, function);
            set_i32(recognized, 1, function);
            text.clear(function);
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
    pub(crate) fn emit_intl_number_format_resolved_options(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let record =
            self.emit_nf_record_from_receiver(NfReceiverOperation::ResolvedOptions, function)?;
        let schema = self.runtime_schema();
        let nf = schema.struct_type::<IntlNumberFormatObject>();
        let object = self.emit_intl_number_result_object(function)?;
        let selected = NfOptionsLocals::new(self, function)?;
        nf.field(IntlNumberFormatObjectSchema::STYLE)
            .read(&record, schema, function)
            .store_domain(&selected.style, function);
        nf.field(IntlNumberFormatObjectSchema::CURRENCY_DISPLAY)
            .read(&record, schema, function)
            .store_domain(&selected.currency_display, function);
        nf.field(IntlNumberFormatObjectSchema::CURRENCY_SIGN)
            .read(&record, schema, function)
            .store_domain(&selected.currency_sign, function);
        nf.field(IntlNumberFormatObjectSchema::UNIT_DISPLAY)
            .read(&record, schema, function)
            .store_domain(&selected.unit_display, function);
        nf.field(IntlNumberFormatObjectSchema::NOTATION)
            .read(&record, schema, function)
            .store_domain(&selected.notation, function);
        nf.field(IntlNumberFormatObjectSchema::COMPACT_DISPLAY)
            .read(&record, schema, function)
            .store_domain(&selected.compact_display, function);
        nf.field(IntlNumberFormatObjectSchema::GROUPING)
            .read(&record, schema, function)
            .store_domain(&selected.grouping, function);
        nf.field(IntlNumberFormatObjectSchema::SIGN_DISPLAY)
            .read(&record, schema, function)
            .store_domain(&selected.sign, function);
        selected.style_text.replace(
            nf.field(IntlNumberFormatObjectSchema::STYLE_TEXT)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        let rounding = schema.reserve_gc_local(function).initialize(
            nf.field(IntlNumberFormatObjectSchema::ROUNDING)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        let rules = schema.struct_type::<IntlNumberRounding>();
        let precision = GcI32DomainLocal::new(schema, NumberPrecisionKind::Fraction, function);
        rules
            .field(IntlNumberRoundingSchema::PRECISION)
            .read(&rounding, schema, function)
            .store_domain(&precision, function);
        let mode = GcI32DomainLocal::new(schema, RoundingMode::HalfExpand, function);
        rules
            .field(IntlNumberRoundingSchema::ROUNDING_MODE)
            .read(&rounding, schema, function)
            .store_domain(&mode, function);
        let trailing = GcI32DomainLocal::new(schema, TrailingZeroDisplay::Auto, function);
        rules
            .field(IntlNumberRoundingSchema::TRAILING_ZERO)
            .read(&rounding, schema, function)
            .store_domain(&trailing, function);
        let value = schema.reserve_value_local(function);
        let count = schema.reserve_i32_local(function);
        let bits = schema.reserve_i64_local(function);
        macro_rules! number_property {
            ($field:expr, $name:literal) => {{
                rules
                    .field($field)
                    .read(&rounding, schema, function)
                    .store(count, function);
                count.load(function);
                function.instruction(&Instruction::I32Const(0));
                function.instruction(&Instruction::I32LtS);
                self.open_frame(ControlFrameKind::If, function);
                function.instruction(&Instruction::Unreachable);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                count.load(function);
                function.instruction(&Instruction::F64ConvertI32U);
                function.instruction(&Instruction::I64ReinterpretF64);
                bits.store(function);
                value.set_number(bits, function);
                self.emit_intl_number_append_result_property(&object, $name, &value, function)?;
            }};
        }
        for (name, field) in [
            ("locale", IntlNumberFormatObjectSchema::LOCALE),
            (
                "numberingSystem",
                IntlNumberFormatObjectSchema::NUMBERING_SYSTEM,
            ),
        ] {
            let text = schema.reserve_gc_local(function).initialize(
                nf.field(field).read(&record, schema, function).reference(),
                function,
            );
            value.set_reference(&text, schema, function);
            self.emit_intl_number_append_result_property(&object, name, &value, function)?;
            text.clear(function);
        }
        self.emit_nf_resolved_choice(
            &object,
            "style",
            &selected.style,
            StyleOption::ALL.iter().map(|v| (v.name(), *v)),
            function,
        )?;
        emit_domain_is(&selected.style, StyleOption::Currency, function);
        self.open_frame(ControlFrameKind::If, function);
        value.set_reference(&selected.style_text, schema, function);
        self.emit_intl_number_append_result_property(&object, "currency", &value, function)?;
        self.emit_nf_resolved_choice(
            &object,
            "currencyDisplay",
            &selected.currency_display,
            CurrencyDisplay::ALL.iter().map(|v| (v.name(), Some(*v))),
            function,
        )?;
        self.emit_nf_resolved_choice(
            &object,
            "currencySign",
            &selected.currency_sign,
            CurrencySign::ALL.iter().map(|v| (v.name(), Some(*v))),
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        emit_domain_is(&selected.style, StyleOption::Unit, function);
        self.open_frame(ControlFrameKind::If, function);
        value.set_reference(&selected.style_text, schema, function);
        self.emit_intl_number_append_result_property(&object, "unit", &value, function)?;
        self.emit_nf_resolved_choice(
            &object,
            "unitDisplay",
            &selected.unit_display,
            UnitDisplay::ALL.iter().map(|v| (v.name(), Some(*v))),
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        number_property!(
            IntlNumberRoundingSchema::MINIMUM_INTEGER,
            "minimumIntegerDigits"
        );
        emit_domain_is(&precision, NumberPrecisionKind::Significant, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        number_property!(
            IntlNumberRoundingSchema::MINIMUM_FRACTION,
            "minimumFractionDigits"
        );
        number_property!(
            IntlNumberRoundingSchema::MAXIMUM_FRACTION,
            "maximumFractionDigits"
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        emit_domain_is(&precision, NumberPrecisionKind::Fraction, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        number_property!(
            IntlNumberRoundingSchema::MINIMUM_SIGNIFICANT,
            "minimumSignificantDigits"
        );
        number_property!(
            IntlNumberRoundingSchema::MAXIMUM_SIGNIFICANT,
            "maximumSignificantDigits"
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        emit_domain_is(&selected.grouping, Grouping::Never, function);
        self.open_frame(ControlFrameKind::If, function);
        value.set_scalar(ScalarValue::Boolean(false), function);
        self.emit_intl_number_append_result_property(&object, "useGrouping", &value, function)?;
        function.instruction(&Instruction::Else);
        self.emit_nf_resolved_choice(
            &object,
            "useGrouping",
            &selected.grouping,
            [
                ("auto", Grouping::Auto),
                ("always", Grouping::Always),
                ("min2", Grouping::MinTwo),
            ],
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_nf_resolved_choice(
            &object,
            "notation",
            &selected.notation,
            NotationOption::ALL.iter().map(|v| (v.name(), *v)),
            function,
        )?;
        emit_domain_is(&selected.notation, NotationOption::Compact, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_nf_resolved_choice(
            &object,
            "compactDisplay",
            &selected.compact_display,
            CompactDisplay::ALL.iter().map(|v| (v.name(), Some(*v))),
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_nf_resolved_choice(
            &object,
            "signDisplay",
            &selected.sign,
            SignDisplay::ALL.iter().map(|v| (v.name(), *v)),
            function,
        )?;
        number_property!(
            IntlNumberRoundingSchema::ROUNDING_INCREMENT,
            "roundingIncrement"
        );
        self.emit_nf_resolved_choice(
            &object,
            "roundingMode",
            &mode,
            RoundingMode::ALL.iter().map(|v| (v.name(), *v)),
            function,
        )?;
        self.emit_nf_resolved_choice(
            &object,
            "roundingPriority",
            &precision,
            [
                (RoundingPriority::Auto.name(), NumberPrecisionKind::Fraction),
                (
                    RoundingPriority::Auto.name(),
                    NumberPrecisionKind::Significant,
                ),
                (
                    RoundingPriority::MorePrecision.name(),
                    NumberPrecisionKind::More,
                ),
                (
                    RoundingPriority::LessPrecision.name(),
                    NumberPrecisionKind::Less,
                ),
            ],
            function,
        )?;
        self.emit_nf_resolved_choice(
            &object,
            "trailingZeroDisplay",
            &trailing,
            TrailingZeroDisplay::ALL.iter().map(|v| (v.name(), *v)),
            function,
        )?;
        self.completion().initialize(function);
        self.completion()
            .value()
            .set_reference(&object, schema, function);
        schema.release_i64_local(bits, function);
        schema.release_i32_local(count, function);
        value.clear(function);
        trailing.clear(schema, function);
        mode.clear(schema, function);
        precision.clear(schema, function);
        rounding.clear(function);
        selected.clear(schema, function);
        object.clear(function);
        record.clear(function);
        Ok(())
    }
    pub(crate) fn emit_intl_number_format_supported_locales_of(
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
        let response = self.emit_intl_number_provider_call(
            IntlNumberProviderRequest::Supported {
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
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
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
}
