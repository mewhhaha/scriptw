//! OwnKeys returns a completed private PropertyKey List. Proxy trap snapshots
//! and target observations remain rooted until all invariant checks finish.

use super::*;
use crate::gc_types::{
    ArgumentsObject, ArgumentsObjectSchema, ArrayObject, ArrayObjectSchema, CodeUnitArray,
    I32Local, I64Local, IndexedTable, ModuleExport, ModuleExportSchema, ModuleExportTable,
    ModuleNamespaceObject, ModuleNamespaceObjectSchema, PrimitiveBox, PrimitiveBoxSchema,
    PropertyKeyConstruction, PropertyKeyTable, StringValue, StringValueSchema, TypedArrayObject,
};

#[must_use = "the complete own-key snapshot must be consumed and cleared"]
pub(crate) struct CompletedOwnPropertyKeys {
    table: GcLocal<PropertyKeyTable>,
}

impl CompletedOwnPropertyKeys {
    pub(crate) fn length(
        &self,
        output: I32Local,
        schema: &crate::gc_types::RuntimeSchema,
        function: &mut Function,
    ) {
        schema
            .array_type::<PropertyKeyTable>()
            .length(&self.table, schema, function);
        output.store(function);
    }
    pub(crate) fn read_key(
        &self,
        index: I32Local,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        builder.emit_property_key_table_entry(&self.table, index, function)
    }

    /// Transfers this completed accepted-key snapshot into a private native
    /// cursor. No constructor accepts an arbitrary table as a completed list.
    pub(crate) fn into_table(self) -> GcLocal<PropertyKeyTable> {
        self.table
    }

    pub(crate) fn clear(self, function: &mut Function) {
        self.table.clear(function);
    }
}

impl FunctionBuilder<'_> {
    /// Both fresh OwnKeys consumers and retained ForIn snapshots use this
    /// same accepted-key read; String/Symbol recognition has no user effects.
    pub(crate) fn emit_property_key_table_entry(
        &mut self,
        table: &GcLocal<PropertyKeyTable>,
        index: I32Local,
        function: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let builder = self;
        let schema = builder.runtime_schema();
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<PropertyKeyTable>()
                .read(table, index, schema, function)
                .reference(),
            function,
        );
        let value = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &value, schema, function);
        // The only constructor stores accepted keys. The common conversion
        // recognizes String/Symbol directly and runs no observable coercion.
        let key = builder.emit_value_to_property_key_locals(&value, function)?;
        value.clear(function);
        stored.clear(function);
        Ok(key)
    }
}

