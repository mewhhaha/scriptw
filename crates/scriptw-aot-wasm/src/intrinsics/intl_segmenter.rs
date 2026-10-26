//! Realm-owned auxiliary Segmenter prototypes. Their instances are produced by
//! compiled builtins; they have no public constructor or entry-Realm fallback.
use super::super::*;
use super::IntrinsicInstall;
use super::intl::{IntlIntrinsicProperty, IntlIntrinsicPropertyKind};

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
        self.install_intl_constructor_intrinsics(context, function)?;
        let schema = self.runtime_schema();
        for (slot, properties, to_string_tag) in [
            (
                crate::functions::NonArrayRealmIntrinsicSlot::IntlSegmentsPrototype,
                SEGMENTS_PROTOTYPE_PROPERTIES,
                None,
            ),
            (
                crate::functions::NonArrayRealmIntrinsicSlot::IntlSegmentIteratorPrototype,
                SEGMENT_ITERATOR_PROTOTYPE_PROPERTIES,
                Some("Segmenter String Iterator"),
            ),
        ] {
            let prototype = schema.reserve_value_local(function);
            self.emit_load_non_array_realm_intrinsic(
                context.realm.realm(),
                slot,
                &prototype,
                function,
            );
            for property in properties {
                self.emit_define_intl_intrinsic_function_property(
                    &prototype,
                    property,
                    context.realm,
                    function,
                )?;
            }
            if let Some(name) = to_string_tag {
                self.emit_define_intl_intrinsic_to_string_tag(&prototype, name, function)?;
            }
            prototype.clear(function);
        }
        Ok(())
    }
}
