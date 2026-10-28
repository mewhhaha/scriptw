//! Intrinsic members retain property order and exact shared function identity.

use super::super::*;
use super::{IntrinsicInstall, IntrinsicKey};
use crate::objects::{AccessorDescriptor, AccessorGetter};

impl FunctionBuilder<'_> {
    pub(crate) fn install_array_buffer_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let shared = context.builtin == StandardBuiltinId::SharedArrayBufferConstructor;
        if !shared {
            self.emit_install_intrinsic_method(
                context.constructor,
                IntrinsicKey::Name("isView"),
                StandardBuiltinId::ArrayBufferIsView,
                context.realm,
                true,
                true,
                function,
            )?;
        }
        self.emit_install_intrinsic_accessor(
            context.constructor,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::Species),
            AccessorDescriptor::Getter(AccessorGetter::new(
                StandardBuiltinId::ArrayBufferSpeciesGetter,
            )),
            context.realm,
            true,
            function,
        )?;
        self.emit_install_intrinsic_accessor(
            context.prototype,
            IntrinsicKey::Name("byteLength"),
            AccessorDescriptor::Getter(AccessorGetter::new(if shared {
                StandardBuiltinId::SharedArrayBufferPrototypeByteLengthGetter
            } else {
                StandardBuiltinId::ArrayBufferPrototypeByteLengthGetter
            })),
            context.realm,
            true,
            function,
        )?;
        if shared {
            self.emit_install_intrinsic_method(
                context.prototype,
                IntrinsicKey::Name("grow"),
                StandardBuiltinId::SharedArrayBufferPrototypeGrow,
                context.realm,
                true,
                true,
                function,
            )?;
            for (name, builtin) in [
                (
                    "maxByteLength",
                    StandardBuiltinId::SharedArrayBufferPrototypeMaxByteLengthGetter,
                ),
                (
                    "growable",
                    StandardBuiltinId::SharedArrayBufferPrototypeGrowableGetter,
                ),
            ] {
                self.emit_install_intrinsic_accessor(
                    context.prototype,
                    IntrinsicKey::Name(name),
                    AccessorDescriptor::Getter(AccessorGetter::new(builtin)),
                    context.realm,
                    true,
                    function,
                )?;
            }
        } else {
            for (name, builtin) in [
                (
                    "detached",
                    StandardBuiltinId::ArrayBufferPrototypeDetachedGetter,
                ),
                (
                    "maxByteLength",
                    StandardBuiltinId::ArrayBufferPrototypeMaxByteLengthGetter,
                ),
                (
                    "resizable",
                    StandardBuiltinId::ArrayBufferPrototypeResizableGetter,
                ),
                (
                    "immutable",
                    StandardBuiltinId::ArrayBufferPrototypeImmutableGetter,
                ),
            ] {
                self.emit_install_intrinsic_accessor(
                    context.prototype,
                    IntrinsicKey::Name(name),
                    AccessorDescriptor::Getter(AccessorGetter::new(builtin)),
                    context.realm,
                    true,
                    function,
                )?;
            }
        }
        self.emit_install_intrinsic_method(
            context.prototype,
            IntrinsicKey::Name("slice"),
            if shared {
                StandardBuiltinId::SharedArrayBufferPrototypeSlice
            } else {
                StandardBuiltinId::ArrayBufferPrototypeSlice
            },
            context.realm,
            true,
            true,
            function,
        )?;
        if !shared {
            self.emit_install_intrinsic_methods(
                context.prototype,
                crate::intrinsics::IntrinsicMethodPlan::ArrayBufferPrototypeMutable,
                context.realm,
                function,
            )?;
        }
        self.emit_install_intrinsic_string(
            context.prototype,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            if shared {
                "SharedArrayBuffer"
            } else {
                "ArrayBuffer"
            },
            false,
            false,
            true,
            function,
        )?;
        Ok(())
    }

    pub(crate) fn install_data_view_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        for (name, builtin) in [
            ("buffer", StandardBuiltinId::DataViewPrototypeBufferGetter),
            (
                "byteLength",
                StandardBuiltinId::DataViewPrototypeByteLengthGetter,
            ),
            (
                "byteOffset",
                StandardBuiltinId::DataViewPrototypeByteOffsetGetter,
            ),
        ] {
            self.emit_install_intrinsic_accessor(
                context.prototype,
                IntrinsicKey::Name(name),
                AccessorDescriptor::Getter(AccessorGetter::new(builtin)),
                context.realm,
                true,
                function,
            )?;
        }
        self.emit_install_intrinsic_methods(
            context.prototype,
            crate::intrinsics::IntrinsicMethodPlan::DataViewPrototype,
            context.realm,
            function,
        )?;
        self.emit_install_intrinsic_string(
            context.prototype,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            "DataView",
            false,
            false,
            true,
            function,
        )?;
        Ok(())
    }

    pub(crate) fn install_uint8_array_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        for (members, target) in [
            (
                scriptw_ir::UINT8_ARRAY_CODEC_STATIC_MEMBERS.as_slice(),
                context.constructor,
            ),
            (
                scriptw_ir::UINT8_ARRAY_CODEC_PROTOTYPE_MEMBERS.as_slice(),
                context.prototype,
            ),
        ] {
            for &(name, builtin) in members {
                self.emit_install_intrinsic_method(
                    target,
                    IntrinsicKey::Name(name),
                    builtin,
                    context.realm,
                    true,
                    true,
                    function,
                )?;
            }
        }
        Ok(())
    }
}
