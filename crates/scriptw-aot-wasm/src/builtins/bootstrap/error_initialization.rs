//! One defining-Realm owner constructs the complete Error graph.
//! Allocation and intrinsic installation call no JavaScript or host code.

use super::*;
use crate::heap::{ErrorInitializationState, ErrorIntrinsicConstructor};
use crate::runtime_helpers::{HelperParameters, RealmEnsureErrorsParameters};

/// Only this module can authorize reads of completed Error slots while
/// Building. Ordinary consumers always cross the defining-Realm ensure boundary.
pub(crate) struct ErrorIntrinsicBuild<'a>(&'a GcLocal<RealmRecord>);

impl ErrorIntrinsicBuild<'_> {
    pub(crate) fn realm(&self) -> &GcLocal<RealmRecord> {
        self.0
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_realm_ensure_errors_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RealmEnsureErrors);
        let parameters = self.helper_parameters::<RealmEnsureErrorsParameters>(&mut function);
        let schema = self.runtime_schema();
        let state = GcI32DomainLocal::new(schema, ErrorInitializationState::Dormant, &mut function);
        schema
            .field(RealmRecordSchema::ERROR_STATE)
            .read(&parameters.realm, schema, &mut function)
            .store_domain(&state, &mut function);
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            ErrorInitializationState::Ready.word() as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::Return);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // Reentry is a compiler invariant failure; never retry a partial graph.
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            ErrorInitializationState::Dormant.word() as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.field(RealmRecordSchema::ERROR_STATE).write(
            &parameters.realm,
            GcOperand::constant(ErrorInitializationState::Building),
            schema,
            &mut function,
        );
        self.emit_build_error_family(&ErrorIntrinsicBuild(&parameters.realm), &mut function)?;
        // The 18 canonical slots are the strong roots, even when every public
        // global was deleted or replaced before private first use.
        schema.field(RealmRecordSchema::ERROR_STATE).write(
            &parameters.realm,
            GcOperand::constant(ErrorInitializationState::Ready),
            schema,
            &mut function,
        );
        state.clear(schema, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_build_error_family(
        &mut self,
        building: &ErrorIntrinsicBuild<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = building.realm();
        let context = self.emit_realm_function_materialization_context_from_realm(realm, function);
        let parent = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &parent,
            function,
        );
        // Ordinary prototypes carry no [[ErrorData]]. Preserve their original
        // own-property order: name, message, constructor, then Error methods.
        for identity in ErrorIntrinsicConstructor::ALL {
            if identity != ErrorIntrinsicConstructor::Error {
                self.emit_load_building_error_intrinsic(
                    building,
                    NonArrayRealmIntrinsicSlot::ErrorPrototype,
                    &parent,
                    function,
                );
            }
            let prototype = self.emit_bootstrap_namespace(&parent, function)?;
            self.emit_install_intrinsic_string(
                &prototype,
                IntrinsicKey::Name("name"),
                identity.name(),
                true,
                false,
                true,
                function,
            )?;
            self.emit_install_intrinsic_string(
                &prototype,
                IntrinsicKey::Name("message"),
                "",
                true,
                false,
                true,
                function,
            )?;
            self.emit_store_non_array_realm_intrinsic(
                realm,
                identity.prototype_slot(),
                &prototype,
                function,
            );
            prototype.clear(function);
        }
        // The first constructor inherits Function.prototype; all derived
        // constructors inherit the completed private canonical Error constructor.
        let constructor_parent = schema.reserve_value_local(function);
        for identity in ErrorIntrinsicConstructor::ALL {
            let prototype = schema.reserve_value_local(function);
            self.emit_load_building_error_intrinsic(
                building,
                identity.prototype_slot(),
                &prototype,
                function,
            );
            let parent = if identity == ErrorIntrinsicConstructor::Error {
                None
            } else {
                self.emit_load_building_error_intrinsic(
                    building,
                    NonArrayRealmIntrinsicSlot::ErrorConstructor,
                    &constructor_parent,
                    function,
                );
                Some(&constructor_parent)
            };
            let constructor = self.init_builtin_constructor_object_with_parent(
                identity.builtin(),
                &context,
                &prototype,
                parent,
                function,
            )?;
            constructor.clear(function);
            prototype.clear(function);
        }
        constructor_parent.clear(function);
        parent.clear(function);
        self.release_realm_function_materialization_context(context, function);
        Ok(())
    }

    pub(super) fn emit_install_deferred_error_constructor(
        &mut self,
        target: &ValueLocals,
        name: &str,
        constructor: ErrorIntrinsicConstructor,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let builtin = constructor.builtin();
        let meta = self.functions.get(&builtin.function_id()).ok_or_else(|| {
            EmitError::unsupported("deferred Error constructor lacks planned native entry")
        })?;
        if meta.standard_builtin != Some(builtin)
            || !builtin.constructable()
            || standard_builtin_constructor_realm_slot(builtin)
                .map(NonArrayRealmIntrinsicSlot::gc_index)
                != Some(constructor.slot().gc_index())
        {
            return Err(EmitError::unsupported(
                "invalid deferred Error constructor identity",
            ));
        }
        let schema = self.runtime_schema();
        let recipe = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<DeferredErrorConstructor>().construct(
                (
                    GcOperand::reference(realm.realm(), schema),
                    GcOperand::constant(constructor),
                ),
                function,
            ),
            function,
        );
        let deferred = schema
            .reserve_gc_local(function)
            .initialize(recipe.load(schema, function).upcast(), function);
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(target, function),
            function,
        );
        let key = self.intrinsic_key(IntrinsicKey::Name(name), function)?;
        let writable = schema.reserve_i32_local(function);
        writable.set_constant(1, function);
        let enumerable = schema.reserve_i32_local(function);
        enumerable.set_constant(0, function);
        let configurable = schema.reserve_i32_local(function);
        configurable.set_constant(1, function);
        schema.call_helper(
            crate::runtime_helpers::IntrinsicDataPublishArguments::new(
                &header,
                &key,
                &deferred,
                writable,
                enumerable,
                configurable,
            ),
            self.runtime_helper_base()?,
            function,
        );
        schema.release_i32_local(configurable, function);
        schema.release_i32_local(enumerable, function);
        schema.release_i32_local(writable, function);
        key.clear(function);
        header.clear(function);
        deferred.clear(function);
        recipe.clear(function);
        Ok(())
    }
}
