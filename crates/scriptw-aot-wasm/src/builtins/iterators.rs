//! Native iterator records retain typed GC slots and whole completions.
use super::super::*;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::gc_types::{
    ArrayIterationKind, ArrayIteratorObject, ArrayIteratorObjectSchema, CodeUnitArray,
    CompletionLocals, GcI32Constant, GcLocal, GcNullability, GcOperand, GcStackReference, I32Local,
    I64Local, OrdinaryObject, RealmRecord, RegExpStringIteratorObject,
    RegExpStringIteratorObjectSchema, StoredValue, StringConstruction, StringIteratorObject,
    StringIteratorObjectSchema, StringValue, StringValueSchema, TypedArrayIteratorObject,
    TypedArrayIteratorObjectSchema, TypedArrayObject, ValueLocals,
};
use crate::objects::PropertyKeyLocals;

mod from;
mod prototype_accessors;
mod symbol_dispose;
pub(crate) use prototype_accessors::IteratorPrototypeWeirdSetter;

impl FunctionBuilder<'_> {
    fn emit_native_iterator_header(
        &mut self,
        slot: NonArrayRealmIntrinsicSlot,
        function: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(&realm, slot, &prototype, function);
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?,
            function,
        );
        prototype.clear(function);
        realm.clear(function);
        Ok(header)
    }

    fn emit_native_iterator_abrupt_exit(
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

    fn emit_native_iterator_error_if(
        &mut self,
        message: RuntimeErrorMessage,
        result: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(message, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_iterator_named_key(
        &mut self,
        name: &str,
        function: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let schema = self.runtime_schema();
        let text = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(name, function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &text, function);
        text.clear(function);
        Ok(key)
    }

    pub(crate) fn emit_string_iterator_create_from_local(
        &mut self,
        input: &GcLocal<StringValue>,
        function: &mut Function,
    ) -> Result<GcStackReference<StringIteratorObject>, EmitError> {
        let schema = self.runtime_schema();
        let header = self.emit_native_iterator_header(
            NonArrayRealmIntrinsicSlot::StringIteratorPrototype,
            function,
        )?;
        let result = schema.struct_type::<StringIteratorObject>().construct(
            (
                GcOperand::reference(&header, schema),
                GcOperand::reference(input, schema),
                GcOperand::i64(0),
                GcOperand::boolean(false),
            ),
            function,
        );
        header.clear(function);
        Ok(result)
    }

    pub(crate) fn emit_array_iterator_create_from_locals(
        &mut self,
        object: &ValueLocals,
        kind: ArrayIterationKind,
        function: &mut Function,
    ) -> Result<GcStackReference<ArrayIteratorObject>, EmitError> {
        let schema = self.runtime_schema();
        let header = self.emit_native_iterator_header(
            NonArrayRealmIntrinsicSlot::ArrayIteratorPrototype,
            function,
        )?;
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(object, function),
            function,
        );
        let result = schema.struct_type::<ArrayIteratorObject>().construct(
            (
                GcOperand::reference(&header, schema),
                GcOperand::reference(&stored, schema),
                GcOperand::i64(0),
                GcOperand::boolean(false),
                GcOperand::constant(kind),
            ),
            function,
        );
        stored.clear(function);
        header.clear(function);
        Ok(result)
    }

    pub(crate) fn emit_typed_array_iterator_create_from_locals(
        &mut self,
        object: &GcLocal<TypedArrayObject>,
        kind: ArrayIterationKind,
        function: &mut Function,
    ) -> Result<GcStackReference<TypedArrayIteratorObject>, EmitError> {
        let schema = self.runtime_schema();
        let header = self.emit_native_iterator_header(
            NonArrayRealmIntrinsicSlot::ArrayIteratorPrototype,
            function,
        )?;
        let result = schema.struct_type::<TypedArrayIteratorObject>().construct(
            (
                GcOperand::reference(&header, schema),
                GcOperand::nullable_reference(object, schema),
                GcOperand::i64(0),
                GcOperand::boolean(false),
                GcOperand::constant(kind),
            ),
            function,
        );
        header.clear(function);
        Ok(result)
    }

    pub(crate) fn emit_regexp_string_iterator_create_from_locals(
        &mut self,
        regexp: &ValueLocals,
        input: &GcLocal<StringValue>,
        global: I32Local,
        full_unicode: I32Local,
        function: &mut Function,
    ) -> Result<GcStackReference<RegExpStringIteratorObject>, EmitError> {
        let schema = self.runtime_schema();
        let header = self.emit_native_iterator_header(
            NonArrayRealmIntrinsicSlot::RegExpStringIteratorPrototype,
            function,
        )?;
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(regexp, function),
            function,
        );
        let result = schema
            .struct_type::<RegExpStringIteratorObject>()
            .construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&stored, schema),
                    GcOperand::reference(input, schema),
                    GcOperand::boolean_local(global),
                    GcOperand::boolean_local(full_unicode),
                    GcOperand::boolean(false),
                ),
                function,
            );
        stored.clear(function);
        header.clear(function);
        Ok(result)
    }

    pub(crate) fn emit_iterator_result_object_from_locals(
        &mut self,
        value: &ValueLocals,
        done: bool,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        let done_local = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(i32::from(done)));
        done_local.store(function);
        let object =
            self.emit_iterator_result_object_in_realm(&realm, value, done_local, function)?;
        let output = schema.reserve_value_local(function);
        output.set_reference(&object, schema, function);
        result.set_normal(&output, function);
        output.clear(function);
        object.clear(function);
        schema.release_i32_local(done_local, function);
        realm.clear(function);
        Ok(())
    }

    /// Array iterator entries use the real private List-to-Array owner.
    fn emit_native_iterator_entry(
        &mut self,
        index: I64Local,
        element: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let key = schema.reserve_value_local(function);
        index.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        key.scalar().store(function);
        key.set_number(key.scalar(), function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[&key, element], function);
        let entry = self.emit_array_from_argument_list(&arguments, function)?;
        output.set_reference(&entry, schema, function);
        entry.clear(function);
        arguments.clear(function);
        key.clear(function);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    /// CreateIteratorResultObject publishes two fresh W/E/C data properties
    /// using the actual supplied execution Realm's Object prototype.
    pub(crate) fn emit_iterator_result_object_in_realm(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        value: &ValueLocals,
        done: I32Local,
        function: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &prototype,
            function,
        );
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?,
            function,
        );
        let boolean = schema.reserve_value_local(function);
        boolean.set_boolean(done, function);
        for (name, field_value) in [("value", value), ("done", &boolean)] {
            let text = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
            let key = crate::objects::PropertyKeyLocals::from_string(schema, &text, function);
            self.emit_object_append_data_property_with_flags(
                &object,
                &key,
                field_value,
                true,
                true,
                true,
                function,
            )?;
            key.clear(function);
            text.clear(function);
        }
        boolean.clear(function);
        prototype.clear(function);
        Ok(object)
    }
}

