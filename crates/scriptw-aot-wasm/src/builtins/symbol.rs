//! Symbol identities and the Agent registry are strong GC records.

use super::super::*;
use crate::gc_types::*;

enum SymbolBuiltin {
    Constructor,
    For,
    KeyFor,
    PrototypeDescriptionGetter,
    PrototypeToString,
    PrototypeValueOf,
    PrototypeToPrimitive,
}

enum SymbolReceiverOperation {
    Description,
    ToString,
    ValueOf,
    ToPrimitive,
}
impl SymbolReceiverOperation {
    const fn receiver_error_message(&self) -> RuntimeErrorMessage {
        match self {
        Self::Description => RuntimeErrorMessage::SYMBOL_PROTOTYPE_DESCRIPTION_REQUIRES_THAT_THIS_BE_A_SYMBOL,
        Self::ToString => RuntimeErrorMessage::SYMBOL_PROTOTYPE_TOSTRING_REQUIRES_THAT_THIS_BE_A_SYMBOL,
        Self::ValueOf => RuntimeErrorMessage::SYMBOL_PROTOTYPE_VALUEOF_REQUIRES_THAT_THIS_BE_A_SYMBOL,
        Self::ToPrimitive => RuntimeErrorMessage::SYMBOL_PROTOTYPE_SYMBOL_TOPRIMITIVE_REQUIRES_THAT_THIS_BE_A_SYMBOL,
    }
    }
}

