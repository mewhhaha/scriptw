use super::*;
use crate::gc_types::{
    CodeUnitArray, FunctionObject, GcLocal, GcStackReference, Nullable, StringConstruction,
    StringValue, StringValueSchema, SymbolValue, SymbolValueSchema,
};
use crate::operations::PropertyKeyLocals;
use crate::runtime_helpers::HelperParameters;

#[derive(Clone, Copy)]
pub(crate) enum FunctionNamePrefix {
    None,
    Getter,
    Setter,
}

impl FunctionNamePrefix {
    const fn encoding(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Getter => 1,
            Self::Setter => 2,
        }
    }
}

impl FunctionBuilder<'_> {
    /// Concatenation copies exact UTF-16 units into one immutable StringValue.
    pub(crate) fn emit_concat_gc_strings(
        &self,
        left: &GcLocal<StringValue>,
        right: &GcLocal<StringValue>,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        let schema = self.runtime_schema();
        let units = schema.array_type::<CodeUnitArray>();
        let left_units = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(left, schema, function)
                .reference(),
            function,
        );
        let right_units = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(right, schema, function)
                .reference(),
            function,
        );
        let left_length = schema.reserve_i32_local(function);
        let right_length = schema.reserve_i32_local(function);
        let total = schema.reserve_i32_local(function);
        let destination_index = schema.reserve_i32_local(function);
        units.length(&left_units, schema, function);
        left_length.store(function);
        units.length(&right_units, schema, function);
        right_length.store(function);
        left_length.load(function);
        right_length.load(function);
        function.instruction(&Instruction::I32Add);
        total.store(function);
        total.load(function);
        left_length.load(function);
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        // Strings have no identity: an empty operand leaves the other
        // operand as the whole result without copying it.
        let result = schema
            .reserve_gc_local(function)
            .initialize(right.load(schema, function), function);
        right_length.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        result.replace(left.load(schema, function), function);
        function.instruction(&Instruction::Else);
        left_length.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let output = StringConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            total,
            function,
        );
        function.instruction(&Instruction::I32Const(0));
        destination_index.store(function);
        output.copy_units_from(
            destination_index,
            &left_units,
            left_length,
            schema,
            function,
        );
        left_length.load(function);
        destination_index.store(function);
        output.copy_units_from(
            destination_index,
            &right_units,
            right_length,
            schema,
            function,
        );
        result.replace(output.publish(schema, function), function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i32_local(destination_index, function);
        schema.release_i32_local(total, function);
        schema.release_i32_local(right_length, function);
        schema.release_i32_local(left_length, function);
        right_units.clear(function);
        left_units.clear(function);
        let value = result.load(schema, function);
        result.clear(function);
        value
    }

    /// SymbolDescriptiveString is distinct from implicit ToString, which
    /// continues to reject a Symbol primitive.
    pub(crate) fn emit_symbol_descriptive_string(
        &mut self,
        symbol: &GcLocal<SymbolValue>,
        function: &mut Function,
    ) -> Result<GcStackReference<StringValue>, EmitError> {
        let schema = self.runtime_schema();
        let description = schema
            .reserve_gc_local::<StringValue, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<SymbolValue>()
                    .field(SymbolValueSchema::DESCRIPTION)
                    .read(symbol, schema, function)
                    .reference(),
                function,
            );
        let output = schema
            .reserve_gc_local::<StringValue, Nullable>(function)
            .initialize_null(schema, function);
        description.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        output.replace(
            self.emit_interned_string_reference("Symbol()", function)?
                .nullable(),
            function,
        );
        function.instruction(&Instruction::Else);
        let text = schema.reserve_gc_local(function).initialize(
            description
                .load(schema, function)
                .require_non_null(function),
            function,
        );
        let prefix = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("Symbol(", function)?,
            function,
        );
        let partial = schema.reserve_gc_local(function).initialize(
            self.emit_concat_gc_strings(&prefix, &text, function),
            function,
        );
        let suffix = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(")", function)?,
            function,
        );
        output.replace(
            self.emit_concat_gc_strings(&partial, &suffix, function)
                .nullable(),
            function,
        );
        suffix.clear(function);
        partial.clear(function);
        prefix.clear(function);
        text.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let value = output.load(schema, function).require_non_null(function);
        output.clear(function);
        description.clear(function);
        Ok(value)
    }

    /// One R kernel owns symbol bracketing and exact UTF-16 prefix copying.
    /// The caller supplies a normalized key and a closed prefix encoding.
    pub(crate) fn compile_function_name_from_key_helper(&mut self) -> Result<Function, EmitError> {
        use crate::runtime_helpers::{FunctionNameFromKeyParameters, RuntimeHelperId};
        let mut function = self.begin_helper_body(RuntimeHelperId::FunctionNameFromKey);
        let parameters = self.helper_parameters::<FunctionNameFromKeyParameters>(&mut function);
        let result = self.emit_function_name_from_key_inner(
            &parameters.key,
            parameters.prefix,
            &mut function,
        )?;
        parameters.release(&mut function);
        let _result = result;
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_function_name_from_key_inner(
        &mut self,
        property_key: &PropertyKeyLocals,
        prefix: crate::gc_types::I32Local,
        function: &mut Function,
    ) -> Result<GcStackReference<StringValue>, EmitError> {
        let schema = self.runtime_schema();
        // Unsigned comparison also rejects every negative encoding.
        prefix.load(function);
        function.instruction(&Instruction::I32Const(
            FunctionNamePrefix::Setter.encoding(),
        ));
        function.instruction(&Instruction::I32GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        let name = schema
            .reserve_gc_local::<StringValue, Nullable>(function)
            .initialize_null(schema, function);
        property_key.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let symbol = schema.reserve_gc_local(function).initialize(
            property_key
                .value()
                .cast_reference::<SymbolValue>(schema, function),
            function,
        );
        let description = schema
            .reserve_gc_local::<StringValue, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<SymbolValue>()
                    .field(SymbolValueSchema::DESCRIPTION)
                    .read(&symbol, schema, function)
                    .reference(),
                function,
            );
        description.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        name.replace(
            self.emit_interned_string_reference("", function)?
                .nullable(),
            function,
        );
        function.instruction(&Instruction::Else);
        let description_value = schema.reserve_gc_local(function).initialize(
            description
                .load(schema, function)
                .require_non_null(function),
            function,
        );
        let open = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("[", function)?,
            function,
        );
        let partial = schema.reserve_gc_local(function).initialize(
            self.emit_concat_gc_strings(&open, &description_value, function),
            function,
        );
        let close = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("]", function)?,
            function,
        );
        name.replace(
            self.emit_concat_gc_strings(&partial, &close, function)
                .nullable(),
            function,
        );
        close.clear(function);
        partial.clear(function);
        open.clear(function);
        description_value.clear(function);
        function.instruction(&Instruction::End);
        description.clear(function);
        symbol.clear(function);
        function.instruction(&Instruction::Else);
        name.replace(
            property_key
                .value()
                .cast_reference::<StringValue>(schema, function)
                .nullable(),
            function,
        );
        function.instruction(&Instruction::End);
        let final_name = schema.reserve_gc_local(function).initialize(
            name.load(schema, function).require_non_null(function),
            function,
        );
        prefix.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let prefix_string = schema
            .reserve_gc_local::<StringValue, Nullable>(function)
            .initialize_null(schema, function);
        prefix.load(function);
        function.instruction(&Instruction::I32Const(
            FunctionNamePrefix::Getter.encoding(),
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        prefix_string.replace(
            self.emit_interned_string_reference("get ", function)?
                .nullable(),
            function,
        );
        function.instruction(&Instruction::Else);
        prefix_string.replace(
            self.emit_interned_string_reference("set ", function)?
                .nullable(),
            function,
        );
        function.instruction(&Instruction::End);
        let prefix_value = schema.reserve_gc_local(function).initialize(
            prefix_string
                .load(schema, function)
                .require_non_null(function),
            function,
        );
        final_name.replace(
            self.emit_concat_gc_strings(&prefix_value, &final_name, function),
            function,
        );
        prefix_value.clear(function);
        prefix_string.clear(function);
        function.instruction(&Instruction::End);
        let result = final_name.load(schema, function);
        final_name.clear(function);
        name.clear(function);
        Ok(result)
    }

    pub(crate) fn emit_set_function_name(
        &mut self,
        callable: &GcLocal<FunctionObject>,
        property_key: &PropertyKeyLocals,
        prefix: FunctionNamePrefix,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let prefix_local = schema.reserve_i32_local(function);
        prefix_local.set_constant(prefix.encoding(), function);
        let slot = schema.reserve_gc_local(function);
        let final_name = schema
            .call_helper(
                crate::runtime_helpers::FunctionNameFromKeyArguments::new(
                    property_key,
                    prefix_local,
                ),
                self.runtime_helper_base()?,
                function,
            )
            .bind(schema, slot, function);
        schema.release_i32_local(prefix_local, function);
        let value = schema.reserve_value_local(function);
        value.set_reference(&final_name, schema, function);
        let target = schema.reserve_value_local(function);
        target.set_reference(callable, schema, function);
        let pending = schema.reserve_completion(function);
        let name_key = self.emit_function_string_key("name", function)?;
        // Allocation has already published the configurable name descriptor.
        // Definition replaces it in place, retaining one key and its order.
        self.emit_object_define_data_with_configurable(
            &target, &name_key, &value, false, false, true, &pending, function,
        )?;
        name_key.clear(function);
        target.clear(function);
        value.clear(function);
        final_name.clear(function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        Ok(())
    }
}
