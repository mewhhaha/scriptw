//! Intrinsic members retain property order and exact shared function identity.

use super::super::*;
use super::{IntrinsicInstall, IntrinsicKey};
use crate::builtins::RegExpIntrinsicBuild;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::gc_types::{FunctionObject, GcLocal};
use crate::objects::{AccessorDescriptor, AccessorGetter};

impl FunctionBuilder<'_> {
    pub(crate) fn install_regexp_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        building: &RegExpIntrinsicBuild<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_install_intrinsic_methods(
            context.prototype,
            crate::intrinsics::IntrinsicMethodPlan::RegExpPrototypeBeforeAccessors,
            context.realm,
            function,
        )?;
        for (name, getter) in [
            ("source", StandardBuiltinId::RegExpPrototypeSourceGetter),
            (
                "hasIndices",
                StandardBuiltinId::RegExpPrototypeHasIndicesGetter,
            ),
            ("global", StandardBuiltinId::RegExpPrototypeGlobalGetter),
            (
                "ignoreCase",
                StandardBuiltinId::RegExpPrototypeIgnoreCaseGetter,
            ),
            (
                "multiline",
                StandardBuiltinId::RegExpPrototypeMultilineGetter,
            ),
            ("dotAll", StandardBuiltinId::RegExpPrototypeDotAllGetter),
            ("unicode", StandardBuiltinId::RegExpPrototypeUnicodeGetter),
            (
                "unicodeSets",
                StandardBuiltinId::RegExpPrototypeUnicodeSetsGetter,
            ),
            ("sticky", StandardBuiltinId::RegExpPrototypeStickyGetter),
            ("flags", StandardBuiltinId::RegExpPrototypeFlagsGetter),
        ] {
            self.emit_install_intrinsic_accessor(
                context.prototype,
                IntrinsicKey::Name(name),
                AccessorDescriptor::Getter(AccessorGetter::new(getter)),
                context.realm,
                true,
                function,
            )?;
        }
        // Keep the ordered plan, but canonical methods must not demand this
        // family again while its private builder is installing the graph.
        for &method in crate::intrinsics::IntrinsicMethodPlan::RegExpSymbols
            .methods()?
            .iter()
        {
            match method.builtin {
                StandardBuiltinId::RegExpPrototypeSymbolMatch
                | StandardBuiltinId::RegExpPrototypeSymbolMatchAll
                | StandardBuiltinId::RegExpPrototypeSymbolSearch => {
                    let callable =
                        self.emit_building_regexp_callable(method.builtin, building, function)?;
                    let schema = self.runtime_schema();
                    let value = schema.reserve_value_local(function);
                    value.set_reference(&callable, schema, function);
                    self.emit_install_intrinsic_data(
                        context.prototype,
                        method.key,
                        &value,
                        method.writable,
                        false,
                        method.configurable,
                        function,
                    )?;
                    value.clear(function);
                    callable.clear(function);
                }
                _ => self.emit_install_intrinsic_method(
                    context.prototype,
                    method.key,
                    method.builtin,
                    context.realm,
                    method.writable,
                    method.configurable,
                    function,
                )?,
            }
        }
        self.emit_install_intrinsic_method(
            context.constructor,
            IntrinsicKey::Name("escape"),
            StandardBuiltinId::RegExpEscape,
            context.realm,
            true,
            true,
            function,
        )?;
        self.emit_install_intrinsic_accessor(
            context.constructor,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::Species),
            AccessorDescriptor::Getter(AccessorGetter::new(StandardBuiltinId::RegExpSpeciesGetter)),
            context.realm,
            true,
            function,
        )?;
        self.emit_install_regexp_legacy_accessors(context, function)?;
        Ok(())
    }

    // The private capability supplies the defining Realm for both slot reads
    // and callable construction; callers cannot pair it with another context.
    fn emit_building_regexp_callable(
        &mut self,
        builtin: StandardBuiltinId,
        building: &RegExpIntrinsicBuild<'_>,
        function: &mut Function,
    ) -> Result<GcLocal<FunctionObject>, EmitError> {
        let slot = match builtin {
            StandardBuiltinId::RegExpPrototypeSymbolMatch => {
                NonArrayRealmIntrinsicSlot::RegExpPrototypeSymbolMatch
            }
            StandardBuiltinId::RegExpPrototypeSymbolMatchAll => {
                NonArrayRealmIntrinsicSlot::RegExpPrototypeSymbolMatchAll
            }
            StandardBuiltinId::RegExpPrototypeSymbolSearch => {
                NonArrayRealmIntrinsicSlot::RegExpPrototypeSymbolSearch
            }
            _ => {
                return Err(EmitError::unsupported(
                    "RegExp build capability requires a canonical symbol method",
                ))
            }
        };
        let meta = self
            .functions
            .get(&builtin.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported("RegExp canonical method lacks planned native entry")
            })?;
        let schema = self.runtime_schema();
        // The single Dormant-to-Building owner installs each canonical method
        // once. Its slot is still Undefined, so no completed-slot read is valid.
        let value = schema.reserve_value_local(function);
        let realm =
            self.emit_realm_function_materialization_context_from_realm(building.realm(), function);
        let callable = schema.reserve_gc_local(function).initialize(
            self.emit_function_value_payload_in_realm(&meta, &realm, function)?,
            function,
        );
        value.set_reference(&callable, schema, function);
        self.emit_store_non_array_realm_intrinsic(building.realm(), slot, &value, function);
        self.release_realm_function_materialization_context(realm, function);
        value.clear(function);
        Ok(callable)
    }
}
