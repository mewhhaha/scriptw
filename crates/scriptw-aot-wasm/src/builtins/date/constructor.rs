//! Date construction and mutation retain complete GC records and coercions.
use super::*;
use crate::gc_types::CompletionLocals;

impl FunctionBuilder<'_> {
    fn emit_date_alloc_with_new_target(
        &mut self,
        clip: &CompletedDateClipLocals,
        new_target: &ValueLocals,
        output: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let prototype = schema.reserve_completion(function);
        self.emit_get_prototype_from_constructor(
            new_target,
            OrdinaryDefaultPrototype::Date,
            &prototype,
            function,
        )?;
        output.copy_from(&prototype, function);
        prototype.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(prototype.value()), function)?,
            function,
        );
        let time = schema.reserve_f64_local(function);
        clip.payload().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        time.store(function);
        let record = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<DateObject>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::f64_local(time),
                ),
                function,
            ),
            function,
        );
        let value = schema.reserve_value_local(function);
        value.set_reference(&record, schema, function);
        output.set_normal(&value, function);
        value.clear(function);
        record.clear(function);
        schema.release_f64_local(time, function);
        header.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        prototype.clear(function);
        Ok(())
    }

    pub(super) fn emit_date_number_coercion(
        &mut self,
        value: &ValueLocals,
        bits: I64Local,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_value_to_number_payload(value, pending, function)?;
        self.emit_date_native_throw_exit(pending, output, exit, function);
        pending.value().scalar().load(function);
        bits.store(function);
        Ok(())
    }
    pub(super) fn emit_date_native_throw_exit(
        &mut self,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) {
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        output.copy_from(pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    /// Argument presence and conversion are distinct. An explicit Undefined
    /// must be coerced; later numeric hooks run only after earlier Normal.
    fn emit_date_constructor_number_fields(
        &mut self,
        fields: &[I64Local; 7],
        value: &ValueLocals,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        for (index, default) in [f64::NAN, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]
            .into_iter()
            .enumerate()
        {
            self.emit_builtin_arg_is_present_i32(index, function);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_builtin_arg_to_value(index, value, function);
            self.emit_date_number_coercion(value, fields[index], pending, output, exit, function)?;
            function.instruction(&Instruction::Else);
            function.instruction(&Instruction::F64Const(Ieee64::from(default)));
            function.instruction(&Instruction::I64ReinterpretF64);
            fields[index].store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.emit_date_make_full_year(fields[0], fields[0], function);
        Ok(())
    }

    pub(in crate::builtins) fn emit_date_constructor(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let new_target = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let primitive = schema.reserve_completion(function);
        let output = schema.reserve_completion(function);
        output.initialize(function);
        let bits = schema.reserve_i64_local(function);
        let fields: [I64Local; 7] = std::array::from_fn(|_| schema.reserve_i64_local(function));
        self.compile_new_target_to_locals(&new_target, function)?;
        let cleanup = self.open_frame(ControlFrameKind::Block, function);
        new_target.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_date_function_call(function)?;
        output.copy_from(self.completion(), function);
        self.emit_branch_to_target(cleanup, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let time_selected = self.open_frame(ControlFrameKind::Block, function);
        self.body_entry_locals()
            .expect("Date constructor owns native entry")
            .argument_count()
            .load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_date_current_time_payload(bits, function)?;
        self.emit_branch_to_target(time_selected, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.body_entry_locals()
            .expect("Date constructor owns native entry")
            .argument_count()
            .load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_builtin_arg_to_value(0, &value, function);
        self.emit_date_record_branch(
            &value,
            function,
            |builder, function, record| {
                builder.emit_date_record_value_into(record, bits, function);
                Ok(())
            },
            |builder, function| {
                builder.emit_tagged_to_primitive_locals_pending(
                    ToPrimitiveHint::Default,
                    &value,
                    &primitive,
                    function,
                )?;
                builder.emit_date_native_throw_exit(&primitive, &output, cleanup, function);
                primitive.value().tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
                function.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, function);
                let string = schema.reserve_gc_local(function).initialize(
                    primitive
                        .value()
                        .cast_reference::<StringValue>(schema, function),
                    function,
                );
                builder.emit_date_parse_string(&string, bits, function)?;
                string.clear(function);
                function.instruction(&Instruction::Else);
                builder.emit_date_number_coercion(
                    primitive.value(),
                    bits,
                    &pending,
                    &output,
                    cleanup,
                    function,
                )?;
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                Ok(())
            },
        )?;
        self.emit_branch_to_target(time_selected, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_date_constructor_number_fields(
            &fields, &value, &pending, &output, cleanup, function,
        )?;
        let prepared_date = self.reserve_date_make_date_result(function);
        let date = self.emit_date_make_date_from_components_into(prepared_date, &fields, function);
        let prepared_utc = self.reserve_date_clip_result(function);
        let utc =
            self.emit_date_utc_time_clip_from_make_date_into(prepared_utc, &date, function)?;
        utc.payload().load(function);
        bits.store(function);
        utc.release(self, function);
        date.release(self, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let prepared = self.reserve_date_clip_result(function);
        let clip = self.emit_date_time_clip_into(prepared, bits, function);
        self.emit_date_alloc_with_new_target(&clip, &new_target, &output, function)?;
        clip.release(self, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        for field in fields.into_iter().rev() {
            schema.release_i64_local(field, function);
        }
        schema.release_i64_local(bits, function);
        output.clear(function);
        primitive.clear(function);
        pending.clear(function);
        value.clear(function);
        new_target.clear(function);
        Ok(())
    }

    pub(in crate::builtins) fn emit_date_utc(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let output = schema.reserve_completion(function);
        output.initialize(function);
        let fields: [I64Local; 7] = std::array::from_fn(|_| schema.reserve_i64_local(function));
        let cleanup = self.open_frame(ControlFrameKind::Block, function);
        self.emit_date_constructor_number_fields(
            &fields, &value, &pending, &output, cleanup, function,
        )?;
        let prepared_date = self.reserve_date_make_date_result(function);
        let date = self.emit_date_make_date_from_components_into(prepared_date, &fields, function);
        let prepared_clip = self.reserve_date_clip_result(function);
        let clip = self.emit_date_time_clip_into(prepared_clip, date.payload(), function);
        value.set_number(clip.payload(), function);
        output.set_normal(&value, function);
        clip.release(self, function);
        date.release(self, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        for field in fields.into_iter().rev() {
            schema.release_i64_local(field, function);
        }
        output.clear(function);
        pending.clear(function);
        value.clear(function);
        Ok(())
    }

    pub(in crate::builtins) fn emit_date_set_time(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let output = schema.reserve_completion(function);
        output.initialize(function);
        let bits = schema.reserve_i64_local(function);
        self.compile_this_to_locals(&receiver, function)?;
        let captured = self.emit_date_capture_value(&receiver, function)?;
        let cleanup = self.open_frame(ControlFrameKind::Block, function);
        self.emit_builtin_arg_to_value(0, &value, function);
        self.emit_date_number_coercion(&value, bits, &pending, &output, cleanup, function)?;
        let prepared = self.reserve_date_clip_result(function);
        let clip = self.emit_date_time_clip_into(prepared, bits, function);
        self.emit_date_store_clip(&captured, &clip, function);
        value.set_number(clip.payload(), function);
        output.set_normal(&value, function);
        clip.release(self, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        captured.release(self, function);
        schema.release_i64_local(bits, function);
        output.clear(function);
        pending.clear(function);
        value.clear(function);
        receiver.clear(function);
        Ok(())
    }
}
