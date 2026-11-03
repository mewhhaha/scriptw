//! Reflect invokes the shared internal-method owners with whole GC values.
use super::super::*;
use crate::functions::NativeObjectAlgorithm;
use crate::gc_types::{CompletionLocals, ValueLocals};
use crate::objects::PropertyKeyLocals;

#[derive(Clone, Copy)]
enum ReflectPropertyBuiltin {
    Get,
    Set,
    Has,
    DefineProperty,
    DeleteProperty,
    GetOwnPropertyDescriptor,
}
impl ReflectPropertyBuiltin {
    fn target_error(self) -> RuntimeErrorMessage {
        match self {
            Self::Get => RuntimeErrorMessage::REFLECT_GET_TARGET_MUST_BE_OBJECT,
            Self::Set => RuntimeErrorMessage::REFLECT_SET_TARGET_MUST_BE_OBJECT,
            Self::Has => RuntimeErrorMessage::REFLECT_HAS_TARGET_MUST_BE_OBJECT,
            Self::DefineProperty => {
                RuntimeErrorMessage::REFLECT_DEFINEPROPERTY_TARGET_MUST_BE_OBJECT
            }
            Self::DeleteProperty => {
                RuntimeErrorMessage::REFLECT_DELETEPROPERTY_TARGET_MUST_BE_OBJECT
            }
            Self::GetOwnPropertyDescriptor => {
                RuntimeErrorMessage::REFLECT_GETOWNPROPERTYDESCRIPTOR_TARGET_MUST_BE_OBJECT
            }
        }
    }
}
#[derive(Clone, Copy)]
enum ReflectObjectBuiltin {
    GetPrototypeOf,
    SetPrototypeOf,
    OwnKeys,
    IsExtensible,
    PreventExtensions,
}
impl ReflectObjectBuiltin {
    fn target_error(self) -> RuntimeErrorMessage {
        match self {
            Self::GetPrototypeOf => {
                RuntimeErrorMessage::REFLECT_GETPROTOTYPEOF_TARGET_MUST_BE_OBJECT
            }
            Self::SetPrototypeOf => {
                RuntimeErrorMessage::OBJECT_SETPROTOTYPEOF_TARGET_MUST_BE_OBJECT
            }
            Self::OwnKeys => RuntimeErrorMessage::REFLECT_OWNKEYS_TARGET_MUST_BE_OBJECT,
            Self::IsExtensible => RuntimeErrorMessage::REFLECT_ISEXTENSIBLE_TARGET_MUST_BE_OBJECT,
            Self::PreventExtensions => {
                RuntimeErrorMessage::REFLECT_PREVENTEXTENSIONS_TARGET_MUST_BE_OBJECT
            }
        }
    }
}
#[derive(Clone, Copy)]
enum ReflectInvokeBuiltin {
    Apply,
    Construct,
}

enum ReflectReceiverOperation {
    Get,
    Set,
}

/// Created only inside the successful native target check. Property and object
/// method emitters accept this witness, so they cannot accidentally box a
/// primitive target or coerce its key before rejecting the target.
struct ReflectObjectTarget<'v>(&'v ValueLocals);

