//! Complete Arguments publication and private-list rest slicing.

use super::*;
use crate::gc_types::{
    ArgumentsObject, ArgumentsParameterMap, ArrayObject, GcOperand, GcStackReference, IndexedTable,
    NonNullable, Nullable, PropertyDescriptor, StoredValue, ValueArray,
};
use crate::objects::{AccessorDescriptorLocals, AccessorGetterLocals, AccessorSetterLocals};
use crate::operations::PropertyKeyLocals;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_arguments_object_payload(
        &mut self,
        protocol: &PresentArgumentsObjectProtocol,
        function: &mut Function,
    ) -> Result<GcStackReference<ArgumentsObject>, EmitError> {
        let schema = self.runtime_schema();
        let entry = self.body_entry_locals().ok_or_else(|| {
            EmitError::unsupported("Arguments construction requires declared body-entry roles")
        })?;
        let callable = schema.reserve_gc_local(function).initialize(
            entry
                .function_object()
                .ok_or_else(|| {
                    EmitError::unsupported("Arguments construction requires an actual callable")
                })?
                .load(schema, function),
            function,
        );
        let list = schema
            .reserve_gc_local(function)
            .initialize(entry.arguments().load(schema, function), function);
        let realm = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &prototype,
            function,
        );
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?,
            function,
        );
        let count = schema.reserve_i32_local(function);
        schema
            .array_type::<ValueArray>()
            .length(&list, schema, function);
        count.store(function);
        let indexed = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<IndexedTable>()
                .filled(GcOperand::null(schema), count, function),
            function,
        );
        let parameter_map = schema
            .reserve_gc_local::<ArgumentsParameterMap, Nullable>(function)
            .initialize_null(schema, function);
        match protocol {
            PresentArgumentsObjectProtocol::Mapped(plan) => {
                let map = schema
                    .reserve_gc_local::<ArgumentsParameterMap, NonNullable>(function)
                    .initialize(
                        schema.array_type::<ArgumentsParameterMap>().filled(
                            GcOperand::null(schema),
                            count,
                            function,
                        ),
                        function,
                    );
                let environment = schema
                    .reserve_gc_local(function)
                    .initialize(self.current_environment().load(schema, function), function);
                let position = schema.reserve_i32_local(function);
                for entry in plan.entries().iter().copied() {
                    function.instruction(&Instruction::I32Const(entry.argument_index() as i32));
                    position.store(function);
                    position.load(function);
                    count.load(function);
                    function.instruction(&Instruction::I32LtU);
                    self.open_frame(ControlFrameKind::If, function);
                    let cell = self.emit_environment_cell_local(
                        &environment,
                        entry.environment_slot(),
                        function,
                    );
                    schema.array_type::<ArgumentsParameterMap>().write(
                        &map,
                        position,
                        GcOperand::nullable_reference(&cell, schema),
                        schema,
                        function,
                    );
                    cell.clear(function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
                schema.release_i32_local(position, function);
                environment.clear(function);
                parameter_map.replace(map.load(schema, function).nullable(), function);
                map.clear(function);
            }
            PresentArgumentsObjectProtocol::Unmapped(_) => {}
        }
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let absent = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&undefined, function),
            function,
        );
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let value = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ValueArray>()
                .read(&list, index, schema, function)
                .reference(),
            function,
        );
        let descriptor = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PropertyDescriptor>().construct(
                (
                    GcOperand::descriptor_word(
                        StoredPropertyAttributes::Data {
                            writable: true,
                            enumerable: true,
                            configurable: true,
                        }
                        .descriptor_word(),
                    ),
                    GcOperand::subtype_reference(&value, schema),
                    GcOperand::reference(&absent, schema),
                    GcOperand::reference(&absent, schema),
                ),
                function,
            ),
            function,
        );
        schema.array_type::<IndexedTable>().write(
            &indexed,
            index,
            GcOperand::nullable_reference(&descriptor, schema),
            schema,
            function,
        );
        descriptor.clear(function);
        value.clear(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        let property = schema.reserve_value_local(function);
        let bits = schema.reserve_i64_local(function);
        count.load(function);
        function.instruction(&Instruction::F64ConvertI32U);
        function.instruction(&Instruction::I64ReinterpretF64);
        bits.store(function);
        property.set_number(bits, function);
        let length_key = self.emit_function_string_key("length", function)?;
        self.emit_object_append_data_property_with_flags(
            &header,
            &length_key,
            &property,
            true,
            false,
            true,
            function,
        )?;
        length_key.clear(function);
        let callee_key = self.emit_function_string_key("callee", function)?;
        match protocol {
            PresentArgumentsObjectProtocol::Mapped(_) => {
                property.set_reference(&callable, schema, function);
                self.emit_object_append_data_property_with_flags(
                    &header,
                    &callee_key,
                    &property,
                    true,
                    false,
                    true,
                    function,
                )?;
            }
            PresentArgumentsObjectProtocol::Unmapped(_) => {
                self.emit_load_required_function_realm_throw_type_error(
                    &callable, &property, function,
                );
                self.emit_object_append_accessor_property_with_flags(
                    &header,
                    &callee_key,
                    AccessorDescriptorLocals::GetterAndSetter {
                        getter: AccessorGetterLocals::new(&property),
                        setter: AccessorSetterLocals::new(&property),
                    },
                    false,
                    false,
                    function,
                )?;
            }
        }
        callee_key.clear(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::ArrayPrototypeValues,
            &property,
            function,
        );
        let iterator_symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(scriptw_ir::WellKnownSymbol::Iterator, function)?,
            function,
        );
        let iterator_key = PropertyKeyLocals::from_symbol(schema, &iterator_symbol, function);
        self.emit_object_append_data_property_with_flags(
            &header,
            &iterator_key,
            &property,
            true,
            false,
            true,
            function,
        )?;
        iterator_key.clear(function);
        iterator_symbol.clear(function);

        let result = schema.struct_type::<ArgumentsObject>().construct(
            (
                GcOperand::reference(&header, schema),
                GcOperand::reference(&indexed, schema),
                GcOperand::reference(&parameter_map, schema),
            ),
            function,
        );
        schema.release_i64_local(bits, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        property.clear(function);
        absent.clear(function);
        undefined.clear(function);
        parameter_map.clear(function);
        indexed.clear(function);
        header.clear(function);
        prototype.clear(function);
        realm.clear(function);
        list.clear(function);
        callable.clear(function);
        Ok(result)
    }

    /// Rest copies a private List; ArrayFromList alone publishes the JS Array.
    pub(crate) fn emit_rest_array_payload(
        &mut self,
        start_index: usize,
        function: &mut Function,
    ) -> Result<GcStackReference<ArrayObject>, EmitError> {
        let start = u32::try_from(start_index).map_err(|_| {
            EmitError::unsupported(
                "rest parameter index exceeds the private argument vector domain",
            )
        })?;
        let schema = self.runtime_schema();
        let entry = self.body_entry_locals().ok_or_else(|| {
            EmitError::unsupported("rest allocation requires declared body-entry arguments")
        })?;
        let arguments = schema
            .reserve_gc_local(function)
            .initialize(entry.arguments().load(schema, function), function);
        let length = schema.reserve_i32_local(function);
        schema
            .array_type::<ValueArray>()
            .length(&arguments, schema, function);
        length.store(function);
        length.load(function);
        function.instruction(&Instruction::I32Const(start as i32));
        function.instruction(&Instruction::I32GtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        length.load(function);
        function.instruction(&Instruction::I32Const(start as i32));
        function.instruction(&Instruction::I32Sub);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::End);
        length.store(function);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let absent = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&undefined, function),
            function,
        );
        let rest = schema
            .reserve_gc_local::<ValueArray, NonNullable>(function)
            .initialize(
                schema.array_type::<ValueArray>().filled(
                    GcOperand::reference(&absent, schema),
                    length,
                    function,
                ),
                function,
            );
        let index = schema.reserve_i32_local(function);
        let source_index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        index.load(function);
        function.instruction(&Instruction::I32Const(start as i32));
        function.instruction(&Instruction::I32Add);
        source_index.store(function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ValueArray>()
                .read(&arguments, source_index, schema, function)
                .reference(),
            function,
        );
        schema.array_type::<ValueArray>().write(
            &rest,
            index,
            GcOperand::reference(&stored, schema),
            schema,
            function,
        );
        stored.clear(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let array = self.emit_array_from_argument_list(&rest, function)?;
        let result = array.load(schema, function);
        array.clear(function);
        rest.clear(function);
        absent.clear(function);
        undefined.clear(function);
        arguments.clear(function);
        schema.release_i32_local(source_index, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
        Ok(result)
    }
}
