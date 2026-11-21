use super::*;
use crate::gc_types::GcI32Constant;

pub(crate) enum TypedArraySpeciesLengthMethod {
    Map,
    Filter,
    Slice,
}

impl TypedArraySpeciesLengthMethod {
    fn receiver_message(&self) -> RuntimeErrorMessage {
        match self {
            Self::Map => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_MAP_REQUIRES_A_TYPEDARRAY,
            Self::Filter => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FILTER_REQUIRES_A_TYPEDARRAY,
            Self::Slice => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SLICE_REQUIRES_A_TYPEDARRAY,
        }
    }
    fn constructor_message(&self) -> RuntimeErrorMessage {
        match self {
            Self::Map => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_MAP_CONSTRUCTOR_PROPERTY_IS_NOT_AN_OBJECT,
            Self::Filter => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FILTER_CONSTRUCTOR_PROPERTY_IS_NOT_AN_OBJECT,
            Self::Slice => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SLICE_CONSTRUCTOR_PROPERTY_IS_NOT_AN_OBJECT,
        }
    }
    fn species_message(&self) -> RuntimeErrorMessage {
        match self {
            Self::Map => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_MAP_SPECIES_IS_NOT_A_CONSTRUCTOR,
            Self::Filter => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FILTER_SPECIES_IS_NOT_A_CONSTRUCTOR
            }
            Self::Slice => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SLICE_SPECIES_IS_NOT_A_CONSTRUCTOR
            }
        }
    }
    fn content_message(&self) -> RuntimeErrorMessage {
        match self {
            Self::Map => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_MAP_SPECIES_CONTENT_TYPE_DIFFERS,
            Self::Filter => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FILTER_SPECIES_CONTENT_TYPE_DIFFERS
            }
            Self::Slice => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SLICE_SPECIES_CONTENT_TYPE_DIFFERS
            }
        }
    }
}

// The entry snapshot retains the source array, element kind and length.
// Element access later reacquires current bounds after observable callbacks.
struct SpeciesSource {
    method: TypedArraySpeciesLengthMethod,
    receiver: ValueLocals,
    array: GcLocal<crate::gc_types::TypedArrayObject>,
    length: crate::gc_types::I64Local,
    kind: I32Local,
}
impl SpeciesSource {
    fn clear(self, schema: &crate::gc_types::RuntimeSchema, function: &mut Function) {
        schema.release_i32_local(self.kind, function);
        schema.release_i64_local(self.length, function);
        self.array.clear(function);
        self.receiver.clear(function);
    }
}
#[must_use]
pub(crate) struct TypedArraySpeciesSource(SpeciesSource);
impl TypedArraySpeciesSource {
    pub(crate) fn length_local(&self) -> crate::gc_types::I64Local {
        self.0.length
    }
    pub(crate) fn element_kind_local(&self) -> I32Local {
        self.0.kind
    }
    pub(crate) fn array(&self) -> &GcLocal<crate::gc_types::TypedArrayObject> {
        &self.0.array
    }
    pub(crate) fn clear(self, schema: &crate::gc_types::RuntimeSchema, function: &mut Function) {
        self.0.clear(schema, function);
    }
}
#[must_use]
pub(crate) struct TypedArraySpeciesResult {
    array: GcLocal<crate::gc_types::TypedArrayObject>,
    kind: I32Local,
}
impl TypedArraySpeciesResult {
    pub(crate) fn element_kind_local(&self) -> I32Local {
        self.kind
    }
    pub(crate) fn array(&self) -> &GcLocal<crate::gc_types::TypedArrayObject> {
        &self.array
    }
}

