//! Effect-free own data reads avoid public descriptor snapshots and primitive
//! boxing. Keys have already completed ToPropertyKey; no prototype or accessor
//! lookup can be bypassed by these concrete representation guards.
use super::*;
use crate::gc_types::{
    CodeUnitArray, CompactDataStorage, FunctionObject, FunctionObjectSchema, GcNullability,
    StringValue, StringValueSchema,
};

impl FunctionBuilder<'_> {
    /// Plain objects have ordinary data semantics. Callable headers join this
    /// path only while their own storage is compact native metadata; source or
    /// promoted callable dictionaries retain legacy reflection handling.
    pub(in crate::objects) fn emit_simple_data_header(
        &mut self,
        target: &ValueLocals,
        function: &mut Function,
    ) -> GcLocal<OrdinaryObject, Nullable> {
        let schema = self.runtime_schema();
        let result = schema
            .reserve_gc_local::<OrdinaryObject, Nullable>(function)
            .initialize_null(schema, function);
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<OrdinaryObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        result.replace(
            target
                .cast_reference::<OrdinaryObject>(schema, function)
                .nullable(),
            function,
        );
        function.instruction(&Instruction::Else);
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let callable = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let header = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::OBJECT)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        let storage = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&header, schema, function)
                .reference(),
            function,
        );
        storage.is_subtype::<CompactDataStorage>(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        result.replace(header.load(schema, function).nullable(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        storage.clear(function);
        header.clear(function);
        callable.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result
    }

    /// The completed key is String or Symbol. Only an actual six-unit String
    /// can equal "length"; reject other keys before resolving the pooled name.
    /// Retain full SameValue for computed strings with the right length.
    pub(super) fn emit_length_property_key_match(
        &mut self,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let matches = schema.reserve_i32_local(function);
        matches.set_constant(0, function);
        key.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let text = schema.reserve_gc_local(function).initialize(
            key.value().cast_reference::<StringValue>(schema, function),
            function,
        );
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .field(StringValueSchema::CODE_UNITS)
                .read(&text, schema, function)
                .reference(),
            function,
        );
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        function.instruction(&Instruction::I32Const(6));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let name = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("length", function)?,
            function,
        );
        let length_key = PropertyKeyLocals::from_string(schema, &name, function);
        self.emit_tagged_payload_same_value_i32(key.value(), length_key.value(), function)?;
        matches.store(function);
        length_key.clear(function);
        name.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        units.clear(function);
        text.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(matches)
    }

    pub(super) fn emit_primitive_string_length_read(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        handled.store(function);
        target.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let length_key = self.emit_length_property_key_match(key, function)?;
        length_key.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let string = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<StringValue>(schema, function),
            function,
        );
        let length = schema.reserve_i64_local(function);
        self.emit_native_gc_string_length(&string, length, function);
        length.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        length.store(function);
        let value = schema.reserve_value_local(function);
        value.set_number(length, function);
        result.set_normal(&value, function);
        function.instruction(&Instruction::I32Const(1));
        handled.store(function);
        value.clear(function);
        schema.release_i64_local(length, function);
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(length_key, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(handled)
    }

    pub(super) fn emit_ordinary_own_data_read(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        handled.store(function);
        let header = self.emit_simple_data_header(target, function);
        header.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let object = schema.reserve_gc_local(function).initialize(
            header.load(schema, function).require_non_null(function),
            function,
        );
        let value = schema.reserve_value_local(function);
        self.emit_ordinary_own_data_value(&object, key, &value, handled, function)?;
        handled.load(function);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(&value, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.clear(function);
        object.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        header.clear(function);
        Ok(handled)
    }
}
