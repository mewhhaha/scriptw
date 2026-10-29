//! Realm-owned auxiliary Segmenter prototypes. Their instances are produced by
//! compiled builtins; they have no public constructor or entry-Realm fallback.
use super::super::*;
use super::intl::{IntlIntrinsicProperty, IntlIntrinsicPropertyKind};
use super::IntrinsicInstall;
use crate::functions::RealmFunctionMaterializationContext;
use crate::gc_types::ValueLocals;

pub(crate) const SEGMENTS_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] = &[
    IntlIntrinsicProperty {
        name: "containing",
        builtin: StandardBuiltinId::IntlSegmentsPrototypeContaining,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "Symbol.iterator",
        builtin: StandardBuiltinId::IntlSegmentsPrototypeIterator,
        kind: IntlIntrinsicPropertyKind::SymbolMethod(scriptw_ir::WellKnownSymbol::Iterator),
    },
];
pub(crate) const SEGMENT_ITERATOR_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] =
    &[IntlIntrinsicProperty {
        name: "next",
        builtin: StandardBuiltinId::IntlSegmentIteratorPrototypeNext,
        kind: IntlIntrinsicPropertyKind::Method,
    }];

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn install_intl_segmenter_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.install_intl_constructor_intrinsics(context, function)
    }

    pub(crate) fn install_intl_segmenter_auxiliary_intrinsics(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        segments: &ValueLocals,
        segment_iterator: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        for (prototype, properties, to_string_tag) in [
            (segments, SEGMENTS_PROTOTYPE_PROPERTIES, None),
            (
                segment_iterator,
                SEGMENT_ITERATOR_PROTOTYPE_PROPERTIES,
                Some("Segmenter String Iterator"),
            ),
        ] {
            for property in properties {
                self.emit_define_intl_intrinsic_function_property(
                    prototype, property, realm, function,
                )?;
            }
            if let Some(name) = to_string_tag {
                self.emit_define_intl_intrinsic_to_string_tag(prototype, name, function)?;
            }
        }
        Ok(())
    }
}
