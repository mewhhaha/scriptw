//! The complete Temporal graph has one defining-Realm initialization owner.
//! Construction executes only intrinsic allocation and direct installation;
//! no user callback or public global lookup can observe the Building state.

use super::*;
use crate::heap::TemporalInitializationState;
use crate::intrinsics::temporal::TemporalIntrinsicFamily;
use crate::runtime_helpers::{HelperParameters, RealmEnsureTemporalParameters};

/// A private construction capability, bound to the Realm being initialized.
/// Only this module can create it, after the Dormant -> Building transition.
/// It grants reads of already completed Temporal slots during construction.
pub(crate) struct TemporalIntrinsicBuild<'a>(&'a GcLocal<RealmRecord>);

impl TemporalIntrinsicBuild<'_> {
    pub(crate) fn realm(&self) -> &GcLocal<RealmRecord> {
        self.0
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_realm_ensure_temporal_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RealmEnsureTemporal);
        let parameters = self.helper_parameters::<RealmEnsureTemporalParameters>(&mut function);
        let schema = self.runtime_schema();
        let state =
            GcI32DomainLocal::new(schema, TemporalInitializationState::Dormant, &mut function);
        schema
            .field(RealmRecordSchema::TEMPORAL_STATE)
            .read(&parameters.realm, schema, &mut function)
            .store_domain(&state, &mut function);
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            TemporalInitializationState::Ready.word() as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, &mut function);
        schema
            .field(RealmRecordSchema::TEMPORAL_NAMESPACE)
            .read(&parameters.realm, schema, &mut function)
            .reference()
            .require_non_null(&mut function);
        function.instruction(&Instruction::Return);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // Building reentry is impossible without a compiler invariant failure.
        // A trap leaves the Store failed: never reset or retry a partial graph.
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            TemporalInitializationState::Dormant.word() as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.field(RealmRecordSchema::TEMPORAL_STATE).write(
            &parameters.realm,
            GcOperand::constant(TemporalInitializationState::Building),
            schema,
            &mut function,
        );
        let building = TemporalIntrinsicBuild(&parameters.realm);
        let namespace = self.emit_build_temporal_cluster(&building, &mut function)?;
        let stored = schema.reserve_gc_local(&mut function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&namespace, &mut function),
            &mut function,
        );
        // Keep the namespace and every intrinsic slot rooted before publishing
        // Ready. The canonical value survives deletion of the public property.
        schema.field(RealmRecordSchema::TEMPORAL_NAMESPACE).write(
            &parameters.realm,
            GcOperand::nullable_reference(&stored, schema),
            schema,
            &mut function,
        );
        schema.field(RealmRecordSchema::TEMPORAL_STATE).write(
            &parameters.realm,
            GcOperand::constant(TemporalInitializationState::Ready),
            schema,
            &mut function,
        );
        stored.load(schema, &mut function);
        stored.clear(&mut function);
        namespace.clear(&mut function);
        state.clear(schema, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_build_temporal_cluster(
        &mut self,
        building: &TemporalIntrinsicBuild<'_>,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let schema = self.runtime_schema();
        let realm = building.realm();
        let object_prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &object_prototype,
            function,
        );
        let context = self.emit_realm_function_materialization_context_from_realm(realm, function);
        // Preserve the independent historical prototype and constructor orders.
        // Publish all eight prototypes before installing any constructor/member.
        for family in TemporalIntrinsicFamily::ALL {
            self.emit_bootstrap_plain_prototype(
                &context,
                family.prototype_slot(),
                &object_prototype,
                function,
            )?;
        }
        for family in [
            TemporalIntrinsicFamily::Instant,
            TemporalIntrinsicFamily::PlainDate,
            TemporalIntrinsicFamily::Duration,
            TemporalIntrinsicFamily::PlainTime,
            TemporalIntrinsicFamily::PlainDateTime,
            TemporalIntrinsicFamily::PlainYearMonth,
            TemporalIntrinsicFamily::PlainMonthDay,
            TemporalIntrinsicFamily::ZonedDateTime,
        ] {
            let prototype = schema.reserve_value_local(function);
            self.emit_load_building_temporal_intrinsic(
                building,
                family.prototype_slot(),
                &prototype,
                function,
            );
            let constructor = self.init_builtin_constructor_object(
                family.constructor(),
                &context,
                &prototype,
                function,
            )?;
            constructor.clear(function);
            prototype.clear(function);
        }
        let members = self
            .runtime_bootstrap_plan
            .temporal_namespace_members()
            .ok_or_else(|| {
                EmitError::unsupported("Temporal ensure requires the complete cluster")
            })?;
        let namespace = self.init_building_temporal_object(
            building,
            members,
            &context,
            &object_prototype,
            function,
        )?;
        self.release_realm_function_materialization_context(context, function);
        object_prototype.clear(function);
        Ok(namespace)
    }

    fn init_temporal_now_object(
        &mut self,
        members: TemporalNamespaceMembers,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let object = self.emit_bootstrap_namespace(object_prototype, function)?;
        self.emit_install_intrinsic_string(
            &object,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            "Temporal.Now",
            false,
            false,
            true,
            function,
        )?;
        for (name, builtin) in members.now_members_in_installation_order() {
            self.emit_install_intrinsic_method(
                &object,
                IntrinsicKey::Name(name),
                builtin,
                realm,
                true,
                true,
                function,
            )?;
        }
        Ok(object)
    }

    fn init_building_temporal_object(
        &mut self,
        building: &TemporalIntrinsicBuild<'_>,
        members: TemporalNamespaceMembers,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let object = self.emit_bootstrap_namespace(object_prototype, function)?;
        let now = self.init_temporal_now_object(members, realm, object_prototype, function)?;
        self.emit_install_intrinsic_data(
            &object,
            IntrinsicKey::Name(TEMPORAL_NOW_NAME),
            &now,
            true,
            false,
            true,
            function,
        )?;
        now.clear(function);
        let value = self.runtime_schema().reserve_value_local(function);
        for (name, builtin) in members.constructors_in_installation_order() {
            let slot = standard_builtin_constructor_realm_slot(builtin).ok_or_else(|| {
                EmitError::unsupported("Temporal constructor has no intrinsic slot")
            })?;
            self.emit_load_building_temporal_intrinsic(building, slot, &value, function);
            self.emit_install_intrinsic_data(
                &object,
                IntrinsicKey::Name(name),
                &value,
                true,
                false,
                true,
                function,
            )?;
        }
        value.clear(function);
        self.emit_install_intrinsic_string(
            &object,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            "Temporal",
            false,
            false,
            true,
            function,
        )?;
        Ok(object)
    }
}