impl FunctionBuilder<'_> {
    /// thisSymbolValue accepts a primitive or the actual PrimitiveBox brand;
    /// a Proxy or a different primitive box never impersonates [[SymbolData]].
    fn emit_this_symbol_value(
        &mut self,
        receiver: &ValueLocals,
        operation: &SymbolReceiverOperation,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<GcLocal<SymbolValue, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.copy_from(receiver, function);
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<PrimitiveBox>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let boxed = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<PrimitiveBox>(schema, function),
            function,
        );
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PrimitiveBox>()
                .field(PrimitiveBoxSchema::PRIMITIVE)
                .read(&boxed, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &value, schema, function);
        stored.clear(function);
        boxed.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let symbol = schema
            .reserve_gc_local::<SymbolValue, Nullable>(function)
            .initialize_null(schema, function);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        symbol.replace(
            value
                .cast_reference::<SymbolValue>(schema, function)
                .nullable(),
            function,
        );
        function.instruction(&Instruction::Else);
        self.emit_throw_current_function_realm_type_error(
            operation.receiver_error_message(),
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.clear(function);
        Ok(symbol)
    }

    pub(super) fn emit_symbol_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_symbol(SymbolBuiltin::Constructor, function)
    }
    pub(super) fn emit_symbol_for_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_symbol(SymbolBuiltin::For, function)
    }
    pub(super) fn emit_symbol_key_for_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_symbol(SymbolBuiltin::KeyFor, function)
    }
    pub(super) fn emit_symbol_prototype_description_getter_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_symbol(SymbolBuiltin::PrototypeDescriptionGetter, function)
    }
    pub(super) fn emit_symbol_prototype_to_string_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_symbol(SymbolBuiltin::PrototypeToString, function)
    }
    pub(super) fn emit_symbol_prototype_value_of_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_symbol(SymbolBuiltin::PrototypeValueOf, function)
    }
    pub(super) fn emit_symbol_prototype_to_primitive_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_symbol(SymbolBuiltin::PrototypeToPrimitive, function)
    }

    fn emit_symbol(
        &mut self,
        builtin: SymbolBuiltin,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(function);
        result.initialize(function);
        match builtin {
            SymbolBuiltin::Constructor => {
                let new_target = schema.reserve_value_local(function);
                new_target.copy_from(
                    self.body_entry_locals()
                        .expect("Symbol builtin owns an entry")
                        .new_target(),
                    function,
                );
                new_target.tag().load(function);
                function.instruction(&Instruction::I32Const(
                    WasmRuntimeValueTag::Undefined as i32,
                ));
                function.instruction(&Instruction::I32Ne);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_throw_current_function_realm_type_error(
                    RuntimeErrorMessage::SYMBOL_IS_NOT_A_CONSTRUCTOR,
                    &result,
                    function,
                )?;
                function.instruction(&Instruction::Else);
                let argument = schema.reserve_value_local(function);
                self.emit_builtin_arg_to_value(0, &argument, function);
                let description = schema
                    .reserve_gc_local::<StringValue, Nullable>(function)
                    .initialize_null(schema, function);
                argument.tag().load(function);
                function.instruction(&Instruction::I32Const(
                    WasmRuntimeValueTag::Undefined as i32,
                ));
                function.instruction(&Instruction::I32Ne);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_value_to_string_payload(&argument, &result, function)?;
                result.kind().load(function);
                function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                description.replace(
                    result
                        .value()
                        .cast_reference::<StringValue>(schema, function)
                        .nullable(),
                    function,
                );
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                result.kind().load(function);
                function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                let symbol = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<SymbolValue>().construct(
                        (
                            GcOperand::reference(&description, schema),
                            GcOperand::null(schema),
                            GcOperand::i64(0),
                        ),
                        function,
                    ),
                    function,
                );
                result.initialize(function);
                result.value().set_reference(&symbol, schema, function);
                symbol.clear(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                description.clear(function);
                argument.clear(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                new_target.clear(function);
            }
            SymbolBuiltin::For => {
                let argument = schema.reserve_value_local(function);
                self.emit_builtin_arg_to_value(0, &argument, function);
                self.emit_value_to_string_payload(&argument, &result, function)?;
                result.kind().load(function);
                function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                let key = schema.reserve_gc_local(function).initialize(
                    result
                        .value()
                        .cast_reference::<StringValue>(schema, function),
                    function,
                );
                let registry = schema.load_symbol_registry(function);
                let index = schema.reserve_i32_local(function);
                let length = schema.reserve_i32_local(function);
                registry.load(schema, function).is_null(function);
                function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                function.instruction(&Instruction::I32Const(0));
                function.instruction(&Instruction::Else);
                schema
                    .array_type::<RegisteredSymbolTable>()
                    .length(&registry, schema, function);
                function.instruction(&Instruction::End);
                length.store(function);
                function.instruction(&Instruction::I32Const(0));
                index.store(function);
                let found = schema
                    .reserve_gc_local::<SymbolValue, Nullable>(function)
                    .initialize_null(schema, function);
                self.open_frame(ControlFrameKind::Block, function);
                let search = self.open_frame(ControlFrameKind::Loop, function);
                index.load(function);
                length.load(function);
                function.instruction(&Instruction::I32GeU);
                found.load(schema, function).is_null(function);
                function.instruction(&Instruction::I32Eqz);
                function.instruction(&Instruction::I32Or);
                function.instruction(&Instruction::BrIf(1));
                let candidate = schema.reserve_gc_local(function).initialize(
                    schema
                        .array_type::<RegisteredSymbolTable>()
                        .read(&registry, index, schema, function)
                        .reference(),
                    function,
                );
                let candidate_key = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<SymbolValue>()
                        .field(SymbolValueSchema::REGISTRY_KEY)
                        .read(&candidate, schema, function)
                        .reference()
                        .require_non_null(function),
                    function,
                );
                self.emit_string_payload_equality_i32(&key, &candidate_key, function);
                self.open_frame(ControlFrameKind::If, function);
                found.replace(candidate.load(schema, function).nullable(), function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                candidate_key.clear(function);
                candidate.clear(function);
                index.load(function);
                function.instruction(&Instruction::I32Const(1));
                function.instruction(&Instruction::I32Add);
                index.store(function);
                self.emit_branch_to_target(search, function);
                self.pop_control(ControlFrameKind::Loop);
                function.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                found.load(schema, function).is_null(function);
                self.open_frame(ControlFrameKind::If, function);
                let symbol = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<SymbolValue>().construct(
                        (
                            GcOperand::nullable_reference(&key, schema),
                            GcOperand::nullable_reference(&key, schema),
                            GcOperand::i64(0),
                        ),
                        function,
                    ),
                    function,
                );
                length.load(function);
                function.instruction(&Instruction::I32Const(-1));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                function.instruction(&Instruction::Unreachable);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                let expanded_length = schema.reserve_i32_local(function);
                length.load(function);
                function.instruction(&Instruction::I32Const(1));
                function.instruction(&Instruction::I32Add);
                expanded_length.store(function);
                let expanded = schema.reserve_gc_local(function).initialize(
                    schema.array_type::<RegisteredSymbolTable>().filled(
                        GcOperand::reference(&symbol, schema),
                        expanded_length,
                        function,
                    ),
                    function,
                );
                function.instruction(&Instruction::I32Const(0));
                index.store(function);
                self.open_frame(ControlFrameKind::Block, function);
                let copy = self.open_frame(ControlFrameKind::Loop, function);
                index.load(function);
                length.load(function);
                function.instruction(&Instruction::I32GeU);
                function.instruction(&Instruction::BrIf(1));
                let retained = schema.reserve_gc_local(function).initialize(
                    schema
                        .array_type::<RegisteredSymbolTable>()
                        .read(&registry, index, schema, function)
                        .reference(),
                    function,
                );
                schema.array_type::<RegisteredSymbolTable>().write(
                    &expanded,
                    index,
                    GcOperand::reference(&retained, schema),
                    schema,
                    function,
                );
                retained.clear(function);
                index.load(function);
                function.instruction(&Instruction::I32Const(1));
                function.instruction(&Instruction::I32Add);
                index.store(function);
                self.emit_branch_to_target(copy, function);
                self.pop_control(ControlFrameKind::Loop);
                function.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                schema.replace_symbol_registry(&expanded, function);
                found.replace(symbol.load(schema, function).nullable(), function);
                expanded.clear(function);
                schema.release_i32_local(expanded_length, function);
                symbol.clear(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                let symbol = schema.reserve_gc_local(function).initialize(
                    found.load(schema, function).require_non_null(function),
                    function,
                );
                result.initialize(function);
                result.value().set_reference(&symbol, schema, function);
                symbol.clear(function);
                found.clear(function);
                schema.release_i32_local(length, function);
                schema.release_i32_local(index, function);
                registry.clear(function);
                key.clear(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                argument.clear(function);
            }
            SymbolBuiltin::KeyFor => {
                let argument = schema.reserve_value_local(function);
                self.emit_builtin_arg_to_value(0, &argument, function);
                argument.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
                function.instruction(&Instruction::I32Ne);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_throw_current_function_realm_type_error(
                    RuntimeErrorMessage::SYMBOL_KEYFOR_ARGUMENT_MUST_BE_A_SYMBOL,
                    &result,
                    function,
                )?;
                function.instruction(&Instruction::Else);
                let symbol = schema.reserve_gc_local(function).initialize(
                    argument.cast_reference::<SymbolValue>(schema, function),
                    function,
                );
                let key = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<SymbolValue>()
                        .field(SymbolValueSchema::REGISTRY_KEY)
                        .read(&symbol, schema, function)
                        .reference(),
                    function,
                );
                key.load(schema, function).is_null(function);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                let string = schema.reserve_gc_local(function).initialize(
                    key.load(schema, function).require_non_null(function),
                    function,
                );
                result.value().set_reference(&string, schema, function);
                string.clear(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                key.clear(function);
                symbol.clear(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                argument.clear(function);
            }
            SymbolBuiltin::PrototypeDescriptionGetter
            | SymbolBuiltin::PrototypeToString
            | SymbolBuiltin::PrototypeValueOf
            | SymbolBuiltin::PrototypeToPrimitive => {
                let operation = match builtin {
                    SymbolBuiltin::PrototypeDescriptionGetter => {
                        SymbolReceiverOperation::Description
                    }
                    SymbolBuiltin::PrototypeToString => SymbolReceiverOperation::ToString,
                    SymbolBuiltin::PrototypeValueOf => SymbolReceiverOperation::ValueOf,
                    SymbolBuiltin::PrototypeToPrimitive => SymbolReceiverOperation::ToPrimitive,
                    SymbolBuiltin::Constructor | SymbolBuiltin::For | SymbolBuiltin::KeyFor => {
                        unreachable!()
                    }
                };
                let receiver = schema.reserve_value_local(function);
                receiver.copy_from(
                    self.body_entry_locals()
                        .expect("Symbol method owns an entry")
                        .this_value(),
                    function,
                );
                let resolved =
                    self.emit_this_symbol_value(&receiver, &operation, &result, function)?;
                result.kind().load(function);
                function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                let symbol = schema.reserve_gc_local(function).initialize(
                    resolved.load(schema, function).require_non_null(function),
                    function,
                );
                match operation {
                    SymbolReceiverOperation::Description => {
                        let description = schema.reserve_gc_local(function).initialize(
                            schema
                                .struct_type::<SymbolValue>()
                                .field(SymbolValueSchema::DESCRIPTION)
                                .read(&symbol, schema, function)
                                .reference(),
                            function,
                        );
                        description.load(schema, function).is_null(function);
                        function.instruction(&Instruction::I32Eqz);
                        self.open_frame(ControlFrameKind::If, function);
                        let string = schema.reserve_gc_local(function).initialize(
                            description
                                .load(schema, function)
                                .require_non_null(function),
                            function,
                        );
                        result.value().set_reference(&string, schema, function);
                        string.clear(function);
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                        description.clear(function);
                    }
                    SymbolReceiverOperation::ToString => {
                        let string = schema.reserve_gc_local(function).initialize(
                            self.emit_symbol_descriptive_string(&symbol, function)?,
                            function,
                        );
                        result.value().set_reference(&string, schema, function);
                        string.clear(function);
                    }
                    SymbolReceiverOperation::ValueOf | SymbolReceiverOperation::ToPrimitive => {
                        result.value().set_reference(&symbol, schema, function)
                    }
                }
                symbol.clear(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                resolved.clear(function);
                receiver.clear(function);
            }
        }
        self.completion().copy_from(&result, function);
        result.clear(function);
        Ok(())
    }
}
