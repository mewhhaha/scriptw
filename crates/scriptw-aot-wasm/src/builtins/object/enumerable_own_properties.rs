//! EnumerableOwnProperties builds only unpublished defining-Realm Arrays.
use super::*;
use crate::gc_types::{
    ArrayObject, ArrayObjectSchema, CompactShapeKeys, GcLocal, GcOperand, I32Local,
    PackedValueListConstruction, ValueLocals,
};

#[derive(Clone, Copy)]
enum EnumerableOwnProperties {
    Keys,
    Entries,
    Values,
}

/// Only this owner can append to the private preallocated result. Publication
/// consumes it after the actual compact length has replaced the capacity.
#[must_use]
pub(super) struct NativeObjectResultArray {
    array: GcLocal<ArrayObject>,
    capacity: I32Local,
    next: I32Local,
}

impl NativeObjectResultArray {
    pub(super) fn append(
        &mut self,
        value: &ValueLocals,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = builder.runtime_schema();
        self.next.load(function);
        self.capacity.load(function);
        function.instruction(&Instruction::I32GeU);
        builder.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Unreachable);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let index = schema.reserve_i64_local(function);
        self.next.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        index.store(function);
        builder.emit_array_indexed_publish_default_data(&self.array, index, value, function)?;
        schema.release_i64_local(index, function);
        // The complete data value is installed before advancing length.
        self.next.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        self.next.store(function);
        Ok(())
    }

    pub(super) fn finish(
        self,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> GcLocal<ArrayObject> {
        let schema = builder.runtime_schema();
        let length = schema.reserve_i64_local(function);
        self.next.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        length.store(function);
        schema.field(ArrayObjectSchema::LENGTH).write(
            &self.array,
            GcOperand::i64_local(length),
            schema,
            function,
        );
        schema.release_i64_local(length, function);
        schema.release_i32_local(self.next, function);
        schema.release_i32_local(self.capacity, function);
        self.array
    }
}

