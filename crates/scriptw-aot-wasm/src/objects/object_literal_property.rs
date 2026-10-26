//! The actual ordered definition body shared by ordinary and resumed literals.
use super::*;

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn compile_object_property_definition_payload(
        &mut self,
        definition: &scriptw_ir::ObjectPropertyDefinitionIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let target = schema.reserve_value_local(function);
        // The retained whole object is acquired before its property operands.
        self.compile_expr_to_value(definition.target(), &target, function)?;
        let object = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<OrdinaryObject>(schema, function),
            function,
        );
        let pending = schema.reserve_completion(function);
        let value = schema.reserve_value_local(function);
        self.emit_object_literal_property(
            definition.property(),
            &object,
            &target,
            &value,
            &pending,
            function,
        )?;
        output.copy_from(&target, function);
        value.clear(function);
        pending.clear(function);
        object.clear(function);
        target.clear(function);
        Ok(())
    }

    pub(super) fn emit_object_literal_property(
        &mut self,
        property: &ObjectPropertyIr,
        object: &GcLocal<OrdinaryObject>,
        target: &ValueLocals,
        value: &ValueLocals,
        pending: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        if let ObjectPropertyIr::PrototypeSetter { value: expression } = property {
            self.compile_expr_to_value(expression, value, function)?;
            self.emit_is_heap_object_like_tag_i32(value.tag(), function);
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32Or);
            self.open_frame(ControlFrameKind::If, function);
            let stored = schema
                .reserve_gc_local(function)
                .initialize(schema.prototype_reference(value, function), function);
            schema.field(OrdinaryObjectSchema::PROTOTYPE).write(
                object,
                GcOperand::reference(&stored, schema),
                schema,
                function,
            );
            stored.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            return Ok(());
        }
        if let ObjectPropertyIr::Spread { source } = property {
            self.compile_expr_to_value(source, value, function)?;
            self.emit_copy_data_properties_into(value, &[], target, pending, function)?;
            self.emit_literal_property_abrupt_exit(pending, function);
            return Ok(());
        }
        let key = match property {
            ObjectPropertyIr::Data { key, .. }
            | ObjectPropertyIr::NonEnumerableData { key, .. }
            | ObjectPropertyIr::Method { key, .. }
            | ObjectPropertyIr::Getter { key, .. }
            | ObjectPropertyIr::Setter { key, .. } => {
                let string = schema.reserve_gc_local(function).initialize(
                    self.emit_interned_string_reference(key, function)?,
                    function,
                );
                let key =
                    crate::operations::PropertyKeyLocals::from_string(schema, &string, function);
                string.clear(function);
                key
            }
            ObjectPropertyIr::ComputedData { key, .. }
            | ObjectPropertyIr::ComputedMethod { key, .. }
            | ObjectPropertyIr::ComputedGetter { key, .. }
            | ObjectPropertyIr::ComputedSetter { key, .. } => {
                let key_value = schema.reserve_value_local(function);
                self.compile_expr_to_value(key, &key_value, function)?;
                let key = self.emit_value_to_property_key_locals(&key_value, function)?;
                key_value.clear(function);
                key
            }
            ObjectPropertyIr::PrototypeSetter { .. } | ObjectPropertyIr::Spread { .. } => {
                unreachable!("non-property source arms are consumed above")
            }
        };
        match property {
            ObjectPropertyIr::Data {
                value: expression, ..
            }
            | ObjectPropertyIr::ComputedData {
                value: expression, ..
            } => {
                let inference = match property {
                    ObjectPropertyIr::ComputedData { name_inference, .. } => name_inference,
                    _ => &ComputedPropertyNameInferenceIr::None,
                };
                let private_key_binding = match inference {
                    ComputedPropertyNameInferenceIr::Class { key_binding } => {
                        let storage =
                            self.allocate_compiler_temporary_binding(key.value(), function);
                        self.push_scope();
                        self.binding_scopes
                            .last_mut()
                            .expect("private key owns a scope")
                            .insert(key_binding.clone(), storage);
                        Some(storage)
                    }
                    ComputedPropertyNameInferenceIr::None
                    | ComputedPropertyNameInferenceIr::Function => None,
                };
                self.compile_expr_to_value(expression, value, function)?;
                if matches!(inference, ComputedPropertyNameInferenceIr::Function) {
                    let callable = schema.reserve_gc_local(function).initialize(
                        value.cast_reference::<crate::gc_types::FunctionObject>(schema, function),
                        function,
                    );
                    self.emit_set_function_name(
                        &callable,
                        &key,
                        FunctionNamePrefix::None,
                        function,
                    )?;
                    callable.clear(function);
                }
                self.emit_create_data_property_or_throw(target, &key, value, pending, function)?;
                if let Some(storage) = private_key_binding {
                    self.pop_scope();
                    let BindingStorage::Local(id) = storage else {
                        unreachable!("compiler temporary is a body-owned local")
                    };
                    self.release_local_binding(id, function);
                }
            }
            ObjectPropertyIr::NonEnumerableData {
                key: name,
                value: expression,
            } => {
                self.compile_expr_to_value(expression, value, function)?;
                let (writable, configurable) = match name.as_str() {
                    "lastIndex" => (true, false),
                    "source" | "flags" | "hasIndices" | "global" | "ignoreCase" | "multiline"
                    | "dotAll" | "unicode" | "sticky" => (false, true),
                    _ => (true, true),
                };
                self.emit_object_define_data_with_configurable(
                    target,
                    &key,
                    value,
                    writable,
                    false,
                    configurable,
                    pending,
                    function,
                )?;
            }
            ObjectPropertyIr::Method {
                function: method, ..
            }
            | ObjectPropertyIr::ComputedMethod {
                function: method, ..
            } => {
                self.emit_object_method_value_to_locals(method, object, &key, value, function)?;
                self.emit_create_data_property_or_throw(target, &key, value, pending, function)?;
            }
            ObjectPropertyIr::Getter {
                function: getter, ..
            }
            | ObjectPropertyIr::ComputedGetter {
                function: getter, ..
            } => {
                self.emit_object_method_value_to_locals(getter, object, &key, value, function)?;
                let enumerable = schema.reserve_i32_local(function);
                function.instruction(&Instruction::I32Const(1));
                enumerable.store(function);
                self.emit_object_define_accessor_with_flag_local(
                    target,
                    &key,
                    AccessorDescriptorLocals::Getter(AccessorGetterLocals::new(value)),
                    enumerable,
                    enumerable,
                    pending,
                    function,
                )?;
                schema.release_i32_local(enumerable, function);
            }
            ObjectPropertyIr::Setter {
                function: setter, ..
            }
            | ObjectPropertyIr::ComputedSetter {
                function: setter, ..
            } => {
                self.emit_object_method_value_to_locals(setter, object, &key, value, function)?;
                let enumerable = schema.reserve_i32_local(function);
                function.instruction(&Instruction::I32Const(1));
                enumerable.store(function);
                self.emit_object_define_accessor_with_flag_local(
                    target,
                    &key,
                    AccessorDescriptorLocals::Setter(AccessorSetterLocals::new(value)),
                    enumerable,
                    enumerable,
                    pending,
                    function,
                )?;
                schema.release_i32_local(enumerable, function);
            }
            ObjectPropertyIr::PrototypeSetter { .. } | ObjectPropertyIr::Spread { .. } => {
                unreachable!("non-property source arms are consumed above")
            }
        }
        key.clear(function);
        self.emit_literal_property_abrupt_exit(pending, function);
        Ok(())
    }
}
