//! One defining-Realm owner constructs the complete binary-data graph.
//! Allocation and intrinsic installation call no JavaScript or host code.

use super::*;
use crate::heap::{BinaryDataConstructor, BinaryDataInitializationState};
use crate::runtime_helpers::{HelperParameters, RealmEnsureBinaryDataParameters};

/// Only this module can authorize reads of completed binary-data slots while
/// Building. Ordinary consumers always cross the defining-Realm ensure boundary.
pub(crate) struct BinaryDataIntrinsicBuild<'a>(&'a GcLocal<RealmRecord>);

impl BinaryDataIntrinsicBuild<'_> {
    pub(crate) fn realm(&self) -> &GcLocal<RealmRecord> {
        self.0
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_realm_ensure_binary_data_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RealmEnsureBinaryData);
        let parameters = self.helper_parameters::<RealmEnsureBinaryDataParameters>(&mut function);
        let schema = self.runtime_schema();
        let state = GcI32DomainLocal::new(
            schema,
            BinaryDataInitializationState::Dormant,
            &mut function,
        );
        schema
            .field(RealmRecordSchema::BINARY_DATA_STATE)
            .read(&parameters.realm, schema, &mut function)
            .store_domain(&state, &mut function);
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            BinaryDataInitializationState::Ready.word() as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::Return);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // Reentry is a compiler invariant failure; never retry a partial graph.
        state.load(&mut function);
        function.instruction(&Instruction::I32Const(
            BinaryDataInitializationState::Dormant.word() as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.field(RealmRecordSchema::BINARY_DATA_STATE).write(
            &parameters.realm,
            GcOperand::constant(BinaryDataInitializationState::Building),
            schema,
            &mut function,
        );
        self.emit_build_binary_data_cluster(
            &BinaryDataIntrinsicBuild(&parameters.realm),
            &mut function,
        )?;
        // The 32 canonical slots are the strong roots, even when every public
        // global was deleted or replaced before private first use.
        schema.field(RealmRecordSchema::BINARY_DATA_STATE).write(
            &parameters.realm,
            GcOperand::constant(BinaryDataInitializationState::Ready),
            schema,
            &mut function,
        );
        state.clear(schema, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_build_binary_data_cluster(
        &mut self,
        building: &BinaryDataIntrinsicBuild<'_>,
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
        // Keep prototype allocation order separate from constructor and public
        // global property order; concrete prototypes inherit %TypedArray%.
        for slot in [
            NonArrayRealmIntrinsicSlot::ArrayBufferPrototype,
            NonArrayRealmIntrinsicSlot::SharedArrayBufferPrototype,
            NonArrayRealmIntrinsicSlot::DataViewPrototype,
            NonArrayRealmIntrinsicSlot::TypedArrayPrototype,
        ] {
            self.emit_bootstrap_plain_prototype(&context, slot, &parent, function)?;
        }
        for (builtin, slot) in [
            (
                StandardBuiltinId::ArrayBufferConstructor,
                NonArrayRealmIntrinsicSlot::ArrayBufferPrototype,
            ),
            (
                StandardBuiltinId::SharedArrayBufferConstructor,
                NonArrayRealmIntrinsicSlot::SharedArrayBufferPrototype,
            ),
            (
                StandardBuiltinId::DataViewConstructor,
                NonArrayRealmIntrinsicSlot::DataViewPrototype,
            ),
        ] {
            let prototype = schema.reserve_value_local(function);
            self.emit_load_building_binary_data_intrinsic(building, slot, &prototype, function);
            let constructor = self.init_builtin_constructor_object_with_parent(
                builtin, &context, &prototype, None, function,
            )?;
            constructor.clear(function);
            prototype.clear(function);
        }
        self.init_building_typed_array_intrinsic(building, &context, function)?;
        self.emit_load_building_binary_data_intrinsic(
            building,
            NonArrayRealmIntrinsicSlot::TypedArrayPrototype,
            &parent,
            function,
        );
        let constructor_parent = schema.reserve_value_local(function);
        self.emit_load_building_binary_data_intrinsic(
            building,
            NonArrayRealmIntrinsicSlot::TypedArrayConstructor,
            &constructor_parent,
            function,
        );
        for kind in TypedArrayElementKind::ALL {
            let slot = NonArrayRealmIntrinsicSlot::prototype_identity(kind);
            self.emit_bootstrap_plain_prototype(&context, slot, &parent, function)?;
            let prototype = schema.reserve_value_local(function);
            self.emit_load_building_binary_data_intrinsic(building, slot, &prototype, function);
            let constructor = self.init_builtin_constructor_object_with_parent(
                kind.constructor(),
                &context,
                &prototype,
                Some(&constructor_parent),
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

    pub(super) fn emit_install_deferred_binary_data_constructor(
        &mut self,
        target: &ValueLocals,
        name: &str,
        constructor: BinaryDataConstructor,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let builtin = constructor.builtin();
        let meta = self.functions.get(&builtin.function_id()).ok_or_else(|| {
            EmitError::unsupported("deferred binary-data constructor lacks planned native entry")
        })?;
        if meta.standard_builtin != Some(builtin)
            || !builtin.constructable()
            || standard_builtin_constructor_realm_slot(builtin)
                .map(NonArrayRealmIntrinsicSlot::gc_index)
                != Some(constructor.slot().gc_index())
        {
            return Err(EmitError::unsupported(
                "invalid deferred binary-data constructor identity",
            ));
        }
        let schema = self.runtime_schema();
        let recipe = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<DeferredBinaryDataConstructor>()
                .construct(
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

    fn init_building_typed_array_intrinsic(
        &mut self,
        building: &BinaryDataIntrinsicBuild<'_>,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        self.emit_load_building_binary_data_intrinsic(
            building,
            NonArrayRealmIntrinsicSlot::TypedArrayPrototype,
            &prototype,
            function,
        );
        let constructor = self.init_builtin_constructor_object(
            StandardBuiltinId::TypedArrayConstructor,
            realm,
            &prototype,
            function,
        )?;
        let constructor_value = schema.reserve_value_local(function);
        constructor_value.set_reference(&constructor, schema, function);
        self.emit_install_intrinsic_accessor(
            &constructor_value,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::Species),
            AccessorDescriptor::Getter(AccessorGetter::new(
                StandardBuiltinId::TypedArraySpeciesGetter,
            )),
            realm,
            true,
            function,
        )?;
        for (name, builtin) in [
            ("buffer", StandardBuiltinId::TypedArrayPrototypeBufferGetter),
            (
                "byteLength",
                StandardBuiltinId::TypedArrayPrototypeByteLengthGetter,
            ),
            (
                "byteOffset",
                StandardBuiltinId::TypedArrayPrototypeByteOffsetGetter,
            ),
            ("length", StandardBuiltinId::TypedArrayPrototypeLengthGetter),
        ] {
            self.emit_install_intrinsic_accessor(
                &prototype,
                IntrinsicKey::Name(name),
                AccessorDescriptor::Getter(AccessorGetter::new(builtin)),
                realm,
                true,
                function,
            )?;
        }
        self.emit_install_intrinsic_accessor(
            &prototype,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            AccessorDescriptor::Getter(AccessorGetter::new(
                StandardBuiltinId::TypedArrayPrototypeToStringTagGetter,
            )),
            realm,
            true,
            function,
        )?;
        self.emit_install_intrinsic_methods(
            &prototype,
            crate::intrinsics::IntrinsicMethodPlan::TypedArrayPrototypeBeforeAliases,
            realm,
            function,
        )?;
        self.emit_install_intrinsic_method_aliases(
            &prototype,
            &[
                IntrinsicKey::Name("values"),
                IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::Iterator),
            ],
            StandardBuiltinId::TypedArrayPrototypeValues,
            realm,
            true,
            true,
            function,
        )?;
        self.emit_install_intrinsic_methods(
            &prototype,
            crate::intrinsics::IntrinsicMethodPlan::TypedArrayPrototypeAfterAliases,
            realm,
            function,
        )?;
        // Array.prototype owns this eager callable. Read the canonical slot
        // directly instead of emitting another allocation fallback in this graph.
        let to_string = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm.realm(),
            NonArrayRealmIntrinsicSlot::TypedArrayPrototypeToString,
            &to_string,
            function,
        );
        self.emit_install_intrinsic_data(
            &prototype,
            IntrinsicKey::Name("toString"),
            &to_string,
            true,
            false,
            true,
            function,
        )?;
        to_string.clear(function);
        self.emit_install_intrinsic_methods(
            &prototype,
            crate::intrinsics::IntrinsicMethodPlan::TypedArrayPrototypeAfterToString,
            realm,
            function,
        )?;
        self.emit_install_intrinsic_methods(
            &constructor_value,
            crate::intrinsics::IntrinsicMethodPlan::TypedArrayConstructor,
            realm,
            function,
        )?;
        constructor_value.clear(function);
        constructor.clear(function);
        prototype.clear(function);
        Ok(())
    }
}