impl FunctionBuilder<'_> {
    fn emit_with_reflect_object_target(
        &mut self,
        error: RuntimeErrorMessage,
        result: &CompletionLocals,
        function: &mut Function,
        consume: impl FnOnce(
            &mut Self,
            ReflectObjectTarget<'_>,
            &CompletionLocals,
            &mut Function,
        ) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let target = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &target, function);
        self.emit_is_heap_object_like_tag_i32(target.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        consume(self, ReflectObjectTarget(&target), result, function)?;
        function.instruction(&Instruction::Else);
        self.emit_throw_current_function_realm_type_error(error, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        target.clear(function);
        Ok(())
    }

    fn emit_reflect_receiver(
        &mut self,
        target: &ValueLocals,
        operation: ReflectReceiverOperation,
        function: &mut Function,
    ) -> ValueLocals {
        let receiver = self.runtime_schema().reserve_value_local(function);
        let argument = match operation {
            ReflectReceiverOperation::Get => 2,
            ReflectReceiverOperation::Set => 3,
        };
        self.emit_builtin_arg_is_present_i32(argument, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_builtin_arg_to_value(argument, &receiver, function);
        function.instruction(&Instruction::Else);
        receiver.copy_from(target, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        receiver
    }

    fn emit_reflect_property_builtin(
        &mut self,
        builtin: ReflectPropertyBuiltin,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(function);
        result.initialize(function);
        self.emit_with_reflect_object_target(
            builtin.target_error(),
            &result,
            function,
            |builder, target, result, function| {
                let raw_key = schema.reserve_value_local(function);
                builder.emit_builtin_arg_to_value(1, &raw_key, function);
                let key = builder.emit_value_to_property_key_locals(&raw_key, function)?;
                raw_key.clear(function);
                match builtin {
                    ReflectPropertyBuiltin::Get => {
                        let receiver = builder.emit_reflect_receiver(
                            target.0,
                            ReflectReceiverOperation::Get,
                            function,
                        );
                        builder.emit_object_read(target.0, &receiver, &key, result, function)?;
                        receiver.clear(function);
                    }
                    ReflectPropertyBuiltin::Set => {
                        let receiver = builder.emit_reflect_receiver(
                            target.0,
                            ReflectReceiverOperation::Set,
                            function,
                        );
                        let value = schema.reserve_value_local(function);
                        builder.emit_builtin_arg_to_value(2, &value, function);
                        builder.emit_ordinary_set_result(
                            target.0, &receiver, &key, &value, result, function,
                        )?;
                        value.clear(function);
                        receiver.clear(function);
                    }
                    ReflectPropertyBuiltin::Has => {
                        schema
                            .call_helper(
                                crate::runtime_helpers::ObjectHasPropertyArguments::new(
                                    target.0,
                                    &key,
                                    builder.current_environment(),
                                    builder.current_program(),
                                ),
                                builder.runtime_helper_base()?,
                                function,
                            )
                            .store(result, function);
                    }
                    ReflectPropertyBuiltin::DefineProperty => {
                        let attributes = schema.reserve_value_local(function);
                        builder.emit_builtin_arg_to_value(2, &attributes, function);
                        let converted = builder.emit_to_property_descriptor(
                            &attributes,
                            RuntimeErrorMessage::REFLECT_DEFINEPROPERTY_ATTRIBUTES_MUST_BE_OBJECT,
                            function,
                        )?;
                        builder.emit_object_define_entry_validated(
                            target.0,
                            &key,
                            &converted.definition_descriptor(),
                            result,
                            function,
                        )?;
                        converted.clear(schema, function);
                        attributes.clear(function);
                    }
                    ReflectPropertyBuiltin::DeleteProperty => {
                        builder.emit_object_delete(target.0, &key, result, function)?
                    }
                    ReflectPropertyBuiltin::GetOwnPropertyDescriptor => {
                        // Run the sole GPD algorithm in this executing native
                        // Realm. Its private result is already the fresh
                        // FromPropertyDescriptor object or Undefined.
                        builder.emit_native_object_algorithm_call(
                            NativeObjectAlgorithm::GetOwnPropertyDescriptor,
                            &[target.0, key.value()],
                            result,
                            function,
                        )?;
                    }
                }
                key.clear(function);
                Ok(())
            },
        )?;
        self.completion().copy_from(&result, function);
        result.clear(function);
        Ok(())
    }

    fn emit_reflect_object_builtin(
        &mut self,
        builtin: ReflectObjectBuiltin,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(function);
        result.initialize(function);
        self.emit_with_reflect_object_target(builtin.target_error(), &result, function,
            |builder, target, result, function| {
                match builtin {
                    ReflectObjectBuiltin::GetPrototypeOf => builder.emit_object_get_prototype_of(target.0, result, function)?,
                    ReflectObjectBuiltin::SetPrototypeOf => {
                        let prototype = schema.reserve_value_local(function);
                        builder.emit_builtin_arg_to_value(1, &prototype, function);
                        prototype.tag().load(function);
                        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
                        function.instruction(&Instruction::I32Eq);
                        builder.emit_is_heap_object_like_tag_i32(prototype.tag(), function);
                        function.instruction(&Instruction::I32Or);
                        builder.open_frame(ControlFrameKind::If, function);
                        builder.emit_object_set_prototype_of(target.0, &prototype, result, function)?;
                        function.instruction(&Instruction::Else);
                        builder.emit_throw_current_function_realm_type_error(
                            RuntimeErrorMessage::OBJECT_SETPROTOTYPEOF_PROTOTYPE_MUST_BE_OBJECT_OR_NULL,
                            result, function,
                        )?;
                        builder.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                        prototype.clear(function);
                    }
                    ReflectObjectBuiltin::OwnKeys => builder.emit_reflect_own_keys_result(target, result, function)?,
                    ReflectObjectBuiltin::IsExtensible => builder.emit_object_is_extensible(target.0, result, function)?,
                    ReflectObjectBuiltin::PreventExtensions => builder.emit_object_prevent_extensions(target.0, result, function)?,
                }
                Ok(())
            },
        )?;
        self.completion().copy_from(&result, function);
        result.clear(function);
        Ok(())
    }

    fn emit_reflect_own_keys_result(
        &mut self,
        target: ReflectObjectTarget<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        // Only the public builtin materializes the accepted List as an Array.
        let keys = self.emit_object_own_property_keys(target.0, function)?;
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let length = schema.reserve_i64_local(function);
        let bits = schema.reserve_i64_local(function);
        keys.length(count, schema, function);
        count.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        length.store(function);
        let prototype = self.emit_load_current_function_realm_array_prototype(function);
        let array = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_array_with_current_function_realm_prototype(
                length, prototype, function,
            )?,
            function,
        );
        let value = schema.reserve_value_local(function);
        value.set_reference(&array, schema, function);
        let pending = schema.reserve_completion(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let iteration = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, function);
        let key = keys.read_key(index, self, function)?;
        index.load(function);
        function.instruction(&Instruction::F64ConvertI32U);
        function.instruction(&Instruction::I64ReinterpretF64);
        bits.store(function);
        let text = schema
            .call_helper(
                crate::runtime_helpers::NumberToStringArguments::new(bits),
                self.runtime_helper_base()?,
                function,
            )
            .bind(schema, schema.reserve_gc_local(function), function);
        let index_key = PropertyKeyLocals::from_string(schema, &text, function);
        self.emit_create_data_property_or_throw(
            &value,
            &index_key,
            key.value(),
            &pending,
            function,
        )?;
        text.clear(function);
        index_key.clear(function);
        key.clear(function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(&pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(iteration, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(&value, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        value.clear(function);
        array.clear(function);
        schema.release_i64_local(bits, function);
        schema.release_i64_local(length, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        keys.clear(function);
        Ok(())
    }

    fn emit_reflect_invoke_builtin(
        &mut self,
        builtin: ReflectInvokeBuiltin,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let target = schema.reserve_value_local(function);
        let argument_list = schema.reserve_value_local(function);
        let receiver_or_new_target = schema.reserve_value_local(function);
        let result = schema.reserve_completion(function);
        result.initialize(function);
        self.emit_builtin_arg_to_value(0, &target, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        match builtin {
            ReflectInvokeBuiltin::Apply => self.emit_is_callable_i32(&target, function)?,
            ReflectInvokeBuiltin::Construct => self.emit_is_constructor_i32(&target, function),
        }
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let message = match builtin {
            ReflectInvokeBuiltin::Apply => {
                RuntimeErrorMessage::REFLECT_APPLY_TARGET_MUST_BE_CALLABLE
            }
            ReflectInvokeBuiltin::Construct => {
                RuntimeErrorMessage::REFLECT_CONSTRUCT_TARGET_IS_NOT_A_CONSTRUCTOR
            }
        };
        self.emit_throw_current_function_realm_type_error(message, &result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let list_error = match builtin {
            ReflectInvokeBuiltin::Apply => {
                self.emit_builtin_arg_to_value(1, &receiver_or_new_target, function);
                self.emit_builtin_arg_to_value(2, &argument_list, function);
                RuntimeErrorMessage::REFLECT_APPLY_ARGUMENTSLIST_MUST_BE_ARRAY_LIKE
            }
            ReflectInvokeBuiltin::Construct => {
                self.emit_builtin_arg_to_value(1, &argument_list, function);
                self.emit_builtin_arg_is_present_i32(2, function);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_builtin_arg_to_value(2, &receiver_or_new_target, function);
                function.instruction(&Instruction::Else);
                receiver_or_new_target.copy_from(&target, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.emit_is_constructor_i32(&receiver_or_new_target, function);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_throw_current_function_realm_type_error(
                    RuntimeErrorMessage::REFLECT_CONSTRUCT_NEWTARGET_IS_NOT_A_CONSTRUCTOR,
                    &result,
                    function,
                )?;
                self.emit_branch_to_target(exit, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                RuntimeErrorMessage::REFLECT_CONSTRUCT_ARGUMENTSLIST_MUST_BE_ARRAY_LIKE
            }
        };
        self.emit_with_array_like_argument_vector(
            &argument_list,
            list_error,
            &result,
            function,
            |builder, arguments, output, function| match builtin {
                ReflectInvokeBuiltin::Apply => builder.emit_prepared_tail_call(
                    &target,
                    &receiver_or_new_target,
                    arguments,
                    function,
                ),
                ReflectInvokeBuiltin::Construct => builder
                    .emit_function_or_proxy_construct_with_argv(
                        &target,
                        &receiver_or_new_target,
                        arguments,
                        output,
                        function,
                    ),
            },
        )?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&result, function);
        result.clear(function);
        receiver_or_new_target.clear(function);
        argument_list.clear(function);
        target.clear(function);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_reflect_get_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_property_builtin(ReflectPropertyBuiltin::Get, function)
    }
    pub(crate) fn compile_reflect_set_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_property_builtin(ReflectPropertyBuiltin::Set, function)
    }
    pub(crate) fn compile_reflect_has_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_property_builtin(ReflectPropertyBuiltin::Has, function)
    }
    pub(crate) fn compile_reflect_define_property_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_property_builtin(ReflectPropertyBuiltin::DefineProperty, function)
    }
    pub(crate) fn compile_reflect_delete_property_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_property_builtin(ReflectPropertyBuiltin::DeleteProperty, function)
    }
    pub(crate) fn compile_reflect_get_own_property_descriptor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_property_builtin(
            ReflectPropertyBuiltin::GetOwnPropertyDescriptor,
            function,
        )
    }
    pub(crate) fn compile_reflect_get_prototype_of_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_object_builtin(ReflectObjectBuiltin::GetPrototypeOf, function)
    }
    pub(crate) fn compile_reflect_set_prototype_of_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_object_builtin(ReflectObjectBuiltin::SetPrototypeOf, function)
    }
    pub(crate) fn compile_reflect_own_keys_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_object_builtin(ReflectObjectBuiltin::OwnKeys, function)
    }
    pub(crate) fn compile_reflect_is_extensible_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_object_builtin(ReflectObjectBuiltin::IsExtensible, function)
    }
    pub(crate) fn compile_reflect_prevent_extensions_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_object_builtin(ReflectObjectBuiltin::PreventExtensions, function)
    }
    pub(crate) fn compile_reflect_apply_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_invoke_builtin(ReflectInvokeBuiltin::Apply, function)
    }
    pub(crate) fn compile_reflect_construct_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_reflect_invoke_builtin(ReflectInvokeBuiltin::Construct, function)
    }
}
