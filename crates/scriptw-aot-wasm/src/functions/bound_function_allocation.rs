use super::*;
use crate::emit::AccessorThrowRouting;
use crate::gc_types::{
    BoundFunction, CompletionLocals, GcLocal, GcOperand, I32Local, Nullable, StoredValue,
    StringValue, ValueArray, ValueLocals,
};

impl FunctionBuilder<'_> {
    /// The native bind entry captures argument zero without this adaptation.
    /// The BoundFunction is an exotic record with no independent Realm slot.
    pub(crate) fn emit_alloc_bound_function_for_bind(
        &mut self,
        target: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        let bound_this = schema.reserve_value_local(function);
        let bound_value = schema.reserve_value_local(function);
        let argument_count = schema.reserve_i32_local(function);
        let constructable = schema.reserve_i32_local(function);
        pending.initialize(function);
        result.initialize(function);
        bound_this.set_undefined(function);
        bound_value.set_undefined(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_callable_i32(target, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::FUNCTION_PROTOTYPE_BIND_RECEIVER_IS_NOT_CALLABLE,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // BoundFunctionCreate performs target.[[GetPrototypeOf]] before bind
        // inspects either metadata property. A Proxy throw remains whole.
        self.emit_object_get_prototype_of(target, &pending, function)?;
        self.emit_bind_metadata_abrupt_exit(&pending, result, exit, function);
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(pending.value()), function)?,
            function,
        );
        self.emit_builtin_arg_to_value(0, &bound_this, function);
        self.emit_is_constructor_i32(target, function);
        constructable.store(function);
        let stored_target = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(target, function),
            function,
        );
        let stored_this = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&bound_this, function),
            function,
        );
        let bound = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<BoundFunction>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&stored_target, schema),
                    GcOperand::reference(&stored_this, schema),
                    GcOperand::reference(arguments, schema),
                    GcOperand::boolean_local(constructable),
                ),
                function,
            ),
            function,
        );
        stored_this.clear(function);
        stored_target.clear(function);

        bound_value.set_reference(&bound, schema, function);
        schema
            .array_type::<ValueArray>()
            .length(arguments, schema, function);
        argument_count.store(function);
        self.emit_copy_function_name_and_length(
            target,
            &bound_value,
            Some("bound "),
            argument_count,
            &pending,
            function,
        )?;
        self.emit_bind_metadata_abrupt_exit(&pending, result, exit, function);
        result.set_normal(&bound_value, function);
        bound.clear(function);
        header.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(constructable, function);
        schema.release_i32_local(argument_count, function);
        bound_value.clear(function);
        bound_this.clear(function);
        pending.clear(function);
        Ok(())
    }

    /// CopyNameAndLength returns the whole abrupt completion. The caller owns
    /// whether it is preserved (bind) or replaced (a ShadowRealm boundary).
    /// `prefix` already includes its separating space when present.
    pub(crate) fn emit_copy_function_name_and_length(
        &mut self,
        target: &ValueLocals,
        destination: &ValueLocals,
        prefix: Option<&str>,
        argument_count: I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        let metadata = schema.reserve_value_local(function);
        let length_bits = schema.reserve_i64_local(function);
        let length_key = self.emit_function_string_key("length", function)?;
        let name_key = self.emit_function_string_key("name", function)?;
        pending.initialize(function);
        result.initialize(function);
        function.instruction(&Instruction::I64Const(0));
        length_bits.store(function);
        metadata.set_number(length_bits, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);

        // Retain the caller's cleanup route while acquiring the private own
        // descriptor. A callable Proxy still performs its trap exactly once.
        let descriptor =
            self.emit_object_get_own_property(target, &length_key, &pending, function)?;
        self.emit_bind_metadata_abrupt_exit(&pending, result, exit, function);
        descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        descriptor.clear(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_read_with_throw_routing(
            target,
            target,
            &length_key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        self.emit_bind_metadata_abrupt_exit(&pending, result, exit, function);
        pending.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        // NaN, both zeroes, negative finite values and -Infinity retain +0.
        // Truncation and subtraction preserve +Infinity without integer casts.
        pending.value().scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Gt);
        self.open_frame(ControlFrameKind::If, function);
        pending.value().scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        argument_count.load(function);
        function.instruction(&Instruction::F64ConvertI32U);
        function.instruction(&Instruction::F64Sub);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Max);
        function.instruction(&Instruction::I64ReinterpretF64);
        length_bits.store(function);
        metadata.set_number(length_bits, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_object_define_data_with_configurable(
            destination,
            &length_key,
            &metadata,
            false,
            false,
            true,
            &pending,
            function,
        )?;
        self.emit_bind_metadata_abrupt_exit(&pending, result, exit, function);

        self.emit_object_read_with_throw_routing(
            target,
            target,
            &name_key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        self.emit_bind_metadata_abrupt_exit(&pending, result, exit, function);
        let name = schema
            .reserve_gc_local::<StringValue, Nullable>(function)
            .initialize_null(schema, function);
        pending.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        name.replace(
            pending
                .value()
                .cast_reference::<StringValue>(schema, function)
                .nullable(),
            function,
        );
        function.instruction(&Instruction::Else);
        name.replace(
            self.emit_interned_string_reference("", function)?
                .nullable(),
            function,
        );
        function.instruction(&Instruction::End);
        let final_name = schema.reserve_gc_local(function).initialize(
            name.load(schema, function).require_non_null(function),
            function,
        );
        name.clear(function);
        if let Some(prefix) = prefix {
            let prefix = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(prefix, function)?,
                function,
            );
            final_name.replace(
                self.emit_concat_gc_strings(&prefix, &final_name, function),
                function,
            );
            prefix.clear(function);
        }
        metadata.set_reference(&final_name, schema, function);
        final_name.clear(function);
        self.emit_object_define_data_with_configurable(
            destination,
            &name_key,
            &metadata,
            false,
            false,
            true,
            &pending,
            function,
        )?;
        self.emit_bind_metadata_abrupt_exit(&pending, result, exit, function);
        metadata.set_undefined(function);
        result.set_normal(&metadata, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        name_key.clear(function);
        length_key.clear(function);
        schema.release_i64_local(length_bits, function);
        metadata.clear(function);
        pending.clear(function);
        Ok(())
    }

    pub(super) fn emit_bind_metadata_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        result: &CompletionLocals,
        exit: crate::emit::ControlTarget,
        function: &mut Function,
    ) {
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }
}
