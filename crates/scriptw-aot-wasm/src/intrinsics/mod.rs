//! Per-family realm bootstrap and property-descriptor installation.
//!
//! This is the `intrinsics/` boundary named by T02. It exists to break up
//! `builtins/bootstrap.rs::init_builtin_constructor_object`, formerly a single
//! ~4,760-line function and the worst merge point in the backend. Per-family
//! bodies live here, while the mandatory catalog `StandardBuiltinInstaller`
//! classification now routes its 36 productive roots without naming every
//! other builtin in a no-op tail.
//!
//! Each family consumes borrowed completed Realm/constructor/prototype values.
//! Fresh property installation retains strong GC edges and the same method
//! ordering and alias identity as the canonical native bootstrap.
//!
//! Property installation order is observable through `Object.keys`, so arms
//! must keep their internal ordering and the dispatch in `bootstrap.rs` must
//! keep calling them in the same order it always did.

use super::*;
use crate::objects::{AccessorDescriptor, AccessorDescriptorLocals};

mod abstract_module_source;
pub(crate) mod array;
pub(crate) mod binary_data;
pub(crate) mod collections;
pub(crate) mod date;
mod deferred_methods;
pub(crate) mod errors;
pub(crate) mod function;
pub(crate) mod intl;
pub(crate) mod intl_segmenter;
pub(crate) mod iterator;
pub(crate) mod numeric;
pub(crate) mod object;
pub(crate) mod promise;
pub(crate) mod proxy;
pub(crate) mod regexp;
pub(crate) mod resource_management;
pub(crate) mod shadow_realm;
pub(crate) mod string;
pub(crate) mod symbol;
pub(crate) mod temporal;

/// The completed Realm and public values borrowed by one family installer.
/// No installer can reconstruct a semantic reference from a numeric global.
pub(crate) struct IntrinsicInstall<'m> {
    pub(crate) builtin: StandardBuiltinId,
    pub(crate) realm: &'m crate::functions::RealmFunctionMaterializationContext,
    pub(crate) constructor: &'m crate::gc_types::ValueLocals,
    pub(crate) prototype: &'m crate::gc_types::ValueLocals,
}

/// Intrinsic property names have the same actual String/Symbol key authority
/// as observable property operations, without spelling a Symbol as a String.
#[derive(Clone, Copy)]
pub(crate) enum IntrinsicKey<'k> {
    Name(&'k str),
    Symbol(scriptw_ir::WellKnownSymbol),
}