impl FunctionBuilder<'_> {
    /// The native algorithm is the recursive runtime boundary in the executing
    /// Realm. Its fresh Array is private; no public property is consulted.
    pub(crate) fn emit_object_own_property_keys(
        &mut self,
        target: &ValueLocals,
        function: &mut Function,
    ) -> Result<CompletedOwnPropertyKeys, EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        self.emit_native_object_algorithm_call(
            crate::functions::NativeObjectAlgorithm::OwnKeys,
            &[target],
            &pending,
            function,
        )?;
        self.emit_own_keys_propagate_throw(&pending, function);
        let array = schema.reserve_gc_local(function).initialize(
            pending
                .value()
                .cast_reference::<ArrayObject>(schema, function),
            function,
        );

        let length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let index64 = schema.reserve_i64_local(function);
        let value = schema.reserve_value_local(function);
        schema
            .field(ArrayObjectSchema::LENGTH)
            .read(&array, schema, function)
            .store_i64(value.scalar(), function);
        value.scalar().load(function);
        function.instruction(&Instruction::I32WrapI64);
        length.store(function);
        let construction = PropertyKeyConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        index64.store(function);
        let nullable_descriptor = self.emit_array_indexed_descriptor(&array, index64, function);
        let descriptor = schema.reserve_gc_local(function).initialize(
            nullable_descriptor
                .load(schema, function)
                .require_non_null(function),
            function,
        );
        nullable_descriptor.clear(function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .field(PropertyDescriptorSchema::VALUE)
                .read(&descriptor, schema, function)
                .reference(),
            function,
        );
        self.emit_property_value_into(&stored, &value, function);
        let key = self.emit_value_to_property_key_locals(&value, function)?;
        construction.write(index, &key, schema, function);
        key.clear(function);
        stored.clear(function);
        descriptor.clear(function);
        self.emit_own_keys_increment(index, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let table = schema
            .reserve_gc_local(function)
            .initialize(construction.publish(schema, function), function);
        value.clear(function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
        schema.release_i64_local(index64, function);
        array.clear(function);
        pending.clear(function);
        Ok(CompletedOwnPropertyKeys { table })
    }

    /// Direct native Reflect.ownKeys body. Recursive Proxy targets use the
    /// recorded facade above; the compiler never recursively emits this body.
    pub(crate) fn emit_own_keys_internal(
        &mut self,
        target: &ValueLocals,
        function: &mut Function,
    ) -> Result<CompletedOwnPropertyKeys, EmitError> {
        let schema = self.runtime_schema();
        let zero = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        zero.store(function);
        let empty = PropertyKeyConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            zero,
            function,
        );
        let table = schema
            .reserve_gc_local(function)
            .initialize(empty.publish(schema, function), function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_heap_object_like_tag_i32(target.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::REFLECT_OWNKEYS_TARGET_MUST_BE_OBJECT,
            &pending,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            &pending,
            function,
            |builder, slots, function| {
                builder.emit_proxy_own_keys(slots, &table, &pending, function)
            },
        )?;
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_non_proxy_own_keys(target, &table, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_own_keys_propagate_throw(&pending, function);
        pending.clear(function);
        schema.release_i32_local(zero, function);
        Ok(CompletedOwnPropertyKeys { table })
    }

    fn emit_own_keys_propagate_throw(
        &mut self,
        pending: &CompletionLocals,
        function: &mut Function,
    ) {
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }
    fn emit_own_keys_increment(&self, index: I32Local, function: &mut Function) {
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
    }
    /// Every source phase owns disjoint keys: occupied indices, virtual
    /// indices/length, and the canonical unique named-property table.
    fn emit_own_keys_candidate_push(
        &self,
        list: &GcLocal<crate::gc_types::ValueArray>,
        count: I32Local,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(value, function),
            function,
        );
        schema.array_type::<crate::gc_types::ValueArray>().write(
            list,
            count,
            GcOperand::reference(&stored, schema),
            schema,
            function,
        );
        stored.clear(function);
        self.emit_own_keys_increment(count, function);
    }
    fn emit_own_keys_index_value(
        &mut self,
        index: I64Local,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let number = schema.reserve_i64_local(function);
        index.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        number.store(function);
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_number_to_string_payload(number, function)?,
            function,
        );
        output.set_reference(&string, schema, function);
        string.clear(function);
        schema.release_i64_local(number, function);
        Ok(())
    }

    fn emit_non_proxy_own_keys(
        &mut self,
        target: &ValueLocals,
        output: &GcLocal<PropertyKeyTable>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_is_module_namespace_i32(target, function);
        self.open_frame(ControlFrameKind::If, function);
        let namespace = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<ModuleNamespaceObject>(schema, function),
            function,
        );
        // Namespace export names are already UTF-16 sorted by the linker;
        // numeric-looking names remain in that order rather than integer order.
        let list = self.emit_namespace_own_keys(&namespace, NamespaceOwnKeys::All, function)?;
        let length = schema.reserve_i32_local(function);
        schema
            .array_type::<crate::gc_types::ValueArray>()
            .length(&list, schema, function);
        length.store(function);
        self.emit_own_keys_ordered_values_to_table(&list, length, output, function)?;
        schema.release_i32_local(length, function);
        list.clear(function);
        namespace.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_ordinary_exotic_own_keys(target, output, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_own_keys_ordered_values_to_table(
        &mut self,
        list: &GcLocal<crate::gc_types::ValueArray>,
        length: I32Local,
        output: &GcLocal<PropertyKeyTable>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let index = schema.reserve_i32_local(function);
        let value = schema.reserve_value_local(function);
        let construction = PropertyKeyConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<crate::gc_types::ValueArray>()
                .read(list, index, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &value, schema, function);
        stored.clear(function);
        let key = self.emit_value_to_property_key_locals(&value, function)?;
        construction.write(index, &key, schema, function);
        key.clear(function);
        self.emit_own_keys_increment(index, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        output.replace(construction.publish(schema, function), function);
        value.clear(function);
        schema.release_i32_local(index, function);
        Ok(())
    }

    fn emit_ordinary_exotic_own_keys(
        &mut self,
        target: &ValueLocals,
        output: &GcLocal<PropertyKeyTable>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(target, function),
            function,
        );
        let base = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&header, schema, function)
                .reference(),
            function,
        );
        base.is_subtype::<crate::gc_types::CompactDataStorage>(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        let compact = schema.reserve_gc_local(function).initialize(
            base.load(schema, function)
                .cast_subtype::<crate::gc_types::CompactDataStorage>(schema, function),
            function,
        );
        self.emit_compact_own_keys(&compact, output, function)?;
        compact.clear(function);
        function.instruction(&Instruction::Else);
        let storage = self.emit_dictionary_property_storage(&header, function);
        let properties = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(&storage, schema, function)
                .reference(),
            function,
        );
        let indexed = schema
            .reserve_gc_local::<IndexedTable, Nullable>(function)
            .initialize_null(schema, function);
        let array_keys = schema
            .reserve_gc_local::<crate::gc_types::ArrayIndexKeyTable, Nullable>(function)
            .initialize_null(schema, function);
        let primitive = schema.reserve_value_local(function);
        primitive.set_undefined(function);
        let named_length = schema.reserve_i32_local(function);
        let indexed_length = schema.reserve_i32_local(function);
        let virtual_length = schema.reserve_i64_local(function);
        let capacity = schema.reserve_i64_local(function);
        let array_kind = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        indexed_length.store(function);
        function.instruction(&Instruction::I64Const(0));
        virtual_length.store(function);
        target.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Array as i32));
        function.instruction(&Instruction::I32Eq);
        array_kind.store(function);
        array_kind.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let array = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<ArrayObject>(schema, function),
            function,
        );
        let keys = self.emit_array_indexed_keys(&array, function);
        array_keys.replace(keys.load(schema, function).nullable(), function);
        schema
            .array_type::<crate::gc_types::ArrayIndexKeyTable>()
            .length(&keys, schema, function);
        indexed_length.store(function);
        keys.clear(function);
        array.clear(function);
        function.instruction(&Instruction::Else);
        target.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Arguments as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let arguments = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<ArgumentsObject>(schema, function),
            function,
        );
        indexed.replace(
            schema
                .field(ArgumentsObjectSchema::INDEXED)
                .read(&arguments, schema, function)
                .reference()
                .nullable(),
            function,
        );
        arguments.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        indexed.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .array_type::<IndexedTable>()
            .length(&indexed, schema, function);
        indexed_length.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<PrimitiveBox>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let boxed = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<PrimitiveBox>(schema, function),
            function,
        );
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .field(PrimitiveBoxSchema::PRIMITIVE)
                .read(&boxed, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &primitive, schema, function);
        primitive.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let string = schema.reserve_gc_local(function).initialize(
            primitive.cast_reference::<StringValue>(schema, function),
            function,
        );
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .field(StringValueSchema::CODE_UNITS)
                .read(&string, schema, function)
                .reference(),
            function,
        );
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        function.instruction(&Instruction::I64ExtendI32U);
        virtual_length.store(function);
        units.clear(function);
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        stored.clear(function);
        boxed.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<TypedArrayObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let typed_array = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<TypedArrayObject>(schema, function),
            function,
        );
        self.emit_typed_array_own_keys_length(&typed_array, virtual_length, function);
        typed_array.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&storage, schema, function)
            .store(named_length, function);
        named_length.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        indexed_length.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Add);
        virtual_length.load(function);
        function.instruction(&Instruction::I64Add);
        array_kind.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Add);
        capacity.store(function);
        // This is a private GC allocation bound, never a fabricated language
        // RangeError or a truncation into a smaller key List.
        capacity.load(function);
        function.instruction(&Instruction::I64Const(u32::MAX as i64));
        function.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let index64 = schema.reserve_i64_local(function);
        let value = schema.reserve_value_local(function);
        value.set_undefined(function);
        let undefined = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&value, function),
            function,
        );
        capacity.load(function);
        function.instruction(&Instruction::I32WrapI64);
        count.store(function);
        let candidates = schema.reserve_gc_local(function).initialize(
            schema.array_type::<crate::gc_types::ValueArray>().filled(
                GcOperand::reference(&undefined, schema),
                count,
                function,
            ),
            function,
        );
        undefined.clear(function);
        function.instruction(&Instruction::I32Const(0));
        count.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        indexed_length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        array_kind.load(function);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .array_type::<crate::gc_types::ArrayIndexKeyTable>()
            .read(&array_keys, index, schema, function)
            .store_i64(index64, function);
        self.emit_own_keys_index_value(index64, &value, function)?;
        self.emit_own_keys_candidate_push(&candidates, count, &value, function);
        function.instruction(&Instruction::Else);
        let descriptor = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<IndexedTable>()
                .read(&indexed, index, schema, function)
                .reference(),
            function,
        );
        descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        index64.store(function);
        self.emit_own_keys_index_value(index64, &value, function)?;
        self.emit_own_keys_candidate_push(&candidates, count, &value, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_own_keys_increment(index, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(0));
        index64.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index64.load(function);
        virtual_length.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_own_keys_index_value(index64, &value, function)?;
        self.emit_own_keys_candidate_push(&candidates, count, &value, function);
        index64.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index64.store(function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        array_kind.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let length_string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("length", function)?,
            function,
        );
        value.set_reference(&length_string, schema, function);
        length_string.clear(function);
        self.emit_own_keys_candidate_push(&candidates, count, &value, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        named_length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        let entry = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<PropertyTable>()
                .read(&properties, index, schema, function)
                .reference(),
            function,
        );
        entry.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .field(PropertyEntrySchema::KEY)
                .read(&entry, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &value, schema, function);
        stored.clear(function);
        self.emit_own_keys_candidate_push(&candidates, count, &value, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        entry.clear(function);
        self.emit_own_keys_increment(index, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_own_keys_order_candidates(&candidates, count, output, function)?;
        candidates.clear(function);
        value.clear(function);
        schema.release_i64_local(index64, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        schema.release_i32_local(array_kind, function);
        schema.release_i64_local(capacity, function);
        schema.release_i64_local(virtual_length, function);
        schema.release_i32_local(indexed_length, function);
        schema.release_i32_local(named_length, function);
        primitive.clear(function);
        array_keys.clear(function);
        indexed.clear(function);
        properties.clear(function);
        storage.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        base.clear(function);
        header.clear(function);
        Ok(())
    }

    fn emit_compact_own_keys(
        &mut self,
        storage: &GcLocal<crate::gc_types::CompactDataStorage>,
        output: &GcLocal<PropertyKeyTable>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use crate::gc_types::*;
        let schema = self.runtime_schema();
        let keys = self.emit_compact_shape_keys(storage, function);
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        schema
            .array_type::<CompactShapeKeys>()
            .length(&keys, schema, function);
        count.store(function);
        let value = schema.reserve_value_local(function);
        value.set_undefined(function);
        let undefined = schema
            .reserve_gc_local(function)
            .initialize(schema.stored_undefined(function), function);
        let candidates = schema.reserve_gc_local(function).initialize(
            schema.array_type::<ValueArray>().filled(
                GcOperand::reference(&undefined, schema),
                count,
                function,
            ),
            function,
        );
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let repeat = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, function);
        let name = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<CompactShapeKeys>()
                .read(&keys, index, schema, function)
                .reference(),
            function,
        );
        value.set_reference(&name, schema, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&value, function),
            function,
        );
        schema.array_type::<ValueArray>().write(
            &candidates,
            index,
            GcOperand::reference(&stored, schema),
            schema,
            function,
        );
        stored.clear(function);
        name.clear(function);
        self.emit_own_keys_increment(index, function);
        self.emit_branch_to_target(repeat, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_own_keys_order_candidates(&candidates, count, output, function)?;
        candidates.clear(function);
        undefined.clear(function);
        value.clear(function);
        keys.clear(function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        Ok(())
    }

    fn emit_own_keys_order_candidates(
        &mut self,
        candidates: &GcLocal<crate::gc_types::ValueArray>,
        length: I32Local,
        output: &GcLocal<PropertyKeyTable>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let index = schema.reserve_i32_local(function);
        let ordinal = schema.reserve_i32_local(function);
        let is_index = schema.reserve_i32_local(function);
        let output_index = schema.reserve_i32_local(function);
        let numeric_count = schema.reserve_i32_local(function);
        let numeric_value = schema.reserve_i64_local(function);
        let value = schema.reserve_value_local(function);
        let numeric =
            crate::gc_types::ArrayIndexKeyConstruction::allocate(schema, length, function);
        let construction = PropertyKeyConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        function.instruction(&Instruction::I32Const(0));
        numeric_count.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<crate::gc_types::ValueArray>()
                .read(candidates, index, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &value, schema, function);
        stored.clear(function);
        let key = self.emit_value_to_property_key_locals(&value, function)?;
        self.emit_property_key_array_index(&key, ordinal, is_index, function)?;
        is_index.load(function);
        self.open_frame(ControlFrameKind::If, function);
        ordinal.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        numeric_value.store(function);
        numeric.write(numeric_count, numeric_value, schema, function);
        self.emit_own_keys_increment(numeric_count, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        key.clear(function);
        self.emit_own_keys_increment(index, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_array_index_sort(&numeric, numeric_count, function);
        function.instruction(&Instruction::I32Const(0));
        output_index.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        output_index.load(function);
        numeric_count.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        numeric.read(output_index, numeric_value, schema, function);
        self.emit_own_keys_index_value(numeric_value, &value, function)?;
        let key = self.emit_value_to_property_key_locals(&value, function)?;
        construction.write(output_index, &key, schema, function);
        key.clear(function);
        self.emit_own_keys_increment(output_index, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        for tag in [WasmRuntimeValueTag::String, WasmRuntimeValueTag::Symbol] {
            function.instruction(&Instruction::I32Const(0));
            index.store(function);
            self.open_frame(ControlFrameKind::Block, function);
            self.open_frame(ControlFrameKind::Loop, function);
            index.load(function);
            length.load(function);
            function.instruction(&Instruction::I32GeU);
            function.instruction(&Instruction::BrIf(1));
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .array_type::<crate::gc_types::ValueArray>()
                    .read(candidates, index, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<StoredValue>()
                .read_into(&stored, &value, schema, function);
            stored.clear(function);
            let key = self.emit_value_to_property_key_locals(&value, function)?;
            self.emit_property_key_array_index(&key, ordinal, is_index, function)?;
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(tag as i32));
            function.instruction(&Instruction::I32Eq);
            is_index.load(function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            construction.write(output_index, &key, schema, function);
            self.emit_own_keys_increment(output_index, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            key.clear(function);
            self.emit_own_keys_increment(index, function);
            function.instruction(&Instruction::Br(0));
            self.pop_control(ControlFrameKind::Loop);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
        }
        output.replace(construction.publish(schema, function), function);
        numeric.clear(function);
        value.clear(function);
        schema.release_i64_local(numeric_value, function);
        schema.release_i32_local(numeric_count, function);
        schema.release_i32_local(output_index, function);
        schema.release_i32_local(is_index, function);
        schema.release_i32_local(ordinal, function);
        schema.release_i32_local(index, function);
        Ok(())
    }

    fn emit_proxy_own_keys(
        &mut self,
        slots: &ProxySlotLocals,
        output: &GcLocal<PropertyKeyTable>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let method = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let extensible = schema.reserve_i32_local(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_proxy_named_method(slots, "ownKeys", &method, &pending, result, exit, function)?;
        self.compile_nullish_tagged_i32(method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let keys = self.emit_object_own_property_keys(slots.target(), function)?;
        output.replace(keys.table.load(schema, function), function);
        keys.clear(function);
        result.initialize(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_proxy_method_callable_check(
            &method,
            RuntimeErrorMessage::PROXY_OWNKEYS_TRAP_IS_NOT_CALLABLE,
            result,
            exit,
            function,
        )?;
        let arguments = self.emit_pre_evaluated_arg_vector(&[slots.target()], function);
        self.emit_function_or_proxy_call_with_argv(
            &method,
            slots.handler(),
            &arguments,
            &pending,
            function,
        )?;
        arguments.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        let snapshot = self.emit_proxy_own_keys_snapshot(pending.value(), result, function)?;
        self.emit_object_operation_abrupt_exit(result, result, exit, function);
        // Complete target observations in their required order. No membership
        // test runs while descriptors are still being acquired.
        self.emit_object_is_extensible(slots.target(), &pending, function)?;
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        extensible.store(function);
        let target_keys = self.emit_object_own_property_keys(slots.target(), function)?;
        let length = schema.reserve_i32_local(function);
        let trap_length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let found = schema.reserve_i32_local(function);
        target_keys.length(length, schema, function);
        snapshot.length(trap_length, schema, function);
        let nonconfigurable = schema.reserve_gc_local(function).initialize(
            schema.array_type::<crate::gc_types::ByteArray>().filled(
                GcOperand::i32(0),
                length,
                function,
            ),
            function,
        );
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        let key = target_keys.read_key(index, self, function)?;
        let descriptor = self.emit_proxy_target_descriptor_fact(slots.target(), &key, function)?;
        descriptor.emit_found_i32(schema, function);
        descriptor.emit_configurable_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        found.store(function);
        schema.array_type::<crate::gc_types::ByteArray>().write(
            &nonconfigurable,
            index,
            GcOperand::i32_local(found),
            schema,
            function,
        );
        descriptor.clear(function);
        key.clear(function);
        self.emit_own_keys_increment(index, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let validation_exit = self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        schema
            .array_type::<crate::gc_types::ByteArray>()
            .read(&nonconfigurable, index, schema, function)
            .store(found, function);
        found.load(function);
        extensible.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        let key = target_keys.read_key(index, self, function)?;
        let matches = schema.reserve_i32_local(function);
        self.emit_own_keys_contains(&snapshot, &key, matches, function)?;
        matches.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        found.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_OWNKEYS_TRAP_RESULT_OMITTED_TARGET_PROPERTY,
            result,
            function,
        )?;
        function.instruction(&Instruction::Else);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_OWNKEYS_TRAP_RESULT_DOES_NOT_MATCH_NON_EXTENSIBLE_TARGET,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_branch_to_target(validation_exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(matches, function);
        key.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_own_keys_increment(index, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        extensible.load(function);
        function.instruction(&Instruction::I32Eqz);
        length.load(function);
        trap_length.load(function);
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(RuntimeErrorMessage::PROXY_OWNKEYS_TRAP_RESULT_CONTAINS_AN_EXTRA_KEY_FOR_A_NON_EXTENSIBLE_TARGET, result, function)?;
        self.emit_branch_to_target(validation_exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        output.replace(snapshot.table.load(schema, function), function);
        result.initialize(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        nonconfigurable.clear(function);
        schema.release_i32_local(found, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(trap_length, function);
        schema.release_i32_local(length, function);
        target_keys.clear(function);
        snapshot.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(extensible, function);
        pending.clear(function);
        method.clear(function);
        Ok(())
    }

    fn emit_proxy_own_keys_snapshot(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<CompletedOwnPropertyKeys, EmitError> {
        let schema = self.runtime_schema();
        let length = schema.reserve_i64_local(function);
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let cursor = schema.reserve_i32_local(function);
        let index64 = schema.reserve_i64_local(function);
        let value = schema.reserve_value_local(function);
        let existing = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let zero = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        zero.store(function);
        let empty = PropertyKeyConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            zero,
            function,
        );
        let table = schema
            .reserve_gc_local(function)
            .initialize(empty.publish(schema, function), function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_heap_object_like_tag_i32(input.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_OWNKEYS_TRAP_RESULT_MUST_BE_AN_OBJECT,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let length_name = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("length", function)?,
            function,
        );
        let length_key = PropertyKeyLocals::from_string(schema, &length_name, function);
        length_name.clear(function);
        self.emit_dynamic_property_read_with_key_locals(
            input,
            input,
            &length_key,
            &pending,
            function,
        )?;
        length_key.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        value.copy_from(pending.value(), function);
        self.emit_to_length_i64_from_value_locals(&value, length, &pending, function)?;
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        length.load(function);
        function.instruction(&Instruction::I64Const(u32::MAX as i64));
        function.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        length.load(function);
        function.instruction(&Instruction::I32WrapI64);
        count.store(function);
        value.set_undefined(function);
        let undefined = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&value, function),
            function,
        );
        let values = schema.reserve_gc_local(function).initialize(
            schema.array_type::<crate::gc_types::ValueArray>().filled(
                GcOperand::reference(&undefined, schema),
                count,
                function,
            ),
            function,
        );
        undefined.clear(function);
        let snapshot_exit = self.open_frame(ControlFrameKind::Block, function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        index64.store(function);
        self.emit_own_keys_index_value(index64, &value, function)?;
        let key = self.emit_value_to_property_key_locals(&value, function)?;
        self.emit_dynamic_property_read_with_key_locals(input, input, &key, &pending, function)?;
        key.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, result, snapshot_exit, function);
        value.copy_from(pending.value(), function);
        self.emit_is_property_key_i32(value.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_OWNKEYS_TRAP_RESULT_CONTAINED_A_NON_PROPERTY_KEY,
            result,
            function,
        )?;
        self.emit_branch_to_target(snapshot_exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&value, function),
            function,
        );
        schema.array_type::<crate::gc_types::ValueArray>().write(
            &values,
            index,
            GcOperand::reference(&stored, schema),
            schema,
            function,
        );
        stored.clear(function);
        self.emit_own_keys_increment(index, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        // CreateListFromArrayLike obtains and validates every element before
        // Proxy [[OwnPropertyKeys]] checks duplicates. A later numeric Get
        // therefore still wins over a duplicate seen at an earlier index.
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        let candidate = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<crate::gc_types::ValueArray>()
                .read(&values, index, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&candidate, &value, schema, function);
        candidate.clear(function);
        function.instruction(&Instruction::I32Const(0));
        cursor.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        cursor.load(function);
        index.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<crate::gc_types::ValueArray>()
                .read(&values, cursor, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &existing, schema, function);
        stored.clear(function);
        self.emit_tagged_payload_same_value_i32(&value, &existing, function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_OWNKEYS_TRAP_RESULT_CONTAINED_A_DUPLICATE_KEY,
            result,
            function,
        )?;
        self.emit_branch_to_target(snapshot_exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_own_keys_increment(cursor, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_own_keys_increment(index, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_own_keys_ordered_values_to_table(&values, count, &table, function)?;
        result.initialize(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        values.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(zero, function);
        pending.clear(function);
        existing.clear(function);
        value.clear(function);
        schema.release_i64_local(index64, function);
        schema.release_i32_local(cursor, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        schema.release_i64_local(length, function);
        self.emit_own_keys_propagate_throw(result, function);
        Ok(CompletedOwnPropertyKeys { table })
    }

    fn emit_own_keys_contains(
        &mut self,
        keys: &CompletedOwnPropertyKeys,
        needle: &PropertyKeyLocals,
        output: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        keys.length(length, schema, function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::I32Const(0));
        output.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        let key = keys.read_key(index, self, function)?;
        self.emit_tagged_payload_same_value_i32(key.value(), needle.value(), function)?;
        output.store(function);
        key.clear(function);
        output.load(function);
        function.instruction(&Instruction::BrIf(1));
        self.emit_own_keys_increment(index, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
        Ok(())
    }
}
