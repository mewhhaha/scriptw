//! Shared Intl property order for entry and created Realm installation.

use super::super::*;
use super::{IntrinsicInstall, IntrinsicKey};
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::objects::{AccessorDescriptorLocals, AccessorGetterLocals};

#[derive(Clone, Copy)]
pub(crate) enum IntlIntrinsicPropertyKind {
    Getter,
    Method,
    SymbolMethod(scriptw_ir::WellKnownSymbol),
}

pub(crate) struct IntlIntrinsicProperty {
    pub(crate) name: &'static str,
    pub(crate) builtin: StandardBuiltinId,
    pub(crate) kind: IntlIntrinsicPropertyKind,
}

pub(crate) struct IntlConstructorProperties {
    pub(crate) prototype_name: &'static str,
    pub(crate) prototype_slot: NonArrayRealmIntrinsicSlot,
    pub(crate) constructor: &'static [IntlIntrinsicProperty],
    pub(crate) prototype: &'static [IntlIntrinsicProperty],
}

const LOCALE_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] = &[
    IntlIntrinsicProperty {
        name: "language",
        builtin: StandardBuiltinId::IntlLocalePrototypeLanguageGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "script",
        builtin: StandardBuiltinId::IntlLocalePrototypeScriptGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "region",
        builtin: StandardBuiltinId::IntlLocalePrototypeRegionGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "baseName",
        builtin: StandardBuiltinId::IntlLocalePrototypeBaseNameGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "calendar",
        builtin: StandardBuiltinId::IntlLocalePrototypeCalendarGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "collation",
        builtin: StandardBuiltinId::IntlLocalePrototypeCollationGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "firstDayOfWeek",
        builtin: StandardBuiltinId::IntlLocalePrototypeFirstDayOfWeekGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "hourCycle",
        builtin: StandardBuiltinId::IntlLocalePrototypeHourCycleGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "caseFirst",
        builtin: StandardBuiltinId::IntlLocalePrototypeCaseFirstGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "numeric",
        builtin: StandardBuiltinId::IntlLocalePrototypeNumericGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "numberingSystem",
        builtin: StandardBuiltinId::IntlLocalePrototypeNumberingSystemGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "variants",
        builtin: StandardBuiltinId::IntlLocalePrototypeVariantsGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "maximize",
        builtin: StandardBuiltinId::IntlLocalePrototypeMaximize,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "minimize",
        builtin: StandardBuiltinId::IntlLocalePrototypeMinimize,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "getCalendars",
        builtin: StandardBuiltinId::IntlLocalePrototypeGetCalendars,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "getCollations",
        builtin: StandardBuiltinId::IntlLocalePrototypeGetCollations,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "getTimeZones",
        builtin: StandardBuiltinId::IntlLocalePrototypeGetTimeZones,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "getNumberingSystems",
        builtin: StandardBuiltinId::IntlLocalePrototypeGetNumberingSystems,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "getHourCycles",
        builtin: StandardBuiltinId::IntlLocalePrototypeGetHourCycles,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "getTextInfo",
        builtin: StandardBuiltinId::IntlLocalePrototypeGetTextInfo,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "getWeekInfo",
        builtin: StandardBuiltinId::IntlLocalePrototypeGetWeekInfo,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "toString",
        builtin: StandardBuiltinId::IntlLocalePrototypeToString,
        kind: IntlIntrinsicPropertyKind::Method,
    },
];

const DATE_TIME_FORMAT_CONSTRUCTOR_PROPERTIES: &[IntlIntrinsicProperty] =
    &[IntlIntrinsicProperty {
        name: "supportedLocalesOf",
        builtin: StandardBuiltinId::IntlDateTimeFormatSupportedLocalesOf,
        kind: IntlIntrinsicPropertyKind::Method,
    }];

const DATE_TIME_FORMAT_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] = &[
    IntlIntrinsicProperty {
        name: "format",
        builtin: StandardBuiltinId::IntlDateTimeFormatPrototypeFormatGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "formatToParts",
        builtin: StandardBuiltinId::IntlDateTimeFormatPrototypeFormatToParts,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "formatRange",
        builtin: StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRange,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "formatRangeToParts",
        builtin: StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRangeToParts,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "resolvedOptions",
        builtin: StandardBuiltinId::IntlDateTimeFormatPrototypeResolvedOptions,
        kind: IntlIntrinsicPropertyKind::Method,
    },
];

const NUMBER_FORMAT_CONSTRUCTOR_PROPERTIES: &[IntlIntrinsicProperty] = &[IntlIntrinsicProperty {
    name: "supportedLocalesOf",
    builtin: StandardBuiltinId::IntlNumberFormatSupportedLocalesOf,
    kind: IntlIntrinsicPropertyKind::Method,
}];

