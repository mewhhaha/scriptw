use super::*;
#[derive(Clone, Copy)]
pub(super) enum DtfFormatMode {
    String,
    Parts,
}
impl FunctionBuilder<'_> {
    pub(super) fn emit_dtf_provider_format(
        &mut self,
        record: &GcLocal<IntlDateTimeFormatObject>,
        start: &CompletedDtfInput,
        end: Option<&CompletedDtfInput>,
        mode: DtfFormatMode,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let operation = if end.is_some() {
            IntlHostOp::FormatDateTimeRangeParts
        } else {
            IntlHostOp::FormatDateTimeParts
        };
        let message = self.emit_dtf_request(operation, f);
        start.append(&message, schema, f);
        if let Some(end) = end {
            end.append(&message, schema, f);
        }
        let plan = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<IntlDateTimeFormatObject>()
                .field(IntlDateTimeFormatObjectSchema::PLAN)
                .read(record, schema, f)
                .reference(),
            f,
        );
        message.append_immutable_bytes(&plan, schema, f);
        plan.clear(f);
        let response = self.emit_dtf_provider_call(operation, message, f)?;
        let reader = response.reader(schema, f);
        let count = schema.reserve_i64_local(f);
        let index = schema.reserve_i64_local(f);
        reader.read_u64(count, schema, f);
        reader.require_records(count, if end.is_some() { 24 } else { 16 }, f);
        let string = match mode {
            DtfFormatMode::String => Some(
                schema
                    .reserve_gc_local(f)
                    .initialize(self.emit_interned_string_reference("", f)?, f),
            ),
            DtfFormatMode::Parts => None,
        };
        let list = match mode {
            DtfFormatMode::String => None,
            DtfFormatMode::Parts => Some(ArgumentListConstruction::new(self, f)),
        };
        let value = schema.reserve_value_local(f);
        let kind = GcI32DomainLocal::new(schema, DateTimePartKind::Literal, f);
        let source = GcI32DomainLocal::new(schema, DateTimeRangeSource::Shared, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        self.emit_dtf_read_domain(
            &reader,
            &kind,
            DateTimePartKind::ALL.iter().map(|v| (*v, v.wire_code())),
            f,
        );
        if end.is_some() {
            self.emit_dtf_read_domain(
                &reader,
                &source,
                DateTimeRangeSource::ALL.iter().map(|v| (*v, v.wire_code())),
                f,
            );
        }
        let text = reader.read_utf8(schema, f);
        match mode {
            DtfFormatMode::String => {
                let string = string
                    .as_ref()
                    .expect("String output owns concatenation root");
                string.replace(self.emit_concat_gc_strings(string, &text, f), f);
            }
            DtfFormatMode::Parts => {
                let object = self.emit_intl_number_result_object(f)?;
                for part in DateTimePartKind::ALL {
                    emit_domain_is(&kind, *part, f);
                    self.open_frame(ControlFrameKind::If, f);
                    let name = schema
                        .reserve_gc_local(f)
                        .initialize(self.emit_interned_string_reference(part.as_str(), f)?, f);
                    value.set_reference(&name, schema, f);
                    name.clear(f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                }
                self.emit_intl_number_append_result_property(&object, "type", &value, f)?;
                value.set_reference(&text, schema, f);
                self.emit_intl_number_append_result_property(&object, "value", &value, f)?;
                if end.is_some() {
                    for selected in DateTimeRangeSource::ALL {
                        emit_domain_is(&source, *selected, f);
                        self.open_frame(ControlFrameKind::If, f);
                        let name = schema.reserve_gc_local(f).initialize(
                            self.emit_interned_string_reference(selected.as_str(), f)?,
                            f,
                        );
                        value.set_reference(&name, schema, f);
                        name.clear(f);
                        self.pop_control(ControlFrameKind::If);
                        f.instruction(&Instruction::End);
                    }
                    self.emit_intl_number_append_result_property(&object, "source", &value, f)?;
                }
                value.set_reference(&object, schema, f);
                list.as_ref()
                    .expect("Parts output owns private list")
                    .append(&value, schema, f);
                object.clear(f);
            }
        }
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
        if let Some(list) = list {
            let values = list.finish(self, f);
            let array = self.emit_array_from_argument_list(&values, f)?;
            out.set_reference(&array, schema, f);
            array.clear(f);
            values.clear(f);
        }
        if let Some(string) = string {
            out.set_reference(&string, schema, f);
            string.clear(f);
        }
        source.clear(schema, f);
        kind.clear(schema, f);
        value.clear(f);
        schema.release_i64_local(index, f);
        schema.release_i64_local(count, f);
        response.clear(f);
        Ok(())
    }
    pub(crate) fn emit_intl_date_time_format_bound_format(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let context = self
            .body_entry_locals()
            .and_then(|e| e.function_context())
            .ok_or_else(|| EmitError::unsupported("DateTimeFormat bound closure lacks context"))?;
        let capture = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<FunctionContext>()
                .field(FunctionContextSchema::BUILTIN_CAPTURE)
                .read(context, schema, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let record = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<BuiltinClosureCapture>()
                .field(BuiltinClosureCaptureSchema::DATE_TIME_FORMAT)
                .read(&capture, schema, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let input = self.emit_dtf_single_input(&record, f)?;
        let out = schema.reserve_value_local(f);
        self.emit_dtf_provider_format(&record, &input, None, DtfFormatMode::String, &out, f)?;
        self.completion().initialize(f);
        self.completion().value().copy_from(&out, f);
        out.clear(f);
        input.clear(schema, f);
        record.clear(f);
        capture.clear(f);
        Ok(())
    }
    pub(crate) fn emit_intl_date_time_format_format_to_parts(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let record = self.emit_dtf_record_from_receiver(DtfReceiverOperation::FormatToParts, f)?;
        let schema = self.runtime_schema();
        let input = self.emit_dtf_single_input(&record, f)?;
        let out = schema.reserve_value_local(f);
        self.emit_dtf_provider_format(&record, &input, None, DtfFormatMode::Parts, &out, f)?;
        self.completion().initialize(f);
        self.completion().value().copy_from(&out, f);
        out.clear(f);
        input.clear(schema, f);
        record.clear(f);
        Ok(())
    }
    fn emit_dtf_format_range(
        &mut self,
        mode: DtfFormatMode,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let operation = match mode {
            DtfFormatMode::String => DtfReceiverOperation::FormatRange,
            DtfFormatMode::Parts => DtfReceiverOperation::FormatRangeToParts,
        };
        let record = self.emit_dtf_record_from_receiver(operation, f)?;
        let schema = self.runtime_schema();
        let (start, end) = self.emit_dtf_range_inputs(&record, f)?;
        let out = schema.reserve_value_local(f);
        self.emit_dtf_provider_format(&record, &start, Some(&end), mode, &out, f)?;
        self.completion().initialize(f);
        self.completion().value().copy_from(&out, f);
        out.clear(f);
        end.clear(schema, f);
        start.clear(schema, f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_intl_date_time_format_format_range(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_dtf_format_range(DtfFormatMode::String, f)
    }
    pub(crate) fn emit_intl_date_time_format_format_range_to_parts(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_dtf_format_range(DtfFormatMode::Parts, f)
    }
    pub(crate) fn emit_intl_date_time_format_supported_locales_of(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &value, f);
        let locales = self.emit_intl_canonical_locale_list(&value, f)?;
        self.emit_builtin_arg_to_value(1, &value, f);
        self.emit_intl_number_options_object(&value, f)?;
        let matcher = GcI32DomainLocal::new(schema, DateTimeLocaleMatcher::BestFit, f);
        self.emit_intl_number_choice_option(
            &value,
            IntlErrorOption::LocaleMatcher,
            DateTimeLocaleMatcher::ALL
                .iter()
                .map(|v| (v.option_name(), *v)),
            DateTimeLocaleMatcher::BestFit,
            &matcher,
            f,
        )?;
        let message = self.emit_dtf_request(IntlHostOp::SupportedDateTimeLocales, f);
        dtf_append_domain(&message, &matcher, schema, f);
        self.emit_intl_wire_canonical_locales(&message, &locales, f)?;
        let response =
            self.emit_dtf_provider_call(IntlHostOp::SupportedDateTimeLocales, message, f)?;
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
        let values = list.finish(self, f);
        let array = self.emit_array_from_argument_list(&values, f)?;
        self.completion().initialize(f);
        self.completion().value().set_reference(&array, schema, f);
        array.clear(f);
        values.clear(f);
        schema.release_i64_local(index, f);
        schema.release_i64_local(count, f);
        response.clear(f);
        matcher.clear(schema, f);
        locales.clear(f);
        value.clear(f);
        Ok(())
    }
}
