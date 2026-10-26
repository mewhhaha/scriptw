//! Concrete Array own-data reads stay inside the shared ObjectRead kernel.
//! They need no public descriptor allocation; deferred values resolve on demand. All reads that can invoke user
//! code or consult a prototype retain the ordinary completion-aware path.
use super::*;
use crate::gc_types::{ArrayObject, ArrayObjectSchema, GcNullability};

impl FunctionBuilder<'_> {
    pub(super) fn emit_array_own_data_read(
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
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let array = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<ArrayObject>(schema, function),
            function,
        );
        // Array length is a nonconfigurable intrinsic stored only in LENGTH.
        // Constructors and Array [[DefineOwnProperty]] never publish that key
        // into the ordinary header, so resolve it before named-property probes.
        // This arm admits concrete Arrays only; proxies retain their own path.
        let value = schema.reserve_value_local(function);
        let name = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("length", function)?,
            function,
        );
        let length_key = PropertyKeyLocals::from_string(schema, &name, function);
        self.emit_tagged_payload_same_value_i32(key.value(), length_key.value(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<ArrayObject>()
            .field(ArrayObjectSchema::LENGTH)
            .read(&array, schema, function)
            .store_i64(value.scalar(), function);
        value.scalar().load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        value.scalar().store(function);
        value.set_number(value.scalar(), function);
        result.set_normal(&value, function);
        function.instruction(&Instruction::I32Const(1));
        handled.store(function);
        function.instruction(&Instruction::Else);
        // Canonical indices live exclusively in ArrayIndexStorage. Classify
        // the already-converted key before probing the named-property header.
        // Holes and accessors still leave `handled` false for generic [[Get]].
        let descriptor = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize_null(schema, function);
        let index = schema.reserve_i32_local(function);
        let indexed = schema.reserve_i32_local(function);
        self.emit_property_key_array_index(key, index, indexed, function)?;
        indexed.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let index64 = schema.reserve_i64_local(function);
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        index64.store(function);
        let found = schema.reserve_i32_local(function);
        self.emit_array_own_indexed_value(&array, index64, &value, found, function);
        found.load(function);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(&value, function);
        handled.set_constant(1, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(found, function);
        schema.release_i64_local(index64, function);
        function.instruction(&Instruction::Else);
        let header = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ArrayObject>()
                .field(ArrayObjectSchema::OBJECT)
                .read(&array, schema, function)
                .reference(),
            function,
        );
        let named = self.emit_ordinary_own_descriptor_reference(&header, key, function)?;
        descriptor.replace(named.load(schema, function), function);
        named.clear(function);
        header.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(indexed, function);
        schema.release_i32_local(index, function);
        descriptor.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let present = schema.reserve_gc_local(function).initialize(
            descriptor.load(schema, function).require_non_null(function),
            function,
        );
        let flags = schema.reserve_i32_local(function);
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&present, schema, function)
            .store(flags, function);
        flags.load(function);
        function.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PropertyDescriptor>()
                .field(PropertyDescriptorSchema::VALUE)
                .read(&present, schema, function)
                .reference(),
            function,
        );
        self.emit_property_value_into(&stored, &value, function);
        result.set_normal(&value, function);
        function.instruction(&Instruction::I32Const(1));
        handled.store(function);
        stored.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(flags, function);
        present.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        length_key.clear(function);
        name.clear(function);
        value.clear(function);
        array.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(handled)
    }
}
