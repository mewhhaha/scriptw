//! Explicit Realm-bound ToObject, retaining the whole conversion Completion.
use super::*;
use crate::runtime_helpers::HelperParameters;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_value_to_object_locals(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let realm = self.load_current_realm(function);
        self.emit_value_to_object_in_realm_locals(&realm, input, result, function)?;
        realm.clear(function);
        Ok(())
    }

    pub(crate) fn emit_value_to_current_function_realm_object_locals(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let slot = schema.reserve_gc_local(function);
        let realm = slot.initialize(self.emit_current_function_realm(function), function);
        self.emit_value_to_object_in_realm_locals(&realm, input, result, function)?;
        realm.clear(function);
        Ok(())
    }

    fn emit_value_to_object_in_realm_locals(
        &mut self,
        realm: &GcLocal<crate::gc_types::RealmRecord>,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        // ToObject is the identity on an object; only a primitive needs the
        // outlined owner and its Realm.
        self.emit_is_heap_object_like_tag_i32(input.tag(), function);
        function.instruction(&Instruction::If(BlockType::Empty));
        result.set_normal(input, function);
        function.instruction(&Instruction::Else);
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::ValueToObjectArguments::new(realm, input),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_value_to_object_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ValueToObject);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::ValueToObjectParameters>(&mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_value_to_object_in_realm_kernel(
            &parameters.realm,
            &parameters.input,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Install StringCreate's own UTF-16 length before the wrapper is exposed.
    /// The fresh header receives a data property without invoking its prototype.
    pub(crate) fn emit_initialize_string_object_length(
        &mut self,
        header: &GcLocal<crate::gc_types::OrdinaryObject>,
        string: &GcLocal<StringValue>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(string, schema, function)
                .reference(),
            function,
        );
        let length = schema.reserve_value_local(function);
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        function.instruction(&Instruction::F64ConvertI32U);
        function.instruction(&Instruction::I64ReinterpretF64);
        length.scalar().store(function);
        length.set_number(length.scalar(), function);
        let name = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("length", function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &name, function);
        self.emit_object_append_data_property_with_flags(
            header, &key, &length, false, false, false, function,
        )?;
        key.clear(function);
        name.clear(function);
        length.clear(function);
        units.clear(function);
        Ok(())
    }

    fn emit_value_to_object_in_realm_kernel(
        &mut self,
        realm: &GcLocal<crate::gc_types::RealmRecord>,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        let boxed_value = schema.reserve_value_local(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_heap_object_like_tag_i32(input.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(input, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        for (tag, slot) in [
            (
                WasmRuntimeValueTag::Number,
                crate::functions::NonArrayRealmIntrinsicSlot::NumberPrototype,
            ),
            (
                WasmRuntimeValueTag::String,
                crate::functions::NonArrayRealmIntrinsicSlot::StringPrototype,
            ),
            (
                WasmRuntimeValueTag::Boolean,
                crate::functions::NonArrayRealmIntrinsicSlot::BooleanPrototype,
            ),
            (
                WasmRuntimeValueTag::Symbol,
                crate::functions::NonArrayRealmIntrinsicSlot::SymbolPrototype,
            ),
            (
                WasmRuntimeValueTag::BigInt,
                crate::functions::NonArrayRealmIntrinsicSlot::BigIntPrototype,
            ),
        ] {
            input.tag().load(function);
            function.instruction(&Instruction::I32Const(tag as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_load_non_array_realm_intrinsic(realm, slot, &prototype, function);
            let header = schema.reserve_gc_local(function).initialize(
                self.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?,
                function,
            );
            if tag == WasmRuntimeValueTag::String {
                let string = schema.reserve_gc_local(function).initialize(
                    input.cast_reference::<StringValue>(schema, function),
                    function,
                );
                self.emit_initialize_string_object_length(&header, &string, function)?;
                string.clear(function);
            }
            let primitive = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<crate::gc_types::StoredValue>()
                    .from_value(input, function),
                function,
            );
            let boxed = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<crate::gc_types::PrimitiveBox>()
                    .construct(
                        (
                            crate::gc_types::GcOperand::reference(&header, schema),
                            crate::gc_types::GcOperand::reference(&primitive, schema),
                        ),
                        function,
                    ),
                function,
            );
            boxed_value.set_reference(&boxed, schema, function);
            result.set_normal(&boxed_value, function);
            boxed.clear(function);
            primitive.clear(function);
            header.clear(function);
            self.emit_branch_to_target(exit, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }

        self.emit_load_non_array_realm_intrinsic(
            realm,
            crate::functions::NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
            &prototype,
            function,
        );
        self.emit_throw_runtime_error_with_prototype(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_CONVERT_UNDEFINED_OR_NULL_TO_OBJECT,
            &prototype,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        boxed_value.clear(function);
        prototype.clear(function);
        Ok(())
    }
}