impl FunctionBuilder<'_> {
    fn emit_compact_object_keys(
        &mut self,
        object: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let fallback = self.open_frame(ControlFrameKind::Block, function);
        let compact = self.emit_plain_data_object_storage(object, function);
        compact.load(schema, function).is_null(function);
        self.emit_branch_if_to_target(fallback, function);
        let storage = schema.reserve_gc_local(function).initialize(
            compact.load(schema, function).require_non_null(function),
            function,
        );
        let keys = self.emit_compact_shape_keys(&storage, function);
        let count = schema.reserve_i32_local(function);
        schema
            .array_type::<CompactShapeKeys>()
            .length(&keys, schema, function);
        count.store(function);
        let index = schema.reserve_i32_local(function);
        index.set_constant(0, function);
        let list = PackedValueListConstruction::allocate(schema, function);
        let value = schema.reserve_value_local(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, function);
        let name = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<CompactShapeKeys>()
                .read(&keys, index, schema, function)
                .reference(),
            function,
        );
        value.set_reference(&name, schema, function);
        name.clear(function);
        list.append(&value, schema, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        value.clear(function);
        let prototype = self.emit_load_current_function_realm_array_prototype(function);
        let array = schema.reserve_gc_local(function).initialize(
            self.emit_publish_packed_value_list_with_current_function_realm_prototype(
                list, prototype, function,
            )?,
            function,
        );
        output.value().set_reference(&array, schema, function);
        output.set_kind(CompletionKind::Normal, function);
        array.clear(function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        keys.clear(function);
        storage.clear(function);
        compact.clear(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(super) fn emit_native_object_result_array(
        &mut self,
        capacity: I32Local,
        function: &mut Function,
    ) -> Result<NativeObjectResultArray, EmitError> {
        let schema = self.runtime_schema();
        let length = schema.reserve_i64_local(function);
        capacity.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        length.store(function);
        let prototype = self.emit_load_current_function_realm_array_prototype(function);
        let array = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_array_with_current_function_realm_prototype(
                length, prototype, function,
            )?,
            function,
        );
        schema.release_i64_local(length, function);
        let retained_capacity = schema.reserve_i32_local(function);
        let next = schema.reserve_i32_local(function);
        capacity.load(function);
        retained_capacity.store(function);
        function.instruction(&Instruction::I32Const(0));
        next.store(function);
        Ok(NativeObjectResultArray {
            array,
            capacity: retained_capacity,
            next,
        })
    }

    fn compile_object_enumerable_own_properties_builtin(
        &mut self,
        mode: EnumerableOwnProperties,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let object = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let entry_value = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        let pending = schema.reserve_completion(function);
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let entry_count = schema.reserve_i32_local(function);
        let found = schema.reserve_i32_local(function);
        let enumerable = schema.reserve_i32_local(function);
        output.initialize(function);
        pending.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_builtin_arg_to_value(0, &argument, function);
        self.compile_nullish_tagged_i32(argument.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let message = match mode {
            EnumerableOwnProperties::Keys => {
                RuntimeErrorMessage::OBJECT_KEYS_CALLED_ON_NULL_OR_UNDEFINED
            }
            EnumerableOwnProperties::Entries => {
                RuntimeErrorMessage::OBJECT_ENTRIES_CALLED_ON_NULL_OR_UNDEFINED
            }
            EnumerableOwnProperties::Values => {
                RuntimeErrorMessage::OBJECT_VALUES_CALLED_ON_NULL_OR_UNDEFINED
            }
        };
        self.emit_throw_current_function_realm_type_error(message, &output, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_value_to_current_function_realm_object_locals(&argument, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, &output, exit, function);
        object.copy_from(pending.value(), function);
        if matches!(mode, EnumerableOwnProperties::Keys) {
            self.emit_compact_object_keys(&object, &output, exit, function)?;
        }
        let keys = self.emit_object_own_property_keys(&object, function)?;
        keys.length(count, schema, function);
        let mut array = self.emit_native_object_result_array(count, function)?;
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, function);
        let key = keys.read_key(index, self, function)?;
        key.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_own_property_enumerability(&object, &key, found, enumerable, function)?;
        enumerable.load(function);
        self.open_frame(ControlFrameKind::If, function);
        match mode {
            // Keys never calls Get, and all modes skip Symbols before GPD.
            EnumerableOwnProperties::Keys => array.append(key.value(), self, function)?,
            EnumerableOwnProperties::Values | EnumerableOwnProperties::Entries => {
                self.emit_object_read(&object, &object, &key, &pending, function)?;
                self.emit_native_object_abrupt_exit(&pending, &output, exit, function);
                value.copy_from(pending.value(), function);
                match mode {
                    EnumerableOwnProperties::Values => array.append(&value, self, function)?,
                    EnumerableOwnProperties::Entries => {
                        function.instruction(&Instruction::I32Const(2));
                        entry_count.store(function);
                        let mut entry =
                            self.emit_native_object_result_array(entry_count, function)?;
                        entry.append(key.value(), self, function)?;
                        entry.append(&value, self, function)?;
                        let entry = entry.finish(self, function);
                        entry_value.set_reference(&entry, schema, function);
                        array.append(&entry_value, self, function)?;
                        entry.clear(function);
                    }
                    EnumerableOwnProperties::Keys => unreachable!("Keys already appended its key"),
                }
            }
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        key.clear(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        keys.clear(function);
        let array = array.finish(self, function);
        output.value().set_reference(&array, schema, function);
        output.set_kind(CompletionKind::Normal, function);
        array.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        schema.release_i32_local(enumerable, function);
        schema.release_i32_local(found, function);
        schema.release_i32_local(entry_count, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        pending.clear(function);
        output.clear(function);
        entry_value.clear(function);
        value.clear(function);
        object.clear(function);
        argument.clear(function);
        Ok(())
    }

    pub(in crate::builtins) fn compile_object_keys_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_object_enumerable_own_properties_builtin(
            EnumerableOwnProperties::Keys,
            function,
        )
    }
    pub(in crate::builtins) fn compile_object_entries_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_object_enumerable_own_properties_builtin(
            EnumerableOwnProperties::Entries,
            function,
        )
    }
    pub(in crate::builtins) fn compile_object_values_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_object_enumerable_own_properties_builtin(
            EnumerableOwnProperties::Values,
            function,
        )
    }
}
