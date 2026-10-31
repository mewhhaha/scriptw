//! The defining Realm owns Date's graph, built before its first observation.
//! Installation performs no JavaScript, clock or time-zone operation.

use super::*;
use crate::heap::DateInitializationState;
use crate::runtime_helpers::{HelperParameters, RealmEnsureDateParameters};

/// Only this builder can read completed Date slots while the graph is Building.
pub(crate) struct DateIntrinsicBuild<'a>(&'a GcLocal<RealmRecord>);

impl DateIntrinsicBuild<'_> {
    pub(crate) fn realm(&self) -> &GcLocal<RealmRecord> {
        self.0
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_realm_ensure_date_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RealmEnsureDate);
        let parameters = self.helper_parameters::<RealmEnsureDateParameters>(&mut function);
        let schema = self.runtime_schema();
        let state = GcI32DomainLocal::new(schema, DateInitializationState::Dormant, &mut function);
        schema
            .field(RealmRecordSchema::DATE_STATE)
            .read(&parameters.realm, schema, &mut function)
            .store_domain(&state, &mut function);
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            DateInitializationState::Ready.word() as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::Return);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // Reentry must never expose or retry an incomplete canonical graph.
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            DateInitializationState::Dormant.word() as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.field(RealmRecordSchema::DATE_STATE).write(
            &parameters.realm,
            GcOperand::constant(DateInitializationState::Building),
            schema,
            &mut function,
        );
        self.emit_build_date_family(&DateIntrinsicBuild(&parameters.realm), &mut function)?;
        schema.field(RealmRecordSchema::DATE_STATE).write(
            &parameters.realm,
            GcOperand::constant(DateInitializationState::Ready),
            schema,
            &mut function,
        );
        state.clear(schema, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_build_date_family(
        &mut self,
        building: &DateIntrinsicBuild<'_>,
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
        // Date.prototype is the same ordinary object as eager bootstrap used.
        // Its canonical slot roots it before constructor/context allocation.
        self.emit_bootstrap_plain_prototype(
            &context,
            NonArrayRealmIntrinsicSlot::DatePrototype,
            &parent,
            function,
        )?;
        let prototype = schema.reserve_value_local(function);
        self.emit_load_building_date_intrinsic(
            building,
            NonArrayRealmIntrinsicSlot::DatePrototype,
            &prototype,
            function,
        );
        // This existing producer closes and roots the constructor/prototype
        // cycle before installing the original methods and UTC/GMT aliases.
        let constructor = self.init_builtin_constructor_object_with_parent(
            StandardBuiltinId::DateConstructor,
            &context,
            &prototype,
            None,
            function,
        )?;
        constructor.clear(function);
        prototype.clear(function);
        parent.clear(function);
        self.release_realm_function_materialization_context(context, function);
        Ok(())
    }

    pub(super) fn emit_install_deferred_date_constructor(
        &mut self,
        target: &ValueLocals,
        name: &str,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let builtin = StandardBuiltinId::DateConstructor;
        let meta = self.functions.get(&builtin.function_id()).ok_or_else(|| {
            EmitError::unsupported("deferred Date constructor lacks planned native entry")
        })?;
        if meta.standard_builtin != Some(builtin)
            || !builtin.constructable()
            || standard_builtin_constructor_realm_slot(builtin)
                .map(NonArrayRealmIntrinsicSlot::gc_index)
                != Some(NonArrayRealmIntrinsicSlot::DateConstructor.gc_index())
        {
            return Err(EmitError::unsupported(
                "invalid deferred Date constructor identity",
            ));
        }
        let schema = self.runtime_schema();
        let recipe = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<DeferredDateConstructor>()
                .construct((GcOperand::reference(realm.realm(), schema),), function),
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