impl FunctionBuilder<'_> {
    fn emit_species_completion_or_return(
        &mut self,
        result: &CompletionLocals,
        function: &mut Function,
    ) {
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(result, function);
        self.emit_return_current_completion(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    fn emit_acquire_typed_array_species_source(
        &mut self,
        method: TypedArraySpeciesLengthMethod,
        input: &ValueLocals,
        function: &mut Function,
    ) -> Result<SpeciesSource, EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        receiver.copy_from(input, function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        input.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::TypedArrayObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            method.receiver_message(),
            &pending,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_species_completion_or_return(&pending, function);
        let array = schema.reserve_gc_local(function).initialize(
            input.cast_reference::<crate::gc_types::TypedArrayObject>(schema, function),
            function,
        );
        let length = schema.reserve_i64_local(function);
        let kind = schema.reserve_i32_local(function);
        schema
            .field(crate::gc_types::TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&array, schema, function)
            .store(kind, function);
        self.emit_validate_typed_array_view(&array, length, &pending, function)?;
        self.emit_species_completion_or_return(&pending, function);
        pending.clear(function);
        Ok(SpeciesSource {
            method,
            receiver,
            array,
            length,
            kind,
        })
    }

    pub(crate) fn emit_typed_array_species_length_source(
        &mut self,
        method: TypedArraySpeciesLengthMethod,
        receiver: &ValueLocals,
        function: &mut Function,
    ) -> Result<TypedArraySpeciesSource, EmitError> {
        Ok(TypedArraySpeciesSource(
            self.emit_acquire_typed_array_species_source(method, receiver, function)?,
        ))
    }
    pub(crate) fn emit_typed_array_species_create_with_length(
        &mut self,
        source: &TypedArraySpeciesSource,
        length: crate::gc_types::I64Local,
        function: &mut Function,
    ) -> Result<TypedArraySpeciesResult, EmitError> {
        self.emit_typed_array_species_create(&source.0, length, function)
    }
    fn emit_typed_array_species_create(
        &mut self,
        source: &SpeciesSource,
        length: crate::gc_types::I64Local,
        function: &mut Function,
    ) -> Result<TypedArraySpeciesResult, EmitError> {
        let schema = self.runtime_schema();
        let constructor = schema.reserve_value_local(function);
        let property = schema.reserve_completion(function);
        let species = schema.reserve_completion(function);
        let target = schema.reserve_completion(function);
        let default =
            self.emit_current_function_realm_typed_array_constructor(source.kind, function)?;
        constructor.copy_from(default.value(), function);
        default.clear(function);
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("constructor", function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &string, function);
        string.clear(function);
        self.emit_object_read(
            &source.receiver,
            &source.receiver,
            &key,
            &property,
            function,
        )?;
        key.clear(function);
        self.emit_species_completion_or_return(&property, function);
        property.value().tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_is_heap_object_like_tag_i32(property.value().tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            source.method.constructor_message(),
            &property,
            function,
        )?;
        self.emit_species_completion_or_return(&property, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(scriptw_ir::WellKnownSymbol::Species, function)?,
            function,
        );
        let key = PropertyKeyLocals::from_symbol(schema, &symbol, function);
        symbol.clear(function);
        self.emit_object_read(property.value(), property.value(), &key, &species, function)?;
        key.clear(function);
        self.emit_species_completion_or_return(&species, function);
        self.compile_nullish_tagged_i32(species.value().tag(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_is_constructor_i32(species.value(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            source.method.species_message(),
            &species,
            function,
        )?;
        self.emit_species_completion_or_return(&species, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        constructor.copy_from(species.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let value = schema.reserve_value_local(function);
        let bits = schema.reserve_i64_local(function);
        length.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        bits.store(function);
        value.set_number(bits, function);
        let list = self.emit_pre_evaluated_arg_vector(&[&value], function);
        self.emit_function_or_proxy_construct_with_argv(
            &constructor,
            &constructor,
            &list,
            &target,
            function,
        )?;
        list.clear(function);
        schema.release_i64_local(bits, function);
        value.clear(function);
        self.emit_species_completion_or_return(&target, function);
        let checked = schema.reserve_completion(function);
        self.emit_validate_typed_array_species_target(target.value(), length, &checked, function)?;
        self.emit_species_completion_or_return(&checked, function);
        checked.clear(function);
        let array = schema.reserve_gc_local(function).initialize(
            target
                .value()
                .cast_reference::<crate::gc_types::TypedArrayObject>(schema, function),
            function,
        );
        let kind = schema.reserve_i32_local(function);
        schema
            .field(crate::gc_types::TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&array, schema, function)
            .store(kind, function);
        self.emit_typed_array_bigint_element_kind_i32(source.kind, function);
        self.emit_typed_array_bigint_element_kind_i32(kind, function);
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            source.method.content_message(),
            &target,
            function,
        )?;
        self.emit_species_completion_or_return(&target, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        target.clear(function);
        species.clear(function);
        property.clear(function);
        constructor.clear(function);
        Ok(TypedArraySpeciesResult { array, kind })
    }

    pub(crate) fn emit_typed_array_bigint_element_kind_i32(
        &self,
        kind: I32Local,
        function: &mut Function,
    ) {
        crate::gc_types::emit_i32_in(
            kind,
            TypedArrayElementKind::ALL
                .iter()
                .copied()
                .filter(|element| element.content_type() == TypedArrayContentType::BigInt)
                .map(|element| element.encode()),
            function,
        );
    }
    pub(crate) fn emit_typed_array_species_element_write(
        &mut self,
        target: &TypedArraySpeciesResult,
        index: crate::gc_types::I64Local,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_typed_array_element_write_from_locals(
            &target.array,
            index,
            value,
            result,
            function,
        )
    }
    pub(crate) fn emit_publish_typed_array_species_result(
        &self,
        target: TypedArraySpeciesResult,
        result: &CompletionLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        result
            .value()
            .set_reference(&target.array, schema, function);
        result.set_normal(result.value(), function);
        schema.release_i32_local(target.kind, function);
        target.array.clear(function);
    }
    fn emit_validate_typed_array_species_target(
        &mut self,
        target: &ValueLocals,
        requested_length: crate::gc_types::I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        result.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::TypedArrayObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::CONSTRUCTED_TARGET_IS_NOT_A_TYPED_ARRAY,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let array = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<crate::gc_types::TypedArrayObject>(schema, function),
            function,
        );
        let length = schema.reserve_i64_local(function);
        self.emit_validate_typed_array_write_view(&array, length, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        length.load(function);
        requested_length.load(function);
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::CONSTRUCTED_TYPED_ARRAY_IS_TOO_SMALL,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(length, function);
        array.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }
}
