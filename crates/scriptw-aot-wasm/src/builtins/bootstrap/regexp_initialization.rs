//! The defining Realm owns RegExp's graph, built before its first observation.
//! Installation performs no JavaScript, pattern compilation or matching operation.

use super::*;
use crate::heap::RegExpInitializationState;
use crate::runtime_helpers::{HelperParameters, RealmEnsureRegExpParameters};

/// Only this builder can read completed RegExp slots while the graph is Building.
pub(crate) struct RegExpIntrinsicBuild<'a>(&'a GcLocal<RealmRecord>);

impl RegExpIntrinsicBuild<'_> {
    pub(crate) fn realm(&self) -> &GcLocal<RealmRecord> {
        self.0
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_realm_ensure_regexp_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RealmEnsureRegExp);
        let parameters = self.helper_parameters::<RealmEnsureRegExpParameters>(&mut function);
        let schema = self.runtime_schema();
        let state =
            GcI32DomainLocal::new(schema, RegExpInitializationState::Dormant, &mut function);
        schema
            .field(RealmRecordSchema::REGEXP_STATE)
            .read(&parameters.realm, schema, &mut function)
            .store_domain(&state, &mut function);
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            RegExpInitializationState::Ready.word() as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::Return);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // Reentry must never expose or retry an incomplete canonical graph.
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            RegExpInitializationState::Dormant.word() as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.field(RealmRecordSchema::REGEXP_STATE).write(
            &parameters.realm,
            GcOperand::constant(RegExpInitializationState::Building),
            schema,
            &mut function,
        );
        self.emit_build_regexp_family(&RegExpIntrinsicBuild(&parameters.realm), &mut function)?;
        schema.field(RealmRecordSchema::REGEXP_STATE).write(
            &parameters.realm,
            GcOperand::constant(RegExpInitializationState::Ready),
            schema,
            &mut function,
        );
        state.clear(schema, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_build_regexp_family(
        &mut self,
        building: &RegExpIntrinsicBuild<'_>,
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
        // Root the ordinary prototype before constructing the constructor cycle.
        self.emit_bootstrap_plain_prototype(
            &context,
            NonArrayRealmIntrinsicSlot::RegExpPrototype,
            &parent,
            function,
        )?;
        let prototype = schema.reserve_value_local(function);
        self.emit_load_building_regexp_intrinsic(
            building,
            NonArrayRealmIntrinsicSlot::RegExpPrototype,
            &prototype,
            function,
        );
        let constructor = self.emit_builtin_constructor_shell_with_parent(
            StandardBuiltinId::RegExpConstructor,
            &context,
            &prototype,
            None,
            function,
        )?;
        let constructor_value = schema.reserve_value_local(function);
        constructor_value.set_reference(&constructor, schema, function);
        self.install_regexp_constructor_intrinsics(
            &IntrinsicInstall {
                builtin: StandardBuiltinId::RegExpConstructor,
                realm: &context,
                constructor: &constructor_value,
                prototype: &prototype,
            },
            building,
            function,
        )?;
        constructor_value.clear(function);
        constructor.clear(function);
        prototype.clear(function);
        parent.clear(function);
        self.release_realm_function_materialization_context(context, function);
        Ok(())
    }

    pub(super) fn emit_install_deferred_regexp_constructor(
        &mut self,
        target: &ValueLocals,
        name: &str,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let builtin = StandardBuiltinId::RegExpConstructor;
        let meta = self.functions.get(&builtin.function_id()).ok_or_else(|| {
            EmitError::unsupported("deferred RegExp constructor lacks planned native entry")
        })?;
        if meta.standard_builtin != Some(builtin)
            || !builtin.constructable()
            || standard_builtin_constructor_realm_slot(builtin)
                .map(NonArrayRealmIntrinsicSlot::gc_index)
                != Some(NonArrayRealmIntrinsicSlot::RegExpConstructor.gc_index())
        {
            return Err(EmitError::unsupported(
                "invalid deferred RegExp constructor identity",
            ));
        }
        let schema = self.runtime_schema();
        let recipe = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<DeferredRegExpConstructor>()
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
