//! One defining-Realm owner constructs the complete canonical Intl graph.
//! Intrinsic allocation and installation call no JavaScript or provider code.

use super::*;
use crate::heap::IntlInitializationState;
use crate::runtime_helpers::{HelperParameters, RealmEnsureIntlParameters};

/// Only this module can authorize reads of already completed Intl slots while
/// the Realm is Building. Ordinary consumers always cross the ensure boundary.
pub(crate) struct IntlIntrinsicBuild<'a>(&'a GcLocal<RealmRecord>);

impl IntlIntrinsicBuild<'_> {
    pub(crate) fn realm(&self) -> &GcLocal<RealmRecord> {
        self.0
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_realm_ensure_intl_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RealmEnsureIntl);
        let parameters = self.helper_parameters::<RealmEnsureIntlParameters>(&mut function);
        let schema = self.runtime_schema();
        let state = GcI32DomainLocal::new(schema, IntlInitializationState::Dormant, &mut function);
        schema
            .field(RealmRecordSchema::INTL_STATE)
            .read(&parameters.realm, schema, &mut function)
            .store_domain(&state, &mut function);
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            IntlInitializationState::Ready.word() as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, &mut function);
        schema
            .field(RealmRecordSchema::INTL_NAMESPACE)
            .read(&parameters.realm, schema, &mut function)
            .reference()
            .require_non_null(&mut function);
        function.instruction(&Instruction::Return);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // Building reentry is a compiler invariant failure. A trap must not
        // reset or retry a partially published graph in the failed Store.
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            IntlInitializationState::Dormant.word() as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.field(RealmRecordSchema::INTL_STATE).write(
            &parameters.realm,
            GcOperand::constant(IntlInitializationState::Building),
            schema,
            &mut function,
        );
        let building = IntlIntrinsicBuild(&parameters.realm);
        let namespace = self.emit_build_intl_cluster(&building, &mut function)?;
        let stored = schema.reserve_gc_local(&mut function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&namespace, &mut function),
            &mut function,
        );
        // The private strong root survives public deletion/replacement. Publish
        // Ready only after the namespace and every canonical slot are complete.
        schema.field(RealmRecordSchema::INTL_NAMESPACE).write(
            &parameters.realm,
            GcOperand::nullable_reference(&stored, schema),
            schema,
            &mut function,
        );
        schema.field(RealmRecordSchema::INTL_STATE).write(
            &parameters.realm,
            GcOperand::constant(IntlInitializationState::Ready),
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

    fn emit_build_intl_cluster(
        &mut self,
        building: &IntlIntrinsicBuild<'_>,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let schema = self.runtime_schema();
        let realm = building.realm();
        let context = self.emit_realm_function_materialization_context_from_realm(realm, function);
        let parent = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::IteratorPrototype,
            &parent,
            function,
        );
        // Keep prototype allocation order separate from constructor and public
        // namespace insertion order. Only SegmentIterator inherits Iterator.
        self.emit_bootstrap_plain_prototype(
            &context,
            NonArrayRealmIntrinsicSlot::IntlSegmentIteratorPrototype,
            &parent,
            function,
        )?;
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &parent,
            function,
        );
        for slot in [
            NonArrayRealmIntrinsicSlot::IntlLocalePrototype,
            NonArrayRealmIntrinsicSlot::IntlDateTimeFormatPrototype,
            NonArrayRealmIntrinsicSlot::IntlNumberFormatPrototype,
            NonArrayRealmIntrinsicSlot::IntlPluralRulesPrototype,
            NonArrayRealmIntrinsicSlot::IntlListFormatPrototype,
            NonArrayRealmIntrinsicSlot::IntlCollatorPrototype,
            NonArrayRealmIntrinsicSlot::IntlDisplayNamesPrototype,
            NonArrayRealmIntrinsicSlot::IntlRelativeTimeFormatPrototype,
            NonArrayRealmIntrinsicSlot::IntlSegmenterPrototype,
            NonArrayRealmIntrinsicSlot::IntlDurationFormatPrototype,
            NonArrayRealmIntrinsicSlot::IntlSegmentsPrototype,
        ] {
            self.emit_bootstrap_plain_prototype(&context, slot, &parent, function)?;
        }
        for (builtin, slot) in [
            (
                StandardBuiltinId::IntlLocaleConstructor,
                NonArrayRealmIntrinsicSlot::IntlLocalePrototype,
            ),
            (
                StandardBuiltinId::IntlDateTimeFormatConstructor,
                NonArrayRealmIntrinsicSlot::IntlDateTimeFormatPrototype,
            ),
            (
                StandardBuiltinId::IntlNumberFormatConstructor,
                NonArrayRealmIntrinsicSlot::IntlNumberFormatPrototype,
            ),
            (
                StandardBuiltinId::IntlPluralRulesConstructor,
                NonArrayRealmIntrinsicSlot::IntlPluralRulesPrototype,
            ),
            (
                StandardBuiltinId::IntlListFormatConstructor,
                NonArrayRealmIntrinsicSlot::IntlListFormatPrototype,
            ),
            (
                StandardBuiltinId::IntlCollatorConstructor,
                NonArrayRealmIntrinsicSlot::IntlCollatorPrototype,
            ),
            (
                StandardBuiltinId::IntlDisplayNamesConstructor,
                NonArrayRealmIntrinsicSlot::IntlDisplayNamesPrototype,
            ),
            (
                StandardBuiltinId::IntlRelativeTimeFormatConstructor,
                NonArrayRealmIntrinsicSlot::IntlRelativeTimeFormatPrototype,
            ),
            (
                StandardBuiltinId::IntlSegmenterConstructor,
                NonArrayRealmIntrinsicSlot::IntlSegmenterPrototype,
            ),
            (
                StandardBuiltinId::IntlDurationFormatConstructor,
                NonArrayRealmIntrinsicSlot::IntlDurationFormatPrototype,
            ),
        ] {
            let prototype = schema.reserve_value_local(function);
            self.emit_load_building_intl_intrinsic(building, slot, &prototype, function);
            let constructor =
                self.init_builtin_constructor_object(builtin, &context, &prototype, function)?;
            constructor.clear(function);
            prototype.clear(function);
            if builtin == StandardBuiltinId::IntlSegmenterConstructor {
                let segments = schema.reserve_value_local(function);
                let iterator = schema.reserve_value_local(function);
                self.emit_load_building_intl_intrinsic(
                    building,
                    NonArrayRealmIntrinsicSlot::IntlSegmentsPrototype,
                    &segments,
                    function,
                );
                self.emit_load_building_intl_intrinsic(
                    building,
                    NonArrayRealmIntrinsicSlot::IntlSegmentIteratorPrototype,
                    &iterator,
                    function,
                );
                self.install_intl_segmenter_auxiliary_intrinsics(
                    &context, &segments, &iterator, function,
                )?;
                iterator.clear(function);
                segments.clear(function);
            }
        }
        let members = self
            .runtime_bootstrap_plan
            .intl_namespace_members()
            .ok_or_else(|| EmitError::unsupported("Intl ensure requires the complete cluster"))?;
        let namespace =
            self.init_building_intl_object(building, members, &context, &parent, function)?;
        parent.clear(function);
        self.release_realm_function_materialization_context(context, function);
        Ok(namespace)
    }

    fn init_building_intl_object(
        &mut self,
        building: &IntlIntrinsicBuild<'_>,
        members: IntlNamespaceMembers,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let object = self.emit_bootstrap_namespace(object_prototype, function)?;
        for (name, builtin) in members.methods_in_installation_order() {
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
        let value = self.runtime_schema().reserve_value_local(function);
        for (name, builtin) in members.in_installation_order() {
            let slot = standard_builtin_constructor_realm_slot(builtin)
                .ok_or_else(|| EmitError::unsupported("Intl constructor has no intrinsic slot"))?;
            self.emit_load_building_intl_intrinsic(building, slot, &value, function);
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
            "Intl",
            false,
            false,
            true,
            function,
        )?;
        Ok(object)
    }
}