const NUMBER_FORMAT_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] = &[
    IntlIntrinsicProperty {
        name: "format",
        builtin: StandardBuiltinId::IntlNumberFormatPrototypeFormatGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "formatToParts",
        builtin: StandardBuiltinId::IntlNumberFormatPrototypeFormatToParts,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "formatRange",
        builtin: StandardBuiltinId::IntlNumberFormatPrototypeFormatRange,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "formatRangeToParts",
        builtin: StandardBuiltinId::IntlNumberFormatPrototypeFormatRangeToParts,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "resolvedOptions",
        builtin: StandardBuiltinId::IntlNumberFormatPrototypeResolvedOptions,
        kind: IntlIntrinsicPropertyKind::Method,
    },
];

/// Namespace membership is separately proven by IntlNamespaceMembers. A newly
/// catalogued constructor must also supply its complete intrinsic properties.
const PLURAL_RULES_CONSTRUCTOR_PROPERTIES: &[IntlIntrinsicProperty] = &[IntlIntrinsicProperty {
    name: "supportedLocalesOf",
    builtin: StandardBuiltinId::IntlPluralRulesSupportedLocalesOf,
    kind: IntlIntrinsicPropertyKind::Method,
}];
const PLURAL_RULES_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] = &[
    IntlIntrinsicProperty {
        name: "resolvedOptions",
        builtin: StandardBuiltinId::IntlPluralRulesPrototypeResolvedOptions,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "select",
        builtin: StandardBuiltinId::IntlPluralRulesPrototypeSelect,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "selectRange",
        builtin: StandardBuiltinId::IntlPluralRulesPrototypeSelectRange,
        kind: IntlIntrinsicPropertyKind::Method,
    },
];
const LIST_FORMAT_CONSTRUCTOR_PROPERTIES: &[IntlIntrinsicProperty] = &[IntlIntrinsicProperty {
    name: "supportedLocalesOf",
    builtin: StandardBuiltinId::IntlListFormatSupportedLocalesOf,
    kind: IntlIntrinsicPropertyKind::Method,
}];
const LIST_FORMAT_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] = &[
    IntlIntrinsicProperty {
        name: "resolvedOptions",
        builtin: StandardBuiltinId::IntlListFormatPrototypeResolvedOptions,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "format",
        builtin: StandardBuiltinId::IntlListFormatPrototypeFormat,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "formatToParts",
        builtin: StandardBuiltinId::IntlListFormatPrototypeFormatToParts,
        kind: IntlIntrinsicPropertyKind::Method,
    },
];

const COLLATOR_CONSTRUCTOR_PROPERTIES: &[IntlIntrinsicProperty] = &[IntlIntrinsicProperty {
    name: "supportedLocalesOf",
    builtin: StandardBuiltinId::IntlCollatorSupportedLocalesOf,
    kind: IntlIntrinsicPropertyKind::Method,
}];
const COLLATOR_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] = &[
    IntlIntrinsicProperty {
        name: "compare",
        builtin: StandardBuiltinId::IntlCollatorPrototypeCompareGetter,
        kind: IntlIntrinsicPropertyKind::Getter,
    },
    IntlIntrinsicProperty {
        name: "resolvedOptions",
        builtin: StandardBuiltinId::IntlCollatorPrototypeResolvedOptions,
        kind: IntlIntrinsicPropertyKind::Method,
    },
];

const DISPLAY_NAMES_CONSTRUCTOR_PROPERTIES: &[IntlIntrinsicProperty] = &[IntlIntrinsicProperty {
    name: "supportedLocalesOf",
    builtin: StandardBuiltinId::IntlDisplayNamesSupportedLocalesOf,
    kind: IntlIntrinsicPropertyKind::Method,
}];
const DISPLAY_NAMES_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] = &[
    IntlIntrinsicProperty {
        name: "resolvedOptions",
        builtin: StandardBuiltinId::IntlDisplayNamesPrototypeResolvedOptions,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "of",
        builtin: StandardBuiltinId::IntlDisplayNamesPrototypeOf,
        kind: IntlIntrinsicPropertyKind::Method,
    },
];
const RELATIVE_TIME_FORMAT_CONSTRUCTOR_PROPERTIES: &[IntlIntrinsicProperty] =
    &[IntlIntrinsicProperty {
        name: "supportedLocalesOf",
        builtin: StandardBuiltinId::IntlRelativeTimeFormatSupportedLocalesOf,
        kind: IntlIntrinsicPropertyKind::Method,
    }];
const RELATIVE_TIME_FORMAT_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] = &[
    IntlIntrinsicProperty {
        name: "resolvedOptions",
        builtin: StandardBuiltinId::IntlRelativeTimeFormatPrototypeResolvedOptions,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "format",
        builtin: StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormat,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "formatToParts",
        builtin: StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormatToParts,
        kind: IntlIntrinsicPropertyKind::Method,
    },
];

