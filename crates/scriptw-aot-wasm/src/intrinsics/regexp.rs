//! Intrinsic members retain property order and exact shared function identity.

use super::super::*;
use super::{IntrinsicInstall, IntrinsicKey};
use crate::objects::{AccessorDescriptor, AccessorGetter};

impl FunctionBuilder<'_> {
    pub(crate) fn install_regexp_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
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
        self.emit_install_intrinsic_methods(
            context.prototype,
            crate::intrinsics::IntrinsicMethodPlan::RegExpSymbols,
            context.realm,
            function,
        )?;
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
}