impl FunctionBuilder<'_> {
    /// AdvanceStringIndex's code-point width. Reads occur only within the
    /// validated immutable UTF-16 array; an index beyond it advances by one.
    pub(in crate::builtins) fn emit_native_iterator_string_width(
        &mut self,
        input: &GcLocal<StringValue>,
        index: I64Local,
        width: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .field(StringValueSchema::CODE_UNITS)
                .read(input, schema, function)
                .reference(),
            function,
        );
        let length = schema.reserve_i32_local(function);
        let offset = schema.reserve_i32_local(function);
        let unit = schema.reserve_i32_local(function);
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        length.store(function);
        function.instruction(&Instruction::I32Const(1));
        width.store(function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        index.load(function);
        function.instruction(&Instruction::I32WrapI64);
        offset.store(function);
        schema
            .array_type::<CodeUnitArray>()
            .read(&units, offset, schema, function)
            .store(unit, function);
        unit.load(function);
        function.instruction(&Instruction::I32Const(0xd800));
        function.instruction(&Instruction::I32GeU);
        unit.load(function);
        function.instruction(&Instruction::I32Const(0xdbff));
        function.instruction(&Instruction::I32LeU);
        function.instruction(&Instruction::I32And);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        length.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        offset.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        offset.store(function);
        schema
            .array_type::<CodeUnitArray>()
            .read(&units, offset, schema, function)
            .store(unit, function);
        unit.load(function);
        function.instruction(&Instruction::I32Const(0xdc00));
        function.instruction(&Instruction::I32GeU);
        unit.load(function);
        function.instruction(&Instruction::I32Const(0xdfff));
        function.instruction(&Instruction::I32LeU);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(2));
        width.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(unit, function);
        schema.release_i32_local(offset, function);
        schema.release_i32_local(length, function);
        units.clear(function);
    }

    pub(crate) fn emit_string_iterator_next_from_locals(
        &mut self,
        receiver: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let index = schema.reserve_i64_local(function);
        let length = schema.reserve_i32_local(function);
        let width = schema.reserve_i32_local(function);
        let cursor = schema.reserve_i32_local(function);
        let source_index = schema.reserve_i32_local(function);
        value.set_undefined(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<StringIteratorObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.emit_native_iterator_error_if(
            RuntimeErrorMessage::STRING_ITERATOR_NEXT_CALLED_ON_INCOMPATIBLE_RECEIVER,
            result,
            exit,
            function,
        )?;
        let iterator = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<StringIteratorObject>(schema, function),
            function,
        );
        schema
            .field(StringIteratorObjectSchema::DONE)
            .read(&iterator, schema, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_iterator_result_object_from_locals(&value, true, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let input = schema.reserve_gc_local(function).initialize(
            schema
                .field(StringIteratorObjectSchema::INPUT)
                .read(&iterator, schema, function)
                .reference(),
            function,
        );
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .field(StringValueSchema::CODE_UNITS)
                .read(&input, schema, function)
                .reference(),
            function,
        );
        schema
            .field(StringIteratorObjectSchema::NEXT_CODE_UNIT_INDEX)
            .read(&iterator, schema, function)
            .store_i64(index, function);
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        length.store(function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, function);
        schema.field(StringIteratorObjectSchema::DONE).write(
            &iterator,
            GcOperand::boolean(true),
            schema,
            function,
        );
        self.emit_iterator_result_object_from_locals(&value, true, result, function)?;
        function.instruction(&Instruction::Else);
        self.emit_native_iterator_string_width(&input, index, width, function);
        let construction = StringConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            width,
            function,
        );
        cursor.set_constant(0, function);
        index.load(function);
        function.instruction(&Instruction::I32WrapI64);
        source_index.store(function);
        // Width is the validated one- or two-unit code point span. Copy it
        // into private construction storage without a per-unit control loop.
        construction.copy_units_range_from(cursor, &units, source_index, width, schema, function);
        let substring = schema
            .reserve_gc_local(function)
            .initialize(construction.publish(schema, function), function);
        value.set_reference(&substring, schema, function);
        index.load(function);
        width.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Add);
        index.store(function);
        schema
            .field(StringIteratorObjectSchema::NEXT_CODE_UNIT_INDEX)
            .write(&iterator, GcOperand::i64_local(index), schema, function);
        self.emit_iterator_result_object_from_locals(&value, false, result, function)?;
        substring.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        units.clear(function);
        input.clear(function);
        iterator.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        for local in [source_index, cursor, width, length] {
            schema.release_i32_local(local, function);
        }
        schema.release_i64_local(index, function);
        value.clear(function);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_typed_array_iterator_next_from_locals(
        &mut self,
        receiver: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let element = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let index = schema.reserve_i64_local(function);
        let next_index = schema.reserve_i64_local(function);
        let length = schema.reserve_i64_local(function);
        let kind_local = schema.reserve_i32_local(function);
        value.set_undefined(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<TypedArrayIteratorObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.emit_native_iterator_error_if(
            RuntimeErrorMessage::ARRAY_ITERATOR_NEXT_CALLED_ON_INCOMPATIBLE_RECEIVER,
            result,
            exit,
            function,
        )?;
        let iterator = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<TypedArrayIteratorObject>(schema, function),
            function,
        );
        schema
            .field(TypedArrayIteratorObjectSchema::DONE)
            .read(&iterator, schema, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_iterator_result_object_from_locals(&value, true, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let array = schema.reserve_gc_local(function).initialize(
            schema
                .field(TypedArrayIteratorObjectSchema::ARRAY)
                .read(&iterator, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        schema
            .field(TypedArrayIteratorObjectSchema::NEXT_INDEX)
            .read(&iterator, schema, function)
            .store_i64(index, function);
        schema
            .field(TypedArrayIteratorObjectSchema::KIND)
            .read(&iterator, schema, function)
            .store(kind_local, function);
        // Every active next takes a new validated view witness. An invalid
        // buffer leaves both the retained array and its next index unchanged.
        self.emit_validate_typed_array_view(&array, length, &pending, function)?;
        self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, function);
        schema.field(TypedArrayIteratorObjectSchema::DONE).write(
            &iterator,
            GcOperand::boolean(true),
            schema,
            function,
        );
        schema.field(TypedArrayIteratorObjectSchema::ARRAY).write(
            &iterator,
            GcOperand::null(schema),
            schema,
            function,
        );
        self.emit_iterator_result_object_from_locals(&value, true, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        next_index.store(function);
        schema
            .field(TypedArrayIteratorObjectSchema::NEXT_INDEX)
            .write(
                &iterator,
                GcOperand::i64_local(next_index),
                schema,
                function,
            );
        let selected = self.open_frame(ControlFrameKind::Block, function);
        for kind in [
            ArrayIterationKind::Key,
            ArrayIterationKind::Value,
            ArrayIterationKind::KeyAndValue,
        ] {
            kind_local.load(function);
            function.instruction(&Instruction::I32Const(kind.encode()));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            match kind {
                ArrayIterationKind::Key => {
                    index.load(function);
                    function.instruction(&Instruction::F64ConvertI64U);
                    function.instruction(&Instruction::I64ReinterpretF64);
                    value.scalar().store(function);
                    value.set_number(value.scalar(), function);
                }
                ArrayIterationKind::Value | ArrayIterationKind::KeyAndValue => {
                    self.emit_typed_array_element_read_from_locals(
                        &array, index, &element, function,
                    )?;
                    match kind {
                        ArrayIterationKind::Value => value.copy_from(&element, function),
                        ArrayIterationKind::KeyAndValue => {
                            self.emit_native_iterator_entry(index, &element, &value, function)?
                        }
                        ArrayIterationKind::Key => unreachable!("Key performs no element read"),
                    }
                }
            }
            self.emit_branch_to_target(selected, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_iterator_result_object_from_locals(&value, false, result, function)?;
        array.clear(function);
        iterator.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(kind_local, function);
        schema.release_i64_local(length, function);
        schema.release_i64_local(next_index, function);
        schema.release_i64_local(index, function);
        pending.clear(function);
        element.clear(function);
        value.clear(function);
        Ok(())
    }

    /// The single ArrayIteratorPrototype.next entry handles both concrete
    /// iterator records; generic Array records still validate TypedArray input.
    pub(crate) fn emit_array_iterator_next_from_locals(
        &mut self,
        receiver: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let array_value = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let element = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let index = schema.reserve_i64_local(function);
        let next_index = schema.reserve_i64_local(function);
        let length = schema.reserve_i64_local(function);
        let kind_local = schema.reserve_i32_local(function);
        let typed_array_input = schema.reserve_i32_local(function);
        value.set_undefined(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<TypedArrayIteratorObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        self.emit_typed_array_iterator_next_from_locals(receiver, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ArrayIteratorObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.emit_native_iterator_error_if(
            RuntimeErrorMessage::ARRAY_ITERATOR_NEXT_CALLED_ON_INCOMPATIBLE_RECEIVER,
            result,
            exit,
            function,
        )?;
        let iterator = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<ArrayIteratorObject>(schema, function),
            function,
        );
        schema
            .field(ArrayIteratorObjectSchema::DONE)
            .read(&iterator, schema, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_iterator_result_object_from_locals(&value, true, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .field(ArrayIteratorObjectSchema::ARRAY)
                .read(&iterator, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &array_value, schema, function);
        stored.clear(function);
        schema
            .field(ArrayIteratorObjectSchema::NEXT_INDEX)
            .read(&iterator, schema, function)
            .store_i64(index, function);
        schema
            .field(ArrayIteratorObjectSchema::KIND)
            .read(&iterator, schema, function)
            .store(kind_local, function);
        array_value.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<TypedArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        typed_array_input.store(function);
        typed_array_input.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let array = schema.reserve_gc_local(function).initialize(
            array_value.cast_reference::<TypedArrayObject>(schema, function),
            function,
        );
        self.emit_validate_typed_array_view(&array, length, &pending, function)?;
        array.clear(function);
        self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
        function.instruction(&Instruction::Else);
        // LengthOfArrayLike: an actual Array's `length` Get is unobservable
        // and yields [[ArrayLength]] (see emit_actual_array_length_or_branch);
        // every other iterated object, Proxy included, keeps Get + ToLength.
        let measured = self.open_frame(ControlFrameKind::Block, function);
        let generic = self.open_frame(ControlFrameKind::Block, function);
        self.emit_actual_array_length_or_branch(&array_value, length, generic, function);
        self.emit_branch_to_target(measured, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let length_key = self.emit_iterator_named_key("length", function)?;
        self.emit_object_read(&array_value, &array_value, &length_key, &pending, function)?;
        length_key.clear(function);
        self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
        element.copy_from(pending.value(), function);
        self.emit_to_length_i64_from_value_locals(&element, length, &pending, function)?;
        self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, function);
        schema.field(ArrayIteratorObjectSchema::DONE).write(
            &iterator,
            GcOperand::boolean(true),
            schema,
            function,
        );
        let absent = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&value, function),
            function,
        );
        schema.field(ArrayIteratorObjectSchema::ARRAY).write(
            &iterator,
            GcOperand::reference(&absent, schema),
            schema,
            function,
        );
        absent.clear(function);
        self.emit_iterator_result_object_from_locals(&value, true, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // Advance before Get: an abrupt element getter must not replay this
        // index on the next call, and a reentrant next observes the successor.
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        next_index.store(function);
        schema.field(ArrayIteratorObjectSchema::NEXT_INDEX).write(
            &iterator,
            GcOperand::i64_local(next_index),
            schema,
            function,
        );
        let selected = self.open_frame(ControlFrameKind::Block, function);
        for kind in [
            ArrayIterationKind::Key,
            ArrayIterationKind::Value,
            ArrayIterationKind::KeyAndValue,
        ] {
            kind_local.load(function);
            function.instruction(&Instruction::I32Const(kind.encode()));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            match kind {
                ArrayIterationKind::Key => {
                    index.load(function);
                    function.instruction(&Instruction::F64ConvertI64U);
                    function.instruction(&Instruction::I64ReinterpretF64);
                    value.scalar().store(function);
                    value.set_number(value.scalar(), function);
                }
                ArrayIterationKind::Value | ArrayIterationKind::KeyAndValue => {
                    typed_array_input.load(function);
                    self.open_frame(ControlFrameKind::If, function);
                    let array = schema.reserve_gc_local(function).initialize(
                        array_value.cast_reference::<TypedArrayObject>(schema, function),
                        function,
                    );
                    self.emit_typed_array_element_read_from_locals(
                        &array, index, &element, function,
                    )?;
                    array.clear(function);
                    function.instruction(&Instruction::Else);
                    index.load(function);
                    function.instruction(&Instruction::F64ConvertI64U);
                    function.instruction(&Instruction::I64ReinterpretF64);
                    element.scalar().store(function);
                    let text = schema.reserve_gc_local(function).initialize(
                        self.emit_number_to_string_payload(element.scalar(), function)?,
                        function,
                    );
                    let key = PropertyKeyLocals::from_string(schema, &text, function);
                    text.clear(function);
                    self.emit_object_read(&array_value, &array_value, &key, &pending, function)?;
                    key.clear(function);
                    self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
                    element.copy_from(pending.value(), function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                    match kind {
                        ArrayIterationKind::Value => value.copy_from(&element, function),
                        ArrayIterationKind::KeyAndValue => {
                            self.emit_native_iterator_entry(index, &element, &value, function)?
                        }
                        ArrayIterationKind::Key => unreachable!("Key performs no element read"),
                    }
                }
            }
            self.emit_branch_to_target(selected, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_iterator_result_object_from_locals(&value, false, result, function)?;
        iterator.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(typed_array_input, function);
        schema.release_i32_local(kind_local, function);
        schema.release_i64_local(length, function);
        schema.release_i64_local(next_index, function);
        schema.release_i64_local(index, function);
        pending.clear(function);
        element.clear(function);
        value.clear(function);
        array_value.clear(function);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_regexp_string_iterator_next_from_locals(
        &mut self,
        receiver: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let regexp = schema.reserve_value_local(function);
        let acquired_exec = schema.reserve_value_local(function);
        let matched = schema.reserve_value_local(function);
        let element = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let index = schema.reserve_i64_local(function);
        let width = schema.reserve_i32_local(function);
        matched.set_undefined(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<RegExpStringIteratorObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.emit_native_iterator_error_if(
            RuntimeErrorMessage::REGEXP_STRING_ITERATOR_NEXT_CALLED_ON_INCOMPATIBLE_RECEIVER,
            result,
            exit,
            function,
        )?;
        let iterator = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<RegExpStringIteratorObject>(schema, function),
            function,
        );
        schema
            .field(RegExpStringIteratorObjectSchema::DONE)
            .read(&iterator, schema, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_iterator_result_object_from_locals(&matched, true, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .field(RegExpStringIteratorObjectSchema::REGEXP)
                .read(&iterator, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &regexp, schema, function);
        stored.clear(function);
        let input = schema.reserve_gc_local(function).initialize(
            schema
                .field(RegExpStringIteratorObjectSchema::INPUT)
                .read(&iterator, schema, function)
                .reference(),
            function,
        );
        // RegExpExec owns callable exec and the concrete RegExpBuiltinExec
        // fallback. This consumer performs exactly its one observable Get.
        let key = self.emit_iterator_named_key("exec", function)?;
        self.emit_object_read(&regexp, &regexp, &key, &pending, function)?;
        key.clear(function);
        self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
        acquired_exec.copy_from(pending.value(), function);
        self.emit_regexp_exec_from_values(&regexp, &input, &acquired_exec, &pending, function)?;
        self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
        matched.copy_from(pending.value(), function);
        matched.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        schema.field(RegExpStringIteratorObjectSchema::DONE).write(
            &iterator,
            GcOperand::boolean(true),
            schema,
            function,
        );
        matched.set_undefined(function);
        self.emit_iterator_result_object_from_locals(&matched, true, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .field(RegExpStringIteratorObjectSchema::GLOBAL)
            .read(&iterator, schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        schema.field(RegExpStringIteratorObjectSchema::DONE).write(
            &iterator,
            GcOperand::boolean(true),
            schema,
            function,
        );
        self.emit_iterator_result_object_from_locals(&matched, false, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let key = self.emit_iterator_named_key("0", function)?;
        self.emit_object_read(&matched, &matched, &key, &pending, function)?;
        key.clear(function);
        self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
        element.copy_from(pending.value(), function);
        self.emit_value_to_string_payload(&element, &pending, function)?;
        self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
        let match_string = schema.reserve_gc_local(function).initialize(
            pending
                .value()
                .cast_reference::<StringValue>(schema, function),
            function,
        );
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .field(StringValueSchema::CODE_UNITS)
                .read(&match_string, schema, function)
                .reference(),
            function,
        );
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        function.instruction(&Instruction::I32Eqz);
        units.clear(function);
        match_string.clear(function);
        self.open_frame(ControlFrameKind::If, function);
        let key = self.emit_iterator_named_key("lastIndex", function)?;
        self.emit_object_read(&regexp, &regexp, &key, &pending, function)?;
        self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
        element.copy_from(pending.value(), function);
        self.emit_to_length_i64_from_value_locals(&element, index, &pending, function)?;
        self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
        function.instruction(&Instruction::I32Const(1));
        width.store(function);
        schema
            .field(RegExpStringIteratorObjectSchema::FULL_UNICODE)
            .read(&iterator, schema, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_native_iterator_string_width(&input, index, width, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        index.load(function);
        width.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        element.scalar().store(function);
        element.set_number(element.scalar(), function);
        self.emit_object_write_strict(&regexp, &key, &element, &pending, function)?;
        key.clear(function);
        self.emit_native_iterator_abrupt_exit(&pending, result, exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // An abrupt exec/Get/ToString/lastIndex Set leaves Done unchanged.
        self.emit_iterator_result_object_from_locals(&matched, false, result, function)?;
        input.clear(function);
        iterator.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(width, function);
        schema.release_i64_local(index, function);
        pending.clear(function);
        element.clear(function);
        matched.clear(function);
        acquired_exec.clear(function);
        regexp.clear(function);
        Ok(())
    }
}