const DURATION_FORMAT_CONSTRUCTOR_PROPERTIES: &[IntlIntrinsicProperty] = &[IntlIntrinsicProperty {
    name: "supportedLocalesOf",
    builtin: StandardBuiltinId::IntlDurationFormatSupportedLocalesOf,
    kind: IntlIntrinsicPropertyKind::Method,
}];
const DURATION_FORMAT_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] = &[
    IntlIntrinsicProperty {
        name: "resolvedOptions",
        builtin: StandardBuiltinId::IntlDurationFormatPrototypeResolvedOptions,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "format",
        builtin: StandardBuiltinId::IntlDurationFormatPrototypeFormat,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "formatToParts",
        builtin: StandardBuiltinId::IntlDurationFormatPrototypeFormatToParts,
        kind: IntlIntrinsicPropertyKind::Method,
    },
];

const SEGMENTER_CONSTRUCTOR_PROPERTIES: &[IntlIntrinsicProperty] = &[IntlIntrinsicProperty {
    name: "supportedLocalesOf",
    builtin: StandardBuiltinId::IntlSegmenterSupportedLocalesOf,
    kind: IntlIntrinsicPropertyKind::Method,
}];
const SEGMENTER_PROTOTYPE_PROPERTIES: &[IntlIntrinsicProperty] = &[
    IntlIntrinsicProperty {
        name: "segment",
        builtin: StandardBuiltinId::IntlSegmenterPrototypeSegment,
        kind: IntlIntrinsicPropertyKind::Method,
    },
    IntlIntrinsicProperty {
        name: "resolvedOptions",
        builtin: StandardBuiltinId::IntlSegmenterPrototypeResolvedOptions,
        kind: IntlIntrinsicPropertyKind::Method,
    },
];

