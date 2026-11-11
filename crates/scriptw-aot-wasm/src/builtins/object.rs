//! Native Object algorithms consume the shared GC internal-method owners.
use super::super::*;
use crate::functions::{NonArrayRealmIntrinsicSlot, OrdinaryDefaultPrototype};
use crate::gc_types::{
    ArgumentsObject, CompletionLocals, DateObject, GcLocal, GcNullability,
    NativeErrorObject, OrdinaryObject, PrimitiveBox, PrimitiveBoxSchema, RegExpObject, StoredValue,
    StringValue, ValueLocals,
};
use crate::objects::{DescriptorFlag, DescriptorObjectFields, PropertyKeyLocals};
use scriptw_ir::property_descriptor::Presence;

mod assign;
mod define_properties;
mod define_property;
mod enumerable_own_properties;
mod get_own_property_descriptor;
mod get_own_property_descriptors;
mod integrity_test;
mod object_to_locale_string_invoke;
mod own_descriptor_predicate;
mod prototype_definition;
mod prototype_lookup;

#[derive(Clone, Copy)]
enum NativeOwnKeyKind {
    String,
    Symbol,
}

#[derive(Clone, Copy)]
enum NativeIntegrityLevel {
    Sealed,
    Frozen,
}

#[derive(Clone, Copy)]
enum NativePrototypeSet {
    ObjectFunction,
    LegacyAccessor,
}

