use super::*;
use crate::gc_types::{NamedBinding, PropertyDescriptor, PropertyDescriptorSchema};
use crate::operations::PropertyKeyLocals;
use scriptw_ir::{PreparedScriptKind, PreparedScriptUnit};

mod name_loop;
mod shared_checks;

#[derive(Clone, Copy)]
enum GlobalDeclarationAdmission {
    Function,
    Var,
}

#[derive(Clone, Copy)]
enum GlobalDeclarationFailure {
    ExistingLexical,
    RestrictedProperty,
    FunctionNotDefinable,
    VarNotDefinable,
}

/// Admission retains the exact Environment and global object that publication
/// uses. Neither names nor descriptors are acquired again to choose a target.
#[must_use]
struct ValidatedGlobalDeclarationInstantiation<'a> {
    unit: &'a PreparedScriptUnit,
    names: crate::data::GlobalVarDeclarationTables,
    environment: GcLocal<Environment>,
    object: ValueLocals,
    realm: GcLocal<RealmRecord>,
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_validate_main_global_lexical_declarations(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let bindings = self
            .script_global_bindings
            .expect("main global binding plan");
        // The closed initial-global descriptor plan excludes source var/function
        // overlap, so these are pre-existing non-configurable object bindings.
        if bindings.has_restricted_lexical_declarations() {
            self.emit_global_declaration_failure(
                GlobalDeclarationFailure::RestrictedProperty,
                function,
            )?;
        }
        Ok(())
    }

    pub(crate) fn emit_instantiate_prepared_script_declarations(
        &mut self,
        unit: &PreparedScriptUnit,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match &unit.kind {
            PreparedScriptKind::DirectEval(_)
            | PreparedScriptKind::IndirectEval
            | PreparedScriptKind::ShadowRealmEvaluate
                if unit.strict =>
            {
                self.emit_initialize_strict_eval_variable_cells(unit, function)
            }
            PreparedScriptKind::DirectEval(_) => {
                let schema = self.runtime_schema();
                let variable = schema
                    .reserve_gc_local::<Environment, Nullable>(function)
                    .initialize(
                        self.body_entry_locals()
                            .expect("prepared Script entry")
                            .variable_environment()
                            .expect("direct eval variable Environment")
                            .load(schema, function),
                        function,
                    );
                self.emit_instantiate_direct_eval_declarations(unit, &variable, function)?;
                variable.clear(function);
                Ok(())
            }
            PreparedScriptKind::RealmScript
            | PreparedScriptKind::IndirectEval
            | PreparedScriptKind::ShadowRealmEvaluate => {
                self.emit_instantiate_global_declaration_plan(unit, function)
            }
        }
    }

    pub(crate) fn emit_instantiate_global_declaration_plan(
        &mut self,
        unit: &PreparedScriptUnit,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let environment = self.emit_source_global_environment_to_local(function);
        let object = schema.reserve_value_local(function);
        self.emit_execution_global_object_to_locals(&object, function);
        let realm = self.emit_execution_realm(function);
        let validated =
            self.emit_validate_global_declarations(unit, environment, object, realm, function)?;
        self.emit_install_global_declarations(validated, function)
    }

    fn emit_validate_global_declarations<'a>(
        &mut self,
        unit: &'a PreparedScriptUnit,
        environment: GcLocal<Environment>,
        object: ValueLocals,
        realm: GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<ValidatedGlobalDeclarationInstantiation<'a>, EmitError> {
        let schema = self.runtime_schema();
        let plan = &unit.declarations;
        let names = self.strings.global_var_declaration_tables(unit)?;
        for candidate in &plan.annex_b_candidates {
            self.binding_scopes
                .last_mut()
                .expect("prepared Script binding scope")
                .insert(
                    candidate.admission.name.clone(),
                    BindingStorage::EnvSlot {
                        slot: candidate.admission.slot,
                        hops: 0,
                    },
                );
        }
        let entry = schema
            .reserve_gc_local::<NamedBinding, Nullable>(function)
            .initialize_null(schema, function);
        let admitted = schema.reserve_i32_local(function);
        let value = schema.reserve_value_local(function);
        if unit.kind == PreparedScriptKind::RealmScript {
            for name in &plan.lexical_names_in_source_order {
                let key = schema
                    .reserve_gc_local::<StringValue, NonNullable>(function)
                    .initialize(
                        self.emit_interned_string_reference(name, function)?,
                        function,
                    );
                self.emit_reject_existing_global_lexical(&key, &environment, &realm, function)?;
                let property_key = PropertyKeyLocals::from_string(schema, &key, function);
                let descriptor =
                    self.emit_direct_own_descriptor_fact(&object, &property_key, function)?;
                descriptor.load(schema, function);
                function.instruction(&Instruction::RefIsNull);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_global_descriptor_flag(
                    &descriptor,
                    DescriptorMask::CONFIGURABLE,
                    function,
                );
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_global_declaration_failure(
                    GlobalDeclarationFailure::RestrictedProperty,
                    function,
                )?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                descriptor.clear(function);
                property_key.clear(function);
                key.clear(function);
            }
        }
        for declaration in &plan.functions_in_reverse_order {
            let name = &declaration.name;
            let key = schema
                .reserve_gc_local::<StringValue, NonNullable>(function)
                .initialize(
                    self.emit_interned_string_reference(name, function)?,
                    function,
                );
            self.emit_reject_existing_global_lexical(&key, &environment, &realm, function)?;
            key.clear(function);
        }
        self.emit_global_var_name_loop(
            names.lexical_checks,
            function,
            |builder, key, function| {
                builder.emit_reject_existing_global_lexical(key, &environment, &realm, function)
            },
        )?;
        for declaration in &plan.functions_in_reverse_order {
            let key = schema
                .reserve_gc_local::<StringValue, NonNullable>(function)
                .initialize(
                    self.emit_interned_string_reference(&declaration.name, function)?,
                    function,
                );
            let property_key = PropertyKeyLocals::from_string(schema, &key, function);
            self.emit_can_declare_global(
                GlobalDeclarationAdmission::Function,
                &object,
                &property_key,
                admitted,
                &realm,
                function,
            )?;
            admitted.load(function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_global_declaration_failure(
                GlobalDeclarationFailure::FunctionNotDefinable,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            property_key.clear(function);
            key.clear(function);
        }
        self.emit_global_var_name_loop(
            names.variable_bindings,
            function,
            |builder, key, function| {
                let property_key = PropertyKeyLocals::from_string(schema, key, function);
                builder.emit_can_declare_global(
                    GlobalDeclarationAdmission::Var,
                    &object,
                    &property_key,
                    admitted,
                    &realm,
                    function,
                )?;
                admitted.load(function);
                function.instruction(&Instruction::I32Eqz);
                builder.open_frame(ControlFrameKind::If, function);
                builder.emit_global_declaration_failure(
                    GlobalDeclarationFailure::VarNotDefinable,
                    function,
                )?;
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                property_key.clear(function);
                Ok(())
            },
        )?;
        for candidate in &plan.annex_b_candidates {
            if matches!(&unit.kind, PreparedScriptKind::DirectEval(_)) {
                let initialized =
                    self.read_env_slot_to_locals(candidate.admission.slot, 0, &value, function);
                schema.release_i32_local(initialized, function);
                value.scalar().load(function);
                function.instruction(&Instruction::I64Eqz);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
            }
            let key = schema
                .reserve_gc_local::<StringValue, NonNullable>(function)
                .initialize(
                    self.emit_interned_string_reference(&candidate.name, function)?,
                    function,
                );
            let property_key = PropertyKeyLocals::from_string(schema, &key, function);
            function.instruction(&Instruction::I32Const(0));
            admitted.store(function);
            self.emit_global_lexical_entry_to_local(&key, &entry, function);
            entry.load(schema, function);
            function.instruction(&Instruction::RefIsNull);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_can_declare_global(
                GlobalDeclarationAdmission::Var,
                &object,
                &property_key,
                admitted,
                &realm,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            value.set_boolean(admitted, function);
            self.write_env_slot_from_locals(candidate.admission.slot, 0, &value, function);
            property_key.clear(function);
            key.clear(function);
            if matches!(&unit.kind, PreparedScriptKind::DirectEval(_)) {
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        value.clear(function);
        schema.release_i32_local(admitted, function);
        entry.clear(function);
        Ok(ValidatedGlobalDeclarationInstantiation {
            unit,
            names,
            environment,
            object,
            realm,
        })
    }

    /// Called only inside a non-null descriptor branch; absence is never a
    /// zero flags sentinel, because an all-false data descriptor is present.
    fn emit_global_descriptor_flag(
        &self,
        descriptor: &GcLocal<PropertyDescriptor, Nullable>,
        mask: DescriptorMask,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(descriptor, schema, function);
        function.instruction(&Instruction::I32Const(mask.as_i32()));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32Ne);
    }

    fn emit_can_declare_global_inline(
        &mut self,
        admission: GlobalDeclarationAdmission,
        object: &ValueLocals,
        key: &PropertyKeyLocals,
        admitted: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let descriptor = self.emit_direct_own_descriptor_fact(object, key, function)?;
        descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        match admission {
            GlobalDeclarationAdmission::Var => {
                function.instruction(&Instruction::I32Const(1));
            }
            GlobalDeclarationAdmission::Function => {
                self.emit_global_descriptor_flag(
                    &descriptor,
                    DescriptorMask::CONFIGURABLE,
                    function,
                );
                self.emit_global_descriptor_flag(&descriptor, DescriptorMask::ACCESSOR, function);
                function.instruction(&Instruction::I32Eqz);
                self.emit_global_descriptor_flag(&descriptor, DescriptorMask::WRITABLE, function);
                function.instruction(&Instruction::I32And);
                self.emit_global_descriptor_flag(&descriptor, DescriptorMask::ENUMERABLE, function);
                function.instruction(&Instruction::I32And);
                function.instruction(&Instruction::I32Or);
            }
        }
        admitted.store(function);
        function.instruction(&Instruction::Else);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectIsExtensibleArguments::new(
                    object,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(self.completion(), function);
        self.emit_propagate_current_throw_if_needed(function);
        self.completion().value().scalar().load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        admitted.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        Ok(())
    }

    fn emit_global_declaration_failure(
        &mut self,
        failure: GlobalDeclarationFailure,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let (name, message) = match failure {
            GlobalDeclarationFailure::ExistingLexical => (NativeErrorKind::SyntaxError,
                RuntimeErrorMessage::GLOBAL_DECLARATION_CONFLICTS_WITH_EXISTING_LEXICAL_BINDING),
            GlobalDeclarationFailure::RestrictedProperty => (NativeErrorKind::SyntaxError,
                RuntimeErrorMessage::GLOBAL_LEXICAL_DECLARATION_CONFLICTS_WITH_NON_CONFIGURABLE_PROPERTY),
            GlobalDeclarationFailure::FunctionNotDefinable => (NativeErrorKind::TypeError,
                RuntimeErrorMessage::GLOBAL_FUNCTION_DECLARATION_IS_NOT_PERMITTED),
            GlobalDeclarationFailure::VarNotDefinable => (NativeErrorKind::TypeError,
                RuntimeErrorMessage::GLOBAL_VARIABLE_DECLARATION_IS_NOT_PERMITTED),
        };
        let error = self.runtime_schema().reserve_value_local(function);
        self.emit_environment_native_error(name, message, &error, function)?;
        // Main declaration admission precedes its job checkpoint, so this
        // early return must publish the same intrinsic constructor diagnostic
        // as the final main completion path. Prepared Script callers retain
        // their Throw record for the calling body to observe.
        if self.is_main() {
            self.emit_capture_final_throw_constructor_name(function)?;
        }
        self.emit_return_current_completion(function);
        error.clear(function);
        Ok(())
    }

    fn emit_install_global_declarations(
        &mut self,
        validated: ValidatedGlobalDeclarationInstantiation<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let ValidatedGlobalDeclarationInstantiation {
            unit,
            names,
            environment,
            object,
            realm,
        } = validated;
        let schema = self.runtime_schema();
        let plan = &unit.declarations;
        let value = schema.reserve_value_local(function);
        let deletable = match &unit.kind {
            PreparedScriptKind::RealmScript => false,
            PreparedScriptKind::IndirectEval
            | PreparedScriptKind::DirectEval(_)
            | PreparedScriptKind::ShadowRealmEvaluate => true,
        };
        for candidate in &plan.annex_b_candidates {
            let initialized =
                self.read_env_slot_to_locals(candidate.admission.slot, 0, &value, function);
            schema.release_i32_local(initialized, function);
            value.scalar().load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            if !plan
                .functions_in_reverse_order
                .iter()
                .any(|declaration| declaration.name == candidate.name)
                && !plan.var_names.contains(&candidate.name)
            {
                let key = schema
                    .reserve_gc_local::<StringValue, NonNullable>(function)
                    .initialize(
                        self.emit_interned_string_reference(&candidate.name, function)?,
                        function,
                    );
                let property_key = PropertyKeyLocals::from_string(schema, &key, function);
                self.emit_create_global_var_binding(
                    &object,
                    &property_key,
                    deletable,
                    &realm,
                    function,
                )?;
                property_key.clear(function);
                key.clear(function);
            }
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        if unit.kind == PreparedScriptKind::RealmScript {
            let cells = self.resolve_env_handle_local(0, function);
            self.emit_append_global_lexical_cells(
                &unit.global_bindings,
                &unit.owned_env_bindings,
                &environment,
                &cells,
                function,
            )?;
            cells.clear(function);
        }
        for declaration in plan.functions_in_reverse_order.iter().rev() {
            let meta = self
                .functions
                .get(&declaration.function_id)
                .cloned()
                .ok_or_else(|| {
                    EmitError::unsupported(format!(
                        "prepared global declaration `{}` lacks function `{}`",
                        declaration.name, declaration.function_id
                    ))
                })?;
            let callable = schema
                .reserve_gc_local::<FunctionObject, NonNullable>(function)
                .initialize(self.emit_function_value_payload(&meta, function)?, function);
            value.set_reference(&callable, schema, function);
            let key = schema
                .reserve_gc_local::<StringValue, NonNullable>(function)
                .initialize(
                    self.emit_interned_string_reference(&declaration.name, function)?,
                    function,
                );
            let property_key = PropertyKeyLocals::from_string(schema, &key, function);
            self.emit_create_global_function_binding(
                &object,
                &property_key,
                &value,
                deletable,
                function,
            )?;
            property_key.clear(function);
            key.clear(function);
            callable.clear(function);
        }
        self.emit_global_var_name_loop(
            names.variable_bindings,
            function,
            |builder, key, function| {
                let property_key = PropertyKeyLocals::from_string(schema, key, function);
                builder.emit_create_global_var_binding(
                    &object,
                    &property_key,
                    deletable,
                    &realm,
                    function,
                )?;
                property_key.clear(function);
                Ok(())
            },
        )?;
        value.clear(function);
        object.clear(function);
        environment.clear(function);
        realm.clear(function);
        Ok(())
    }

    fn emit_create_global_var_binding_inline(
        &mut self,
        object: &ValueLocals,
        key: &PropertyKeyLocals,
        deletable: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let descriptor = self.emit_direct_own_descriptor_fact(object, key, function)?;
        descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        let value = schema.reserve_value_local(function);
        value.set_undefined(function);
        let writable = schema.reserve_i32_local(function);
        let configurable = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(1));
        writable.store(function);
        deletable.load(function);
        configurable.store(function);
        let pending = schema.reserve_completion(function);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectDefineDataArguments::new(
                    object,
                    key,
                    &value,
                    writable,
                    writable,
                    configurable,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        schema.release_i32_local(configurable, function);
        schema.release_i32_local(writable, function);
        value.clear(function);
        descriptor.set_null(schema, function);
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        Ok(())
    }

    fn emit_create_global_function_binding(
        &mut self,
        object: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        deletable: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let writable = schema.reserve_i32_local(function);
        let configurable = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(1));
        writable.store(function);
        function.instruction(&Instruction::I32Const(i32::from(deletable)));
        configurable.store(function);
        let descriptor = self.emit_direct_own_descriptor_fact(object, key, function)?;
        descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_global_descriptor_flag(&descriptor, DescriptorMask::CONFIGURABLE, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(0));
        configurable.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let pending = schema.reserve_completion(function);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectDefineDataArguments::new(
                    object,
                    key,
                    value,
                    writable,
                    writable,
                    configurable,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        descriptor.clear(function);
        schema.release_i32_local(configurable, function);
        schema.release_i32_local(writable, function);
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }
}
