use super::*;
use crate::builtins::intl_provider_wire::IntlNumberProviderRequest;
use crate::functions::ArgumentListConstruction;

enum NfInputs<'a> {
    Scalar(&'a IntlMathematicalValueLocals),
    Range {
        start: &'a IntlMathematicalValueLocals,
        end: &'a IntlMathematicalValueLocals,
    },
}
impl FunctionBuilder<'_> {
    fn emit_intl_number_provider_format(
        &mut self,
        record: &GcLocal<IntlNumberFormatObject>,
        inputs: NfInputs<'_>,
        mode: NfFormatMode,
        out: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let (request, range) = match inputs {
            NfInputs::Scalar(input) => (IntlNumberProviderRequest::Scalar { record, input }, false),
            NfInputs::Range { start, end } => (
                IntlNumberProviderRequest::Range { record, start, end },
                true,
            ),
        };
        let response = self.emit_intl_number_provider_call(request, function)?;
        let schema = self.runtime_schema();
        let reader = response.reader(schema, function);
        let count = schema.reserve_i64_local(function);
        let index = schema.reserve_i64_local(function);
        let code = schema.reserve_i64_local(function);
        let recognized = schema.reserve_i32_local(function);
        let approximately = schema.reserve_i32_local(function);
        reader.read_u64(count, schema, function);
        reader.require_records(count, if range { 24 } else { 16 }, function);
        let text = match mode {
            NfFormatMode::String => Some(
                schema
                    .reserve_gc_local(function)
                    .initialize(self.emit_interned_string_reference("", function)?, function),
            ),
            NfFormatMode::Parts => None,
        };
        let parts = match mode {
            NfFormatMode::Parts => Some(ArgumentListConstruction::new(self, function)),
            NfFormatMode::String => None,
        };
        let part_type = schema
            .reserve_gc_local(function)
            .initialize(self.emit_interned_string_reference("", function)?, function);
        let source = schema
            .reserve_gc_local(function)
            .initialize(self.emit_interned_string_reference("", function)?, function);
        let value = schema.reserve_value_local(function);
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
        reader.read_u64(code, schema, function);
        set_i32(recognized, 0, function);
        set_i32(approximately, 0, function);
        if range {
            code.load(function);
            function.instruction(&Instruction::I64Const(
                scriptw_intl::NUMBER_APPROXIMATELY_SIGN_CODE as i64,
            ));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            part_type.replace(
                self.emit_interned_string_reference("approximatelySign", function)?,
                function,
            );
            set_i32(recognized, 1, function);
            set_i32(approximately, 1, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for part in NumberPartKind::ALL {
            code.load(function);
            function.instruction(&Instruction::I64Const(part.wire_code() as i64));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            part_type.replace(
                self.emit_interned_string_reference(part.name(), function)?,
                function,
            );
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
        if range {
            reader.read_u64(code, schema, function);
            approximately.load(function);
            code.load(function);
            function.instruction(&Instruction::I64Const(
                RangePartSource::Shared.wire_code() as i64
            ));
            function.instruction(&Instruction::I64Ne);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::Unreachable);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            set_i32(recognized, 0, function);
            for attribution in RangePartSource::ALL {
                code.load(function);
                function.instruction(&Instruction::I64Const(attribution.wire_code() as i64));
                function.instruction(&Instruction::I64Eq);
                self.open_frame(ControlFrameKind::If, function);
                source.replace(
                    self.emit_interned_string_reference(attribution.name(), function)?,
                    function,
                );
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
        }
        let part_text = reader.read_utf8(schema, function);
        match mode {
            NfFormatMode::String => {
                let text = text.as_ref().expect("String mode owns accumulator");
                text.replace(
                    self.emit_concat_gc_strings(text, &part_text, function),
                    function,
                );
            }
            NfFormatMode::Parts => {
                let object = self.emit_intl_number_result_object(function)?;
                value.set_reference(&part_type, schema, function);
                self.emit_intl_number_append_result_property(&object, "type", &value, function)?;
                value.set_reference(&part_text, schema, function);
                self.emit_intl_number_append_result_property(&object, "value", &value, function)?;
                if range {
                    value.set_reference(&source, schema, function);
                    self.emit_intl_number_append_result_property(
                        &object, "source", &value, function,
                    )?;
                }
                value.set_reference(&object, schema, function);
                parts
                    .as_ref()
                    .expect("Parts mode owns List")
                    .append(&value, schema, function);
                object.clear(function);
            }
        }
        part_text.clear(function);
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
        match mode {
            NfFormatMode::String => {
                let text = text.expect("String mode owns accumulator");
                out.set_reference(&text, schema, function);
                text.clear(function);
            }
            NfFormatMode::Parts => {
                let list = parts.expect("Parts mode owns List").finish(self, function);
                let array = self.emit_array_from_argument_list(&list, function)?;
                out.set_reference(&array, schema, function);
                array.clear(function);
                list.clear(function);
            }
        }
        value.clear(function);
        source.clear(function);
        part_type.clear(function);
        schema.release_i32_local(approximately, function);
        schema.release_i32_local(recognized, function);
        schema.release_i64_local(code, function);
        schema.release_i64_local(index, function);
        schema.release_i64_local(count, function);
        response.clear(function);
        Ok(())
    }
    pub(crate) fn emit_intl_number_format_bound_format(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let context = self
            .body_entry_locals()
            .and_then(|entry| entry.function_context())
            .ok_or_else(|| {
                EmitError::unsupported("NumberFormat closure lacks actual function context")
            })?;
        let capture = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionContext>()
                .field(FunctionContextSchema::BUILTIN_CAPTURE)
                .read(context, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        let record = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<BuiltinClosureCapture>()
                .field(BuiltinClosureCaptureSchema::NUMBER_FORMAT)
                .read(&capture, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        let value = schema.reserve_value_local(function);
        let out = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &value, function);
        let input = self.emit_intl_mathematical_value(&value, function)?;
        self.emit_intl_number_provider_format(
            &record,
            NfInputs::Scalar(&input),
            NfFormatMode::String,
            &out,
            function,
        )?;
        self.completion().initialize(function);
        self.completion().value().copy_from(&out, function);
        input.clear(schema, function);
        out.clear(function);
        value.clear(function);
        record.clear(function);
        capture.clear(function);
        Ok(())
    }
    pub(crate) fn emit_intl_number_format_to_parts(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let record =
            self.emit_nf_record_from_receiver(NfReceiverOperation::FormatToParts, function)?;
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let out = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &value, function);
        let input = self.emit_intl_mathematical_value(&value, function)?;
        self.emit_intl_number_provider_format(
            &record,
            NfInputs::Scalar(&input),
            NfFormatMode::Parts,
            &out,
            function,
        )?;
        self.completion().initialize(function);
        self.completion().value().copy_from(&out, function);
        input.clear(schema, function);
        out.clear(function);
        value.clear(function);
        record.clear(function);
        Ok(())
    }
    pub(crate) fn emit_intl_number_format_range(
        &mut self,
        mode: NfFormatMode,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let record =
            self.emit_nf_record_from_receiver(NfReceiverOperation::FormatRange, function)?;
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        let out = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &left, function);
        self.emit_builtin_arg_to_value(1, &right, function);
        for value in [&left, &right] {
            emit_tag_is(value, WasmRuntimeValueTag::Undefined, function);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_intl_number_type_error(NF_RANGE_UNDEFINED, function)?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        let start = self.emit_intl_mathematical_value(&left, function)?;
        let end = self.emit_intl_mathematical_value(&right, function)?;
        self.emit_intl_number_provider_format(
            &record,
            NfInputs::Range {
                start: &start,
                end: &end,
            },
            mode,
            &out,
            function,
        )?;
        self.completion().initialize(function);
        self.completion().value().copy_from(&out, function);
        end.clear(schema, function);
        start.clear(schema, function);
        out.clear(function);
        right.clear(function);
        left.clear(function);
        record.clear(function);
        Ok(())
    }
    pub(crate) fn emit_intl_number_format_getter(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let record =
            self.emit_nf_record_from_receiver(NfReceiverOperation::FormatGetter, function)?;
        let schema = self.runtime_schema();
        let bound = schema
            .reserve_gc_local::<FunctionObject, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<IntlNumberFormatObject>()
                    .field(IntlNumberFormatObjectSchema::BOUND_FORMAT)
                    .read(&record, schema, function)
                    .reference(),
                function,
            );
        bound.load(schema, function).is_null(function);
        self.open_frame(ControlFrameKind::If, function);
        let meta = self
            .functions
            .get(&StandardBuiltinId::IntlNumberFormatBoundFormat.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported("missing NumberFormat bound-format dependency")
            })?;
        let capture = schema
            .reserve_gc_local::<BuiltinClosureCapture, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<BuiltinClosureCapture>()
                    .publish(
                        BuiltinClosurePayload::IntlNumberFormat(&record),
                        schema,
                        function,
                    )
                    .nullable(),
                function,
            );
        let callable = schema.reserve_gc_local(function).initialize(
            self.emit_current_builtin_realm_closure_value(&meta, &capture, function)?,
            function,
        );
        schema
            .struct_type::<IntlNumberFormatObject>()
            .field(IntlNumberFormatObjectSchema::BOUND_FORMAT)
            .write(
                &record,
                GcOperand::nullable_reference(&callable, schema),
                schema,
                function,
            );
        bound.replace(callable.load(schema, function).nullable(), function);
        callable.clear(function);
        capture.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let callable = schema.reserve_gc_local(function).initialize(
            bound.load(schema, function).require_non_null(function),
            function,
        );
        self.completion().initialize(function);
        self.completion()
            .value()
            .set_reference(&callable, schema, function);
        callable.clear(function);
        bound.clear(function);
        record.clear(function);
        Ok(())
    }
}