impl FunctionBuilder<'_> {
    fn emit_native_object_abrupt_exit(
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

    fn emit_native_object_error_if(
        &mut self,
        message: RuntimeErrorMessage,
        output: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(message, output, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_native_object_nullish(value: &ValueLocals, function: &mut Function) {
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Eq);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
    }

    fn emit_native_default_object(
        &mut self,
        function: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &prototype,
            function,
        );
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?,
            function,
        );
        prototype.clear(function);
        realm.clear(function);
        Ok(object)
    }

    pub(super) fn compile_object_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let new_target = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        self.emit_builtin_arg_to_value(0, &argument, function);
        self.compile_new_target_to_locals(&new_target, function)?;
        let exit = self.open_frame(ControlFrameKind::Block, function);
        new_target.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        let active = self
            .body_entry_locals()
            .and_then(|entry| entry.function_object())
            .expect("Object constructor requires its actual callable entry");
        new_target.reference().load(function);
        active.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_get_prototype_from_constructor(
            &new_target,
            OrdinaryDefaultPrototype::Object,
            &output,
            function,
        )?;
        self.emit_native_object_abrupt_exit(&output, &output, exit, function);
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(output.value()), function)?,
            function,
        );
        let value = schema.reserve_value_local(function);
        value.set_reference(&object, schema, function);
        output.set_normal(&value, function);
        value.clear(function);
        object.clear(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Self::emit_native_object_nullish(&argument, function);
        self.open_frame(ControlFrameKind::If, function);
        let object = self.emit_native_default_object(function)?;
        let value = schema.reserve_value_local(function);
        value.set_reference(&object, schema, function);
        output.set_normal(&value, function);
        value.clear(function);
        object.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_value_to_object_locals(&argument, &output, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        output.clear(function);
        new_target.clear(function);
        argument.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_create_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        let properties = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        self.emit_builtin_arg_to_value(0, &prototype, function);
        self.emit_builtin_arg_to_value(1, &properties, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_heap_object_like_tag_i32(prototype.tag(), function);
        prototype.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        self.emit_native_object_error_if(
            RuntimeErrorMessage::OBJECT_CREATE_PROTOTYPE_MUST_BE_OBJECT_OR_NULL,
            &output,
            exit,
            function,
        )?;
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?,
            function,
        );
        value.set_reference(&object, schema, function);
        output.set_normal(&value, function);
        properties.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_define_properties_from_values(
            define_properties::ObjectDefinitionTarget(&value),
            &properties,
            &output,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        object.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        output.clear(function);
        value.clear(function);
        properties.clear(function);
        prototype.clear(function);
        Ok(())
    }

    fn emit_native_object_get_prototype(
        &mut self,
        receiver: &ValueLocals,
        output: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let object = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_value_to_object_locals(receiver, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, output, exit, function);
        object.copy_from(pending.value(), function);
        self.emit_object_get_prototype_of(&object, output, function)?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(function);
        object.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_get_prototype_of_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        self.emit_builtin_arg_to_value(0, &receiver, function);
        self.emit_native_object_get_prototype(&receiver, &output, function)?;
        self.completion().copy_from(&output, function);
        output.clear(function);
        receiver.clear(function);
        Ok(())
    }

    fn emit_native_object_set_prototype(
        &mut self,
        mode: NativePrototypeSet,
        target: &ValueLocals,
        prototype: &ValueLocals,
        output: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        Self::emit_native_object_nullish(target, function);
        self.emit_native_object_error_if(
            RuntimeErrorMessage::CANNOT_CONVERT_UNDEFINED_OR_NULL_TO_OBJECT,
            output,
            exit,
            function,
        )?;
        match mode {
            NativePrototypeSet::ObjectFunction => output.set_normal(target, function),
            NativePrototypeSet::LegacyAccessor => {
                output.value().set_undefined(function);
                output.set_normal(output.value(), function);
            }
        }
        self.emit_is_heap_object_like_tag_i32(prototype.tag(), function);
        prototype.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        match mode {
            NativePrototypeSet::ObjectFunction => {
                self.emit_throw_current_function_realm_type_error(
                    RuntimeErrorMessage::OBJECT_SETPROTOTYPEOF_PROTOTYPE_MUST_BE_OBJECT_OR_NULL,
                    output,
                    function,
                )?;
            }
            NativePrototypeSet::LegacyAccessor => {}
        }
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_heap_object_like_tag_i32(target.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_set_prototype_of(target, prototype, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, output, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.emit_native_object_error_if(
            RuntimeErrorMessage::OBJECT_SETPROTOTYPEOF_RETURNED_FALSE,
            output,
            exit,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_set_prototype_of_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let target = schema.reserve_value_local(function);
        let prototype = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        self.emit_builtin_arg_to_value(0, &target, function);
        self.emit_builtin_arg_to_value(1, &prototype, function);
        self.emit_native_object_set_prototype(
            NativePrototypeSet::ObjectFunction,
            &target,
            &prototype,
            &output,
            function,
        )?;
        self.completion().copy_from(&output, function);
        output.clear(function);
        prototype.clear(function);
        target.clear(function);
        Ok(())
    }

    fn emit_native_object_own_keys_builtin(
        &mut self,
        kind: NativeOwnKeyKind,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let object = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        let pending = schema.reserve_completion(function);
        self.emit_builtin_arg_to_value(0, &argument, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_value_to_object_locals(&argument, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, &output, exit, function);
        object.copy_from(pending.value(), function);
        let keys = self.emit_object_own_property_keys(&object, function)?;
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
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
        let tag = match kind {
            NativeOwnKeyKind::String => WasmRuntimeValueTag::String,
            NativeOwnKeyKind::Symbol => WasmRuntimeValueTag::Symbol,
        };
        function.instruction(&Instruction::I32Const(tag.tag()));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        array.append(key.value(), self, function)?;
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
        let published = array.finish(self, function);
        value.set_reference(&published, schema, function);
        output.set_normal(&value, function);
        published.clear(function);
        keys.clear(function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        pending.clear(function);
        output.clear(function);
        value.clear(function);
        object.clear(function);
        argument.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_get_own_property_names_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_object_own_keys_builtin(NativeOwnKeyKind::String, function)
    }

    pub(super) fn compile_object_get_own_property_symbols_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_object_own_keys_builtin(NativeOwnKeyKind::Symbol, function)
    }

    pub(super) fn compile_object_is_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let truth = schema.reserve_i32_local(function);
        self.emit_builtin_arg_to_value(0, &left, function);
        self.emit_builtin_arg_to_value(1, &right, function);
        self.emit_tagged_payload_same_value_i32(&left, &right, function)?;
        truth.store(function);
        value.set_boolean(truth, function);
        self.completion().set_normal(&value, function);
        schema.release_i32_local(truth, function);
        value.clear(function);
        right.clear(function);
        left.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_is_extensible_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let target = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        self.emit_builtin_arg_to_value(0, &target, function);
        self.emit_is_heap_object_like_tag_i32(target.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_is_extensible(&target, &output, function)?;
        function.instruction(&Instruction::Else);
        output
            .value()
            .set_scalar(crate::gc_types::ScalarValue::Boolean(false), function);
        output.set_normal(output.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        output.clear(function);
        target.clear(function);
        Ok(())
    }

    fn emit_native_object_set_integrity(
        &mut self,
        level: NativeIntegrityLevel,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let target = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        let pending = schema.reserve_completion(function);
        self.emit_builtin_arg_to_value(0, &target, function);
        output.set_normal(&target, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_heap_object_like_tag_i32(target.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, function);
        self.emit_object_prevent_extensions(&target, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, &output, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.emit_native_object_error_if(RuntimeErrorMessage::TYPEERROR, &output, exit, function)?;
        let keys = self.emit_object_own_property_keys(&target, function)?;
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        keys.length(count, schema, function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, function);
        let key = keys.read_key(index, self, function)?;
        let mut fields = DescriptorObjectFields {
            value: Presence::Absent,
            writable: Presence::Absent,
            get: Presence::Absent,
            set: Presence::Absent,
            enumerable: Presence::Absent,
            configurable: Presence::Present(DescriptorFlag::Known(false)),
        };
        match level {
            NativeIntegrityLevel::Sealed => {
                self.emit_object_define_entry_validated(
                    &target,
                    &key,
                    &fields.from_runtime_checked(),
                    &pending,
                    function,
                )?;
            }
            NativeIntegrityLevel::Frozen => {
                let descriptor = self.emit_proxy_target_own_descriptor(&target, &key, function)?;
                descriptor.emit_found_i32(schema, function);
                self.open_frame(ControlFrameKind::If, function);
                let data = schema.reserve_i32_local(function);
                descriptor.emit_accessor_i32(schema, function);
                function.instruction(&Instruction::I32Eqz);
                data.store(function);
                fields.writable = Presence::Runtime {
                    present: data,
                    value: DescriptorFlag::Known(false),
                };
                self.emit_object_define_entry_validated(
                    &target,
                    &key,
                    &fields.from_runtime_checked(),
                    &pending,
                    function,
                )?;
                schema.release_i32_local(data, function);
                function.instruction(&Instruction::Else);
                pending
                    .value()
                    .set_scalar(crate::gc_types::ScalarValue::Boolean(true), function);
                pending.set_normal(pending.value(), function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                descriptor.clear(function);
            }
        }
        key.clear(function);
        self.emit_native_object_abrupt_exit(&pending, &output, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.emit_native_object_error_if(RuntimeErrorMessage::TYPEERROR, &output, exit, function)?;
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
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        pending.clear(function);
        output.clear(function);
        target.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_seal_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_object_set_integrity(NativeIntegrityLevel::Sealed, function)
    }

    pub(super) fn compile_object_freeze_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_object_set_integrity(NativeIntegrityLevel::Frozen, function)
    }

    pub(super) fn compile_object_prevent_extensions_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let target = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        let pending = schema.reserve_completion(function);
        self.emit_builtin_arg_to_value(0, &target, function);
        output.set_normal(&target, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_heap_object_like_tag_i32(target.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_prevent_extensions(&target, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, &output, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.emit_native_object_error_if(RuntimeErrorMessage::TYPEERROR, &output, exit, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        pending.clear(function);
        output.clear(function);
        target.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_prototype_proto_getter_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        self.compile_this_to_locals(&receiver, function)?;
        self.emit_native_object_get_prototype(&receiver, &output, function)?;
        self.completion().copy_from(&output, function);
        output.clear(function);
        receiver.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_prototype_proto_setter_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let prototype = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        self.compile_this_to_locals(&receiver, function)?;
        self.emit_builtin_arg_to_value(0, &prototype, function);
        self.emit_native_object_set_prototype(
            NativePrototypeSet::LegacyAccessor,
            &receiver,
            &prototype,
            &output,
            function,
        )?;
        self.completion().copy_from(&output, function);
        output.clear(function);
        prototype.clear(function);
        receiver.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_prototype_is_prototype_of_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let object = schema.reserve_value_local(function);
        let candidate = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        let pending = schema.reserve_completion(function);
        self.compile_this_to_locals(&receiver, function)?;
        self.emit_builtin_arg_to_value(0, &candidate, function);
        value.set_scalar(crate::gc_types::ScalarValue::Boolean(false), function);
        output.set_normal(&value, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_heap_object_like_tag_i32(candidate.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, function);
        self.emit_value_to_object_locals(&receiver, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, &output, exit, function);
        object.copy_from(pending.value(), function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        self.emit_object_get_prototype_of(&candidate, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, &output, exit, function);
        pending.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
        function.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(exit, function);
        candidate.copy_from(pending.value(), function);
        self.emit_tagged_payload_same_value_i32(&object, &candidate, function)?;
        self.open_frame(ControlFrameKind::If, function);
        value.set_scalar(crate::gc_types::ScalarValue::Boolean(true), function);
        output.set_normal(&value, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        pending.clear(function);
        output.clear(function);
        value.clear(function);
        candidate.clear(function);
        object.clear(function);
        receiver.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_prototype_to_string_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let object = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        let pending = schema.reserve_completion(function);
        let is_array = schema.reserve_i32_local(function);
        self.compile_this_to_locals(&receiver, function)?;
        let exit = self.open_frame(ControlFrameKind::Block, function);
        for (tag, text) in [
            (WasmRuntimeValueTag::Undefined, "[object Undefined]"),
            (WasmRuntimeValueTag::Null, "[object Null]"),
        ] {
            receiver.tag().load(function);
            function.instruction(&Instruction::I32Const(tag.tag()));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            let string = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(text, function)?,
                function,
            );
            value.set_reference(&string, schema, function);
            output.set_normal(&value, function);
            string.clear(function);
            self.emit_branch_to_target(exit, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.emit_value_to_object_locals(&receiver, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, &output, exit, function);
        object.copy_from(pending.value(), function);
        self.emit_is_array_i32(&object, is_array, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, &output, exit, function);
        let label = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("Object", function)?,
            function,
        );
        is_array.load(function);
        self.open_frame(ControlFrameKind::If, function);
        label.replace(
            self.emit_interned_string_reference("Array", function)?,
            function,
        );
        function.instruction(&Instruction::Else);
        // These concrete brands are private internal slots. A Proxy does not
        // expose its target's slots; IsArray above is the required exception.
        macro_rules! concrete_brand {
            ($record:ty, $name:literal) => {
                object.reference().load(function);
                function.instruction(&Instruction::RefTestNonNull(
                    schema
                        .reference_type::<$record>(GcNullability::NonNullable)
                        .heap_type,
                ));
                self.open_frame(ControlFrameKind::If, function);
                label.replace(
                    self.emit_interned_string_reference($name, function)?,
                    function,
                );
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            };
        }
        concrete_brand!(ArgumentsObject, "Arguments");
        self.emit_is_callable_i32(&object, function)?;
        self.open_frame(ControlFrameKind::If, function);
        label.replace(
            self.emit_interned_string_reference("Function", function)?,
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        concrete_brand!(NativeErrorObject, "Error");
        object.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<PrimitiveBox>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let boxed = schema.reserve_gc_local(function).initialize(
            object.cast_reference::<PrimitiveBox>(schema, function),
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
        let primitive = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &primitive, schema, function);
        for (tag, name) in [
            (WasmRuntimeValueTag::Boolean, "Boolean"),
            (WasmRuntimeValueTag::Number, "Number"),
            (WasmRuntimeValueTag::String, "String"),
        ] {
            primitive.tag().load(function);
            function.instruction(&Instruction::I32Const(tag.tag()));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            label.replace(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        primitive.clear(function);
        stored.clear(function);
        boxed.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        concrete_brand!(DateObject, "Date");
        concrete_brand!(RegExpObject, "RegExp");
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(
                scriptw_ir::WellKnownSymbol::ToStringTag,
                function,
            )?,
            function,
        );
        let key = PropertyKeyLocals::from_symbol(schema, &symbol, function);
        self.emit_object_read(&object, &object, &key, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, &output, exit, function);
        pending.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        label.replace(
            pending
                .value()
                .cast_reference::<StringValue>(schema, function),
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let prefix = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("[object ", function)?,
            function,
        );
        let suffix = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("]", function)?,
            function,
        );
        let first = schema.reserve_gc_local(function).initialize(
            self.emit_concat_gc_strings(&prefix, &label, function),
            function,
        );
        let complete = schema.reserve_gc_local(function).initialize(
            self.emit_concat_gc_strings(&first, &suffix, function),
            function,
        );
        value.set_reference(&complete, schema, function);
        output.set_normal(&value, function);
        complete.clear(function);
        first.clear(function);
        suffix.clear(function);
        prefix.clear(function);
        key.clear(function);
        symbol.clear(function);
        label.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        schema.release_i32_local(is_array, function);
        pending.clear(function);
        output.clear(function);
        value.clear(function);
        object.clear(function);
        receiver.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_prototype_value_of_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        self.compile_this_to_locals(&receiver, function)?;
        self.emit_value_to_object_locals(&receiver, &output, function)?;
        self.completion().copy_from(&output, function);
        output.clear(function);
        receiver.clear(function);
        Ok(())
    }
}
