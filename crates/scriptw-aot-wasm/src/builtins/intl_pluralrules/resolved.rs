use super::*;
use crate::functions::ArgumentListConstruction;
impl FunctionBuilder<'_> {
    fn emit_plural_resolved_choice<V: GcI32Constant + Copy>(
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
    pub(crate) fn emit_intl_plural_rules_resolved_options(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let record = self.emit_plural_record_from_receiver(f)?;
        let schema = self.runtime_schema();
        let pr = schema.struct_type::<IntlPluralRulesObject>();
        let object = self.emit_intl_number_result_object(f)?;
        let value = schema.reserve_value_local(f);
        let locale = schema.reserve_gc_local(f).initialize(
            pr.field(IntlPluralRulesObjectSchema::LOCALE)
                .read(&record, schema, f)
                .reference(),
            f,
        );
        value.set_reference(&locale, schema, f);
        self.emit_intl_number_append_result_property(&object, "locale", &value, f)?;
        locale.clear(f);
        let kind = GcI32DomainLocal::new(schema, PluralType::Cardinal, f);
        pr.field(IntlPluralRulesObjectSchema::TYPE)
            .read(&record, schema, f)
            .store_domain(&kind, f);
        self.emit_plural_resolved_choice(
            &object,
            "type",
            &kind,
            PluralType::ALL.iter().map(|v| (v.name(), *v)),
            f,
        )?;
        kind.clear(schema, f);
        let notation = GcI32DomainLocal::new(schema, NotationOption::Standard, f);
        pr.field(IntlPluralRulesObjectSchema::NOTATION)
            .read(&record, schema, f)
            .store_domain(&notation, f);
        self.emit_plural_resolved_choice(
            &object,
            "notation",
            &notation,
            NotationOption::ALL.iter().map(|v| (v.name(), *v)),
            f,
        )?;
        emit_domain_is(&notation, NotationOption::Compact, f);
        self.open_frame(ControlFrameKind::If, f);
        let display = GcI32DomainLocal::new(schema, None, f);
        pr.field(IntlPluralRulesObjectSchema::COMPACT_DISPLAY)
            .read(&record, schema, f)
            .store_domain(&display, f);
        self.emit_plural_resolved_choice(
            &object,
            "compactDisplay",
            &display,
            CompactDisplay::ALL.iter().map(|v| (v.name(), Some(*v))),
            f,
        )?;
        display.clear(schema, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        notation.clear(schema, f);
        let rounding = schema.reserve_gc_local(f).initialize(
            pr.field(IntlPluralRulesObjectSchema::ROUNDING)
                .read(&record, schema, f)
                .reference(),
            f,
        );
        let rules = schema.struct_type::<IntlNumberRounding>();
        let count = schema.reserve_i32_local(f);
        let bits = schema.reserve_i64_local(f);
        macro_rules! number_property {
            ($field:expr,$name:literal) => {{
                rules
                    .field($field)
                    .read(&rounding, schema, f)
                    .store(count, f);
                count.load(f);
                f.instruction(&Instruction::I32Const(0));
                f.instruction(&Instruction::I32LtS);
                self.open_frame(ControlFrameKind::If, f);
                f.instruction(&Instruction::Unreachable);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                count.load(f);
                f.instruction(&Instruction::F64ConvertI32U);
                f.instruction(&Instruction::I64ReinterpretF64);
                bits.store(f);
                value.set_number(bits, f);
                self.emit_intl_number_append_result_property(&object, $name, &value, f)?;
            }};
        }
        number_property!(
            IntlNumberRoundingSchema::MINIMUM_INTEGER,
            "minimumIntegerDigits"
        );
        let precision = GcI32DomainLocal::new(schema, NumberPrecisionKind::Fraction, f);
        rules
            .field(IntlNumberRoundingSchema::PRECISION)
            .read(&rounding, schema, f)
            .store_domain(&precision, f);
        emit_domain_is(&precision, NumberPrecisionKind::Significant, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        number_property!(
            IntlNumberRoundingSchema::MINIMUM_FRACTION,
            "minimumFractionDigits"
        );
        number_property!(
            IntlNumberRoundingSchema::MAXIMUM_FRACTION,
            "maximumFractionDigits"
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        emit_domain_is(&precision, NumberPrecisionKind::Fraction, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        number_property!(
            IntlNumberRoundingSchema::MINIMUM_SIGNIFICANT,
            "minimumSignificantDigits"
        );
        number_property!(
            IntlNumberRoundingSchema::MAXIMUM_SIGNIFICANT,
            "maximumSignificantDigits"
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let mask = schema.reserve_i32_local(f);
        pr.field(IntlPluralRulesObjectSchema::CATEGORIES)
            .read(&record, schema, f)
            .store(mask, f);
        let list = ArgumentListConstruction::new(self, f);
        for category in PluralCategory::ALL {
            mask.load(f);
            f.instruction(&Instruction::I32Const(1 << category.index()));
            f.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, f);
            let text = schema
                .reserve_gc_local(f)
                .initialize(self.emit_interned_string_reference(category.name(), f)?, f);
            value.set_reference(&text, schema, f);
            list.append(&value, schema, f);
            text.clear(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        let list = list.finish(self, f);
        let categories = self.emit_array_from_argument_list(&list, f)?;
        value.set_reference(&categories, schema, f);
        self.emit_intl_number_append_result_property(&object, "pluralCategories", &value, f)?;
        categories.clear(f);
        list.clear(f);
        schema.release_i32_local(mask, f);
        number_property!(
            IntlNumberRoundingSchema::ROUNDING_INCREMENT,
            "roundingIncrement"
        );
        let mode = GcI32DomainLocal::new(schema, RoundingMode::HalfExpand, f);
        rules
            .field(IntlNumberRoundingSchema::ROUNDING_MODE)
            .read(&rounding, schema, f)
            .store_domain(&mode, f);
        self.emit_plural_resolved_choice(
            &object,
            "roundingMode",
            &mode,
            RoundingMode::ALL.iter().map(|v| (v.name(), *v)),
            f,
        )?;
        mode.clear(schema, f);
        self.emit_plural_resolved_choice(
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
            f,
        )?;
        let trailing = GcI32DomainLocal::new(schema, TrailingZeroDisplay::Auto, f);
        rules
            .field(IntlNumberRoundingSchema::TRAILING_ZERO)
            .read(&rounding, schema, f)
            .store_domain(&trailing, f);
        self.emit_plural_resolved_choice(
            &object,
            "trailingZeroDisplay",
            &trailing,
            TrailingZeroDisplay::ALL.iter().map(|v| (v.name(), *v)),
            f,
        )?;
        trailing.clear(schema, f);
        self.completion().initialize(f);
        self.completion().value().set_reference(&object, schema, f);
        precision.clear(schema, f);
        schema.release_i64_local(bits, f);
        schema.release_i32_local(count, f);
        rounding.clear(f);
        value.clear(f);
        object.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_intl_plural_rules_supported_locales_of(
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
        let response = self.emit_plural_provider_call(
            PluralProviderRequest::Supported {
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