impl FunctionBuilder<'_> {
    fn intrinsic_key(
        &mut self,
        key: IntrinsicKey<'_>,
        function: &mut Function,
    ) -> Result<crate::operations::PropertyKeyLocals, EmitError> {
        let schema = self.runtime_schema();
        match key {
            IntrinsicKey::Name(name) => {
                let string = schema.reserve_gc_local(function).initialize(
                    self.emit_interned_string_reference(name, function)?,
                    function,
                );
                let key =
                    crate::operations::PropertyKeyLocals::from_string(schema, &string, function);
                string.clear(function);
                Ok(key)
            }
            IntrinsicKey::Symbol(symbol) => {
                let symbol = schema.reserve_gc_local(function).initialize(
                    self.emit_well_known_symbol_reference(symbol, function)?,
                    function,
                );
                let key =
                    crate::operations::PropertyKeyLocals::from_symbol(schema, &symbol, function);
                symbol.clear(function);
                Ok(key)
            }
        }
    }

    pub(crate) fn emit_install_intrinsic_data(
        &mut self,
        target: &crate::gc_types::ValueLocals,
        key: IntrinsicKey<'_>,
        value: &crate::gc_types::ValueLocals,
        writable: bool,
        enumerable: bool,
        configurable: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(target, function),
            function,
        );
        let key = self.intrinsic_key(key, function)?;
        self.emit_object_append_data_property_with_flags(
            &header,
            &key,
            value,
            writable,
            enumerable,
            configurable,
            function,
        )?;
        key.clear(function);
        header.clear(function);
        Ok(())
    }

    /// Installation may encounter the canonical Undefined placeholder before
    /// it publishes an intrinsic. Runtime readers require the completed object.
    fn emit_load_intrinsic_for_installation(
        &self,
        realm: &crate::gc_types::GcLocal<crate::gc_types::RealmRecord>,
        slot: crate::functions::NonArrayRealmIntrinsicSlot,
        value: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) {
        use crate::gc_types::*;
        let schema = self.runtime_schema();
        let table = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<RealmRecord>()
                .field(RealmRecordSchema::INTRINSICS)
                .read(realm, schema, function)
                .reference(),
            function,
        );
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(slot.gc_index() as i32));
        index.store(function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<IntrinsicTable>()
                .read(&table, index, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, value, schema, function);
        stored.clear(function);
        schema.release_i32_local(index, function);
        table.clear(function);
    }

    pub(crate) fn emit_intrinsic_callable(
        &mut self,
        builtin: StandardBuiltinId,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<crate::gc_types::GcLocal<crate::gc_types::FunctionObject>, EmitError> {
        let meta = self
            .functions
            .get(&builtin.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported(format!(
                    "missing planned intrinsic {}",
                    builtin.debug_name()
                ))
            })?;
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.set_undefined(function);
        let slot = crate::module::standard_builtin_function_realm_slot(builtin);
        if let Some(slot) = slot {
            self.emit_load_intrinsic_for_installation(realm.realm(), slot, &value, function);
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(
                crate::WasmRuntimeValueTag::Undefined as i32,
            ));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
        }
        let callable = schema.reserve_gc_local(function).initialize(
            self.emit_function_value_payload_in_realm(&meta, realm, function)?,
            function,
        );
        value.set_reference(&callable, schema, function);
        if let Some(slot) = slot {
            self.emit_store_non_array_realm_intrinsic(realm.realm(), slot, &value, function);
        }
        callable.clear(function);
        if slot.is_some() {
            function.instruction(&Instruction::End);
        }
        let callable = schema.reserve_gc_local(function).initialize(
            value.cast_reference::<crate::gc_types::FunctionObject>(schema, function),
            function,
        );
        value.clear(function);
        Ok(callable)
    }

    pub(crate) fn emit_intrinsic_canonical_host_callable(
        &mut self,
        builtin: HostBuiltinId,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<crate::gc_types::GcLocal<crate::gc_types::FunctionObject>, EmitError> {
        let slot = crate::module::canonical_host_function_realm_slot(builtin)
            .ok_or_else(|| EmitError::unsupported("host intrinsic has no canonical Realm slot"))?;
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        self.emit_load_intrinsic_for_installation(realm.realm(), slot, &value, function);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(
            crate::WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let meta = self
            .functions
            .get(&builtin.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported(format!(
                    "missing planned host intrinsic {}",
                    builtin.as_str()
                ))
            })?;
        let callable = schema.reserve_gc_local(function).initialize(
            self.emit_function_value_payload_in_realm(&meta, realm, function)?,
            function,
        );
        value.set_reference(&callable, schema, function);
        self.emit_store_non_array_realm_intrinsic(realm.realm(), slot, &value, function);
        callable.clear(function);
        function.instruction(&Instruction::End);
        let callable = schema.reserve_gc_local(function).initialize(
            value.cast_reference::<crate::gc_types::FunctionObject>(schema, function),
            function,
        );
        value.clear(function);
        Ok(callable)
    }

    pub(crate) fn emit_install_intrinsic_method(
        &mut self,
        target: &crate::gc_types::ValueLocals,
        key: IntrinsicKey<'_>,
        builtin: StandardBuiltinId,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        writable: bool,
        configurable: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_install_intrinsic_method_aliases(
            target,
            &[key],
            builtin,
            realm,
            writable,
            configurable,
            function,
        )
    }

    pub(crate) fn emit_install_intrinsic_method_aliases(
        &mut self,
        target: &crate::gc_types::ValueLocals,
        keys: &[IntrinsicKey<'_>],
        builtin: StandardBuiltinId,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        writable: bool,
        configurable: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        if let Some(method) = deferred_methods::DeferredIntrinsicMethod::new(builtin) {
            return self.emit_install_deferred_intrinsic_method_aliases(
                target,
                keys,
                method,
                realm,
                writable,
                configurable,
                function,
            );
        }
        let callable = self.emit_intrinsic_callable(builtin, realm, function)?;
        let value = schema.reserve_value_local(function);
        value.set_reference(&callable, schema, function);
        for &key in keys {
            self.emit_install_intrinsic_data(
                target,
                key,
                &value,
                writable,
                false,
                configurable,
                function,
            )?;
        }
        value.clear(function);
        callable.clear(function);
        Ok(())
    }

    pub(crate) fn emit_install_intrinsic_accessor(
        &mut self,
        target: &crate::gc_types::ValueLocals,
        key: IntrinsicKey<'_>,
        accessors: AccessorDescriptor<StandardBuiltinId>,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        configurable: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let materialize = |builtin| -> Result<crate::gc_types::ValueLocals, EmitError> {
            let callable = self.emit_intrinsic_callable(builtin, realm, function)?;
            let value = schema.reserve_value_local(function);
            value.set_reference(&callable, schema, function);
            callable.clear(function);
            Ok(value)
        };
        let accessors = accessors.try_map(materialize)?;
        self.emit_install_intrinsic_accessor_values(
            target,
            key,
            accessors.borrowed(),
            configurable,
            function,
        )?;
        accessors.clear(function);
        Ok(())
    }

    pub(crate) fn emit_install_intrinsic_accessor_values(
        &mut self,
        target: &crate::gc_types::ValueLocals,
        key: IntrinsicKey<'_>,
        accessors: AccessorDescriptorLocals<'_>,
        configurable: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(target, function),
            function,
        );
        let key = self.intrinsic_key(key, function)?;
        self.emit_object_append_accessor_property_with_flags(
            &header,
            &key,
            accessors,
            false,
            configurable,
            function,
        )?;
        key.clear(function);
        header.clear(function);
        Ok(())
    }

    pub(crate) fn emit_install_intrinsic_string(
        &mut self,
        target: &crate::gc_types::ValueLocals,
        key: IntrinsicKey<'_>,
        text: &str,
        writable: bool,
        enumerable: bool,
        configurable: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(text, function)?,
            function,
        );
        let value = schema.reserve_value_local(function);
        value.set_reference(&string, schema, function);
        self.emit_install_intrinsic_data(
            target,
            key,
            &value,
            writable,
            enumerable,
            configurable,
            function,
        )?;
        value.clear(function);
        string.clear(function);
        Ok(())
    }

    pub(crate) fn emit_install_intrinsic_number(
        &mut self,
        target: &crate::gc_types::ValueLocals,
        name: &str,
        number: f64,
        writable: bool,
        enumerable: bool,
        configurable: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let value = self.runtime_schema().reserve_value_local(function);
        value.set_scalar(
            crate::gc_types::ScalarValue::NumberBits(number.to_bits() as i64),
            function,
        );
        self.emit_install_intrinsic_data(
            target,
            IntrinsicKey::Name(name),
            &value,
            writable,
            enumerable,
            configurable,
            function,
        )?;
        value.clear(function);
        Ok(())
    }
}
