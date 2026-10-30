//! Intrinsic identities are rooted values from the supplied defining Realm.

use super::*;
use crate::builtins::{
    BinaryDataIntrinsicBuild, ErrorIntrinsicBuild, IntlIntrinsicBuild, TemporalIntrinsicBuild,
};
use crate::gc_types::{
    ArrayObject, FunctionContext, FunctionContextSchema, GcLocal, GcStackReference, IntrinsicTable,
    RealmRecord, RealmRecordSchema, StoredValue, ValueLocals,
};

impl FunctionBuilder<'_> {
    pub(super) fn emit_store_realm_intrinsic_at_index(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        index: u32,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let intrinsics = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<RealmRecord>()
                .field(RealmRecordSchema::INTRINSICS)
                .read(realm, schema, function)
                .reference(),
            function,
        );
        let slot = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(index as i32));
        slot.store(function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(value, function),
            function,
        );
        schema.array_type::<IntrinsicTable>().write(
            &intrinsics,
            slot,
            crate::gc_types::GcOperand::reference(&stored, schema),
            schema,
            function,
        );
        stored.clear(function);
        schema.release_i32_local(slot, function);
        intrinsics.clear(function);
    }

    pub(crate) fn emit_current_function_realm(
        &mut self,
        function: &mut Function,
    ) -> GcStackReference<RealmRecord> {
        let schema = self.runtime_schema();
        let context = self
            .current_function_context()
            .expect("current-function Realm requires an actual callable context");
        schema
            .struct_type::<FunctionContext>()
            .field(FunctionContextSchema::REALM)
            .read(context, schema, function)
            .reference()
    }

    pub(crate) fn emit_load_non_array_realm_intrinsic(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        slot: NonArrayRealmIntrinsicSlot,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        let stored = self.emit_load_non_array_realm_intrinsic_stored(realm, slot, function);
        self.emit_read_completed_intrinsic(&stored, result, function);
        stored.clear(function);
    }

    /// Property recipes return this existing immutable record directly. All
    /// ordinary canonical reads share the same defining-Realm demand gate.
    pub(crate) fn emit_load_non_array_realm_intrinsic_stored(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        slot: NonArrayRealmIntrinsicSlot,
        function: &mut Function,
    ) -> GcLocal<StoredValue> {
        if slot.is_error() {
            let schema = self.runtime_schema();
            schema.call_helper(
                crate::runtime_helpers::RealmEnsureErrorsArguments::new(realm),
                self.runtime_helper_base()
                    .expect("Error intrinsic reads require registered runtime helpers"),
                function,
            );
        }
        if slot.is_binary_data() {
            let schema = self.runtime_schema();
            schema.call_helper(
                crate::runtime_helpers::RealmEnsureBinaryDataArguments::new(realm),
                self.runtime_helper_base()
                    .expect("binary-data intrinsic reads require registered runtime helpers"),
                function,
            );
        }
        if slot.is_temporal() {
            let schema = self.runtime_schema();
            let namespace = schema
                .call_helper(
                    crate::runtime_helpers::RealmEnsureTemporalArguments::new(realm),
                    self.runtime_helper_base()
                        .expect("Temporal intrinsic reads require the registered runtime helpers"),
                    function,
                )
                .bind(schema, schema.reserve_gc_local(function), function);
            namespace.clear(function);
        }
        if slot.is_intl() {
            let schema = self.runtime_schema();
            let namespace = schema
                .call_helper(
                    crate::runtime_helpers::RealmEnsureIntlArguments::new(realm),
                    self.runtime_helper_base()
                        .expect("Intl intrinsic reads require the registered runtime helpers"),
                    function,
                )
                .bind(schema, schema.reserve_gc_local(function), function);
            namespace.clear(function);
        }
        self.emit_load_realm_intrinsic_stored_at_index(realm, slot.gc_index(), function)
    }

    /// Only the Error builder may read completed Error slots while Building.
    pub(crate) fn emit_load_building_error_intrinsic(
        &mut self,
        building: &ErrorIntrinsicBuild<'_>,
        slot: NonArrayRealmIntrinsicSlot,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        assert!(
            slot.is_error(),
            "Error build capability used for another family"
        );
        self.emit_load_realm_intrinsic_at_index(
            building.realm(),
            slot.gc_index(),
            result,
            function,
        );
    }

    /// Only the binary-data builder may read its completed slots while Building.
    pub(crate) fn emit_load_building_binary_data_intrinsic(
        &mut self,
        building: &BinaryDataIntrinsicBuild<'_>,
        slot: NonArrayRealmIntrinsicSlot,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        assert!(
            slot.is_binary_data(),
            "binary-data build capability used for another cluster"
        );
        self.emit_load_realm_intrinsic_at_index(
            building.realm(),
            slot.gc_index(),
            result,
            function,
        );
    }

    /// Only the Temporal cluster builder can obtain this capability. Ordinary
    /// reads always cross the ensure boundary, including during Building.
    pub(crate) fn emit_load_building_temporal_intrinsic(
        &mut self,
        building: &TemporalIntrinsicBuild<'_>,
        slot: NonArrayRealmIntrinsicSlot,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        assert!(
            slot.is_temporal(),
            "Temporal build capability used for another cluster"
        );
        self.emit_load_realm_intrinsic_at_index(
            building.realm(),
            slot.gc_index(),
            result,
            function,
        );
    }

    /// Only the Intl builder can read its completed slots during Building.
    pub(crate) fn emit_load_building_intl_intrinsic(
        &mut self,
        building: &IntlIntrinsicBuild<'_>,
        slot: NonArrayRealmIntrinsicSlot,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        assert!(
            slot.is_intl(),
            "Intl build capability used for another cluster"
        );
        self.emit_load_realm_intrinsic_at_index(
            building.realm(),
            slot.gc_index(),
            result,
            function,
        );
    }

    fn emit_load_realm_intrinsic_at_index(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        index: u32,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        let stored = self.emit_load_realm_intrinsic_stored_at_index(realm, index, function);
        self.emit_read_completed_intrinsic(&stored, result, function);
        stored.clear(function);
    }

    fn emit_load_realm_intrinsic_stored_at_index(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        index: u32,
        function: &mut Function,
    ) -> GcLocal<StoredValue> {
        let schema = self.runtime_schema();
        let intrinsics = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<RealmRecord>()
                .field(RealmRecordSchema::INTRINSICS)
                .read(realm, schema, function)
                .reference(),
            function,
        );
        let slot = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(index as i32));
        slot.store(function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<IntrinsicTable>()
                .read(&intrinsics, slot, schema, function)
                .reference(),
            function,
        );
        schema.release_i32_local(slot, function);
        intrinsics.clear(function);
        stored
    }

    fn emit_read_completed_intrinsic(
        &self,
        stored: &GcLocal<StoredValue>,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .struct_type::<StoredValue>()
            .read_into(stored, result, schema, function);
        // The table contains only completed intrinsic objects. An unpopulated
        // bootstrap slot is a compiler invariant failure at this boundary.
        function.instruction(&Instruction::I32Const(0));
        for tag in [
            WasmRuntimeValueTag::Object,
            WasmRuntimeValueTag::Array,
            WasmRuntimeValueTag::Function,
            WasmRuntimeValueTag::Arguments,
        ] {
            result.tag().load(function);
            function.instruction(&Instruction::I32Const(tag as i32));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32Or);
        }
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_load_realm_array_prototype(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> GcStackReference<ArrayObject> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        self.emit_load_realm_intrinsic_at_index(
            realm,
            NonArrayRealmIntrinsicSlot::ARRAY_INDEX,
            &value,
            function,
        );
        let prototype = value.cast_reference::<ArrayObject>(schema, function);
        value.clear(function);
        prototype
    }
}