pub(crate) fn intl_constructor_properties(
    builtin: StandardBuiltinId,
) -> Option<IntlConstructorProperties> {
    match builtin {
        StandardBuiltinId::IntlLocaleConstructor => Some(IntlConstructorProperties {
            prototype_name: "Intl.Locale",
            prototype_slot: NonArrayRealmIntrinsicSlot::IntlLocalePrototype,
            constructor: &[],
            prototype: LOCALE_PROTOTYPE_PROPERTIES,
        }),
        StandardBuiltinId::IntlDateTimeFormatConstructor => Some(IntlConstructorProperties {
            prototype_name: "Intl.DateTimeFormat",
            prototype_slot: NonArrayRealmIntrinsicSlot::IntlDateTimeFormatPrototype,
            constructor: DATE_TIME_FORMAT_CONSTRUCTOR_PROPERTIES,
            prototype: DATE_TIME_FORMAT_PROTOTYPE_PROPERTIES,
        }),
        StandardBuiltinId::IntlNumberFormatConstructor => Some(IntlConstructorProperties {
            prototype_name: "Intl.NumberFormat",
            prototype_slot: NonArrayRealmIntrinsicSlot::IntlNumberFormatPrototype,
            constructor: NUMBER_FORMAT_CONSTRUCTOR_PROPERTIES,
            prototype: NUMBER_FORMAT_PROTOTYPE_PROPERTIES,
        }),
        StandardBuiltinId::IntlPluralRulesConstructor => Some(IntlConstructorProperties {
            prototype_name: "Intl.PluralRules",
            prototype_slot: NonArrayRealmIntrinsicSlot::IntlPluralRulesPrototype,
            constructor: PLURAL_RULES_CONSTRUCTOR_PROPERTIES,
            prototype: PLURAL_RULES_PROTOTYPE_PROPERTIES,
        }),
        StandardBuiltinId::IntlListFormatConstructor => Some(IntlConstructorProperties {
            prototype_name: "Intl.ListFormat",
            prototype_slot: NonArrayRealmIntrinsicSlot::IntlListFormatPrototype,
            constructor: LIST_FORMAT_CONSTRUCTOR_PROPERTIES,
            prototype: LIST_FORMAT_PROTOTYPE_PROPERTIES,
        }),
        StandardBuiltinId::IntlCollatorConstructor => Some(IntlConstructorProperties {
            prototype_name: "Intl.Collator",
            prototype_slot: NonArrayRealmIntrinsicSlot::IntlCollatorPrototype,
            constructor: COLLATOR_CONSTRUCTOR_PROPERTIES,
            prototype: COLLATOR_PROTOTYPE_PROPERTIES,
        }),
        StandardBuiltinId::IntlDisplayNamesConstructor => Some(IntlConstructorProperties {
            prototype_name: "Intl.DisplayNames",
            prototype_slot: NonArrayRealmIntrinsicSlot::IntlDisplayNamesPrototype,
            constructor: DISPLAY_NAMES_CONSTRUCTOR_PROPERTIES,
            prototype: DISPLAY_NAMES_PROTOTYPE_PROPERTIES,
        }),
        StandardBuiltinId::IntlRelativeTimeFormatConstructor => Some(IntlConstructorProperties {
            prototype_name: "Intl.RelativeTimeFormat",
            prototype_slot: NonArrayRealmIntrinsicSlot::IntlRelativeTimeFormatPrototype,
            constructor: RELATIVE_TIME_FORMAT_CONSTRUCTOR_PROPERTIES,
            prototype: RELATIVE_TIME_FORMAT_PROTOTYPE_PROPERTIES,
        }),
        StandardBuiltinId::IntlDurationFormatConstructor => Some(IntlConstructorProperties {
            prototype_name: "Intl.DurationFormat",
            prototype_slot: NonArrayRealmIntrinsicSlot::IntlDurationFormatPrototype,
            constructor: DURATION_FORMAT_CONSTRUCTOR_PROPERTIES,
            prototype: DURATION_FORMAT_PROTOTYPE_PROPERTIES,
        }),
        StandardBuiltinId::IntlSegmenterConstructor => Some(IntlConstructorProperties {
            prototype_name: "Intl.Segmenter",
            prototype_slot: NonArrayRealmIntrinsicSlot::IntlSegmenterPrototype,
            constructor: SEGMENTER_CONSTRUCTOR_PROPERTIES,
            prototype: SEGMENTER_PROTOTYPE_PROPERTIES,
        }),
        _ => None,
    }
}

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn install_intl_locale_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.install_intl_constructor_intrinsics(context, function)
    }

    pub(crate) fn install_intl_date_time_format_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.install_intl_constructor_intrinsics(context, function)
    }

    pub(crate) fn install_intl_number_format_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.install_intl_constructor_intrinsics(context, function)
    }

    pub(crate) fn install_intl_plural_rules_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.install_intl_constructor_intrinsics(context, function)
    }
    pub(crate) fn install_intl_list_format_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.install_intl_constructor_intrinsics(context, function)
    }

    pub(crate) fn install_intl_collator_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.install_intl_constructor_intrinsics(context, function)
    }

    pub(crate) fn install_intl_display_names_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.install_intl_constructor_intrinsics(context, function)
    }

    pub(crate) fn install_intl_relative_time_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.install_intl_constructor_intrinsics(context, function)
    }

    pub(crate) fn install_intl_durationformat_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.install_intl_constructor_intrinsics(context, function)
    }

    pub(super) fn install_intl_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let properties = intl_constructor_properties(context.builtin).ok_or_else(|| {
            EmitError::unsupported("missing represented Intl constructor properties")
        })?;
        for (receiver, entries) in [
            (context.constructor, properties.constructor),
            (context.prototype, properties.prototype),
        ] {
            for property in entries {
                self.emit_define_intl_intrinsic_function_property(
                    receiver,
                    property,
                    context.realm,
                    function,
                )?;
            }
        }
        self.emit_define_intl_intrinsic_to_string_tag(
            context.prototype,
            properties.prototype_name,
            function,
        )
    }

    pub(crate) fn emit_define_intl_intrinsic_function_property(
        &mut self,
        receiver: &crate::gc_types::ValueLocals,
        property: &IntlIntrinsicProperty,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match property.kind {
            IntlIntrinsicPropertyKind::Getter => {
                // Getter fields remain eager; only method data values use the
                // existing deferred intrinsic representation.
                let callable = self.emit_intrinsic_callable(property.builtin, realm, function)?;
                let schema = self.runtime_schema();
                let value = schema.reserve_value_local(function);
                value.set_reference(&callable, schema, function);
                self.emit_install_intrinsic_accessor_values(
                    receiver,
                    IntrinsicKey::Name(property.name),
                    AccessorDescriptorLocals::Getter(AccessorGetterLocals::new(&value)),
                    true,
                    function,
                )?;
                value.clear(function);
                callable.clear(function);
                Ok(())
            }
            IntlIntrinsicPropertyKind::Method => self.emit_install_intrinsic_method(
                receiver,
                IntrinsicKey::Name(property.name),
                property.builtin,
                realm,
                true,
                true,
                function,
            ),
            IntlIntrinsicPropertyKind::SymbolMethod(symbol) => self.emit_install_intrinsic_method(
                receiver,
                IntrinsicKey::Symbol(symbol),
                property.builtin,
                realm,
                true,
                true,
                function,
            ),
        }
    }

    pub(crate) fn emit_define_intl_intrinsic_to_string_tag(
        &mut self,
        receiver: &crate::gc_types::ValueLocals,
        name: &str,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_install_intrinsic_string(
            receiver,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            name,
            false,
            false,
            true,
            function,
        )
    }
}
