use super::*;
use crate::functions::{NonArrayRealmIntrinsicSlot, RealmFunctionMaterializationContext};
use crate::objects::{AccessorDescriptor, AccessorGetter};

/// Every implemented Temporal constructor family. Both bootstrap paths and
/// allocation policies consume this closed domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TemporalIntrinsicFamily {
    Instant,
    Duration,
    PlainDate,
    ZonedDateTime,
    PlainTime,
    PlainDateTime,
    PlainYearMonth,
    PlainMonthDay,
}

impl TemporalIntrinsicFamily {
    pub(crate) fn constructor_installation_methods(
        self,
    ) -> Result<Vec<crate::intrinsics::IntrinsicMethodSpec>, EmitError> {
        self.constructor_methods()
            .iter()
            .copied()
            .map(Self::installation_method)
            .collect()
    }

    pub(crate) fn prototype_installation_methods(
        self,
    ) -> Result<Vec<crate::intrinsics::IntrinsicMethodSpec>, EmitError> {
        self.methods().map(Self::installation_method).collect()
    }

    fn installation_method(
        builtin: StandardBuiltinId,
    ) -> Result<crate::intrinsics::IntrinsicMethodSpec, EmitError> {
        Ok(crate::intrinsics::IntrinsicMethodSpec {
            key: IntrinsicKey::Name(temporal_intrinsic_property_key(builtin)?),
            builtin,
            writable: true,
            configurable: true,
        })
    }

    pub(crate) const ALL: [Self; 8] = [
        Self::Instant,
        Self::PlainDate,
        Self::ZonedDateTime,
        Self::PlainTime,
        Self::PlainDateTime,
        Self::PlainYearMonth,
        Self::PlainMonthDay,
        Self::Duration,
    ];
    pub(crate) const fn constructor(self) -> StandardBuiltinId {
        match self {
            Self::Instant => StandardBuiltinId::TemporalInstantConstructor,
            Self::Duration => StandardBuiltinId::TemporalDurationConstructor,
            Self::PlainDate => StandardBuiltinId::TemporalPlainDateConstructor,
            Self::ZonedDateTime => StandardBuiltinId::TemporalZonedDateTimeConstructor,
            Self::PlainTime => StandardBuiltinId::TemporalPlainTimeConstructor,
            Self::PlainDateTime => StandardBuiltinId::TemporalPlainDateTimeConstructor,
            Self::PlainYearMonth => StandardBuiltinId::TemporalPlainYearMonthConstructor,
            Self::PlainMonthDay => StandardBuiltinId::TemporalPlainMonthDayConstructor,
        }
    }
    pub(crate) const fn prototype_slot(self) -> NonArrayRealmIntrinsicSlot {
        match self {
            Self::Instant => NonArrayRealmIntrinsicSlot::TemporalInstantPrototype,
            Self::Duration => NonArrayRealmIntrinsicSlot::TemporalDurationPrototype,
            Self::PlainDate => NonArrayRealmIntrinsicSlot::TemporalPlainDatePrototype,
            Self::ZonedDateTime => NonArrayRealmIntrinsicSlot::TemporalZonedDateTimePrototype,
            Self::PlainTime => NonArrayRealmIntrinsicSlot::TemporalPlainTimePrototype,
            Self::PlainDateTime => NonArrayRealmIntrinsicSlot::TemporalPlainDateTimePrototype,
            Self::PlainYearMonth => NonArrayRealmIntrinsicSlot::TemporalPlainYearMonthPrototype,
            Self::PlainMonthDay => NonArrayRealmIntrinsicSlot::TemporalPlainMonthDayPrototype,
        }
    }
    fn constructor_methods(self) -> &'static [StandardBuiltinId] {
        match self {
            Self::Instant => &[
                StandardBuiltinId::TemporalInstantFrom,
                StandardBuiltinId::TemporalInstantFromEpochMilliseconds,
                StandardBuiltinId::TemporalInstantFromEpochNanoseconds,
                StandardBuiltinId::TemporalInstantCompare,
            ],
            Self::Duration => &[
                StandardBuiltinId::TemporalDurationFrom,
                StandardBuiltinId::TemporalDurationCompare,
            ],
            Self::PlainDate => &[
                StandardBuiltinId::TemporalPlainDateFrom,
                StandardBuiltinId::TemporalPlainDateCompare,
            ],
            Self::ZonedDateTime => &[
                StandardBuiltinId::TemporalZonedDateTimeFrom,
                StandardBuiltinId::TemporalZonedDateTimeCompare,
            ],
            Self::PlainTime => &[
                StandardBuiltinId::TemporalPlainTimeFrom,
                StandardBuiltinId::TemporalPlainTimeCompare,
            ],
            Self::PlainDateTime => &[
                StandardBuiltinId::TemporalPlainDateTimeFrom,
                StandardBuiltinId::TemporalPlainDateTimeCompare,
            ],
            Self::PlainYearMonth => &[
                StandardBuiltinId::TemporalPlainYearMonthFrom,
                StandardBuiltinId::TemporalPlainYearMonthCompare,
            ],
            Self::PlainMonthDay => &[StandardBuiltinId::TemporalPlainMonthDayFrom],
        }
    }
    fn getters(self) -> &'static [StandardBuiltinId] {
        match self {
            Self::Instant => &[
                StandardBuiltinId::TemporalInstantPrototypeEpochMillisecondsGetter,
                StandardBuiltinId::TemporalInstantPrototypeEpochNanosecondsGetter,
            ],
            Self::Duration => &[
                StandardBuiltinId::TemporalDurationPrototypeYearsGetter,
                StandardBuiltinId::TemporalDurationPrototypeMonthsGetter,
                StandardBuiltinId::TemporalDurationPrototypeWeeksGetter,
                StandardBuiltinId::TemporalDurationPrototypeDaysGetter,
                StandardBuiltinId::TemporalDurationPrototypeHoursGetter,
                StandardBuiltinId::TemporalDurationPrototypeMinutesGetter,
                StandardBuiltinId::TemporalDurationPrototypeSecondsGetter,
                StandardBuiltinId::TemporalDurationPrototypeMillisecondsGetter,
                StandardBuiltinId::TemporalDurationPrototypeMicrosecondsGetter,
                StandardBuiltinId::TemporalDurationPrototypeNanosecondsGetter,
                StandardBuiltinId::TemporalDurationPrototypeSignGetter,
                StandardBuiltinId::TemporalDurationPrototypeBlankGetter,
            ],
            Self::PlainDate => &[
                StandardBuiltinId::TemporalPlainDatePrototypeCalendarIdGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeEraGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeEraYearGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeYearGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeMonthGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeMonthCodeGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeDayGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeDayOfWeekGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeDayOfYearGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeWeekOfYearGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeYearOfWeekGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeDaysInWeekGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeDaysInMonthGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeDaysInYearGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeMonthsInYearGetter,
                StandardBuiltinId::TemporalPlainDatePrototypeInLeapYearGetter,
            ],
            Self::ZonedDateTime => &[
                StandardBuiltinId::TemporalZonedDateTimePrototypeEpochMillisecondsGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeEpochNanosecondsGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetNanosecondsGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeTimeZoneIdGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeCalendarIdGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeEraGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeEraYearGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeYearGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeMonthGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeMonthCodeGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeDayGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeHourGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeMinuteGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeSecondGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeMillisecondGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeMicrosecondGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeNanosecondGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfWeekGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfYearGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeWeekOfYearGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeYearOfWeekGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInWeekGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInMonthGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInYearGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeHoursInDayGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeMonthsInYearGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeInLeapYearGetter,
            ],
            Self::PlainTime => &[
                StandardBuiltinId::TemporalPlainTimePrototypeHourGetter,
                StandardBuiltinId::TemporalPlainTimePrototypeMinuteGetter,
                StandardBuiltinId::TemporalPlainTimePrototypeSecondGetter,
                StandardBuiltinId::TemporalPlainTimePrototypeMillisecondGetter,
                StandardBuiltinId::TemporalPlainTimePrototypeMicrosecondGetter,
                StandardBuiltinId::TemporalPlainTimePrototypeNanosecondGetter,
            ],
            Self::PlainDateTime => &[
                StandardBuiltinId::TemporalPlainDateTimePrototypeCalendarIdGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeEraGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeEraYearGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeYearGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeMonthGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeMonthCodeGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeDayGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeHourGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeMinuteGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeSecondGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeMillisecondGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeMicrosecondGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeNanosecondGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfWeekGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfYearGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeWeekOfYearGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeYearOfWeekGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInWeekGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInMonthGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInYearGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeMonthsInYearGetter,
                StandardBuiltinId::TemporalPlainDateTimePrototypeInLeapYearGetter,
            ],
            Self::PlainYearMonth => &[
                StandardBuiltinId::TemporalPlainYearMonthPrototypeCalendarIdGetter,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeEraGetter,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeEraYearGetter,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeYearGetter,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthGetter,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthCodeGetter,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInYearGetter,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInMonthGetter,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthsInYearGetter,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeInLeapYearGetter,
            ],
            Self::PlainMonthDay => &[
                StandardBuiltinId::TemporalPlainMonthDayPrototypeCalendarIdGetter,
                StandardBuiltinId::TemporalPlainMonthDayPrototypeMonthCodeGetter,
                StandardBuiltinId::TemporalPlainMonthDayPrototypeDayGetter,
            ],
        }
    }
    fn methods(self) -> impl Iterator<Item = StandardBuiltinId> {
        let fixed: &'static [StandardBuiltinId] = match self {
            Self::Instant => &[
                StandardBuiltinId::TemporalInstantPrototypeAdd,
                StandardBuiltinId::TemporalInstantPrototypeSubtract,
                StandardBuiltinId::TemporalInstantPrototypeRound,
                StandardBuiltinId::TemporalInstantPrototypeUntil,
                StandardBuiltinId::TemporalInstantPrototypeSince,
                StandardBuiltinId::TemporalInstantPrototypeToString,
                StandardBuiltinId::TemporalInstantPrototypeToLocaleString,
                StandardBuiltinId::TemporalInstantPrototypeEquals,
                StandardBuiltinId::TemporalInstantPrototypeToJson,
                StandardBuiltinId::TemporalInstantPrototypeValueOf,
                StandardBuiltinId::TemporalInstantPrototypeToZonedDateTimeIso,
            ],
            Self::Duration => &[
                StandardBuiltinId::TemporalDurationPrototypeWith,
                StandardBuiltinId::TemporalDurationPrototypeNegated,
                StandardBuiltinId::TemporalDurationPrototypeAbs,
                StandardBuiltinId::TemporalDurationPrototypeAdd,
                StandardBuiltinId::TemporalDurationPrototypeSubtract,
                StandardBuiltinId::TemporalDurationPrototypeRound,
                StandardBuiltinId::TemporalDurationPrototypeTotal,
                StandardBuiltinId::TemporalDurationPrototypeToString,
                StandardBuiltinId::TemporalDurationPrototypeToJson,
                StandardBuiltinId::TemporalDurationPrototypeToLocaleString,
                StandardBuiltinId::TemporalDurationPrototypeValueOf,
            ],
            Self::PlainDate => &[
                StandardBuiltinId::TemporalPlainDatePrototypeWith,
                StandardBuiltinId::TemporalPlainDatePrototypeWithCalendar,
                StandardBuiltinId::TemporalPlainDatePrototypeEquals,
                StandardBuiltinId::TemporalPlainDatePrototypeToString,
                StandardBuiltinId::TemporalPlainDatePrototypeToJson,
                StandardBuiltinId::TemporalPlainDatePrototypeToLocaleString,
                StandardBuiltinId::TemporalPlainDatePrototypeValueOf,
                StandardBuiltinId::TemporalPlainDatePrototypeAdd,
                StandardBuiltinId::TemporalPlainDatePrototypeSubtract,
                StandardBuiltinId::TemporalPlainDatePrototypeUntil,
                StandardBuiltinId::TemporalPlainDatePrototypeSince,
                StandardBuiltinId::TemporalPlainDatePrototypeToPlainDateTime,
                StandardBuiltinId::TemporalPlainDatePrototypeToZonedDateTime,
                StandardBuiltinId::TemporalPlainDatePrototypeToPlainYearMonth,
                StandardBuiltinId::TemporalPlainDatePrototypeToPlainMonthDay,
            ],
            Self::ZonedDateTime => &[],
            Self::PlainTime => &[
                StandardBuiltinId::TemporalPlainTimePrototypeAdd,
                StandardBuiltinId::TemporalPlainTimePrototypeSubtract,
                StandardBuiltinId::TemporalPlainTimePrototypeWith,
                StandardBuiltinId::TemporalPlainTimePrototypeUntil,
                StandardBuiltinId::TemporalPlainTimePrototypeSince,
                StandardBuiltinId::TemporalPlainTimePrototypeRound,
                StandardBuiltinId::TemporalPlainTimePrototypeEquals,
                StandardBuiltinId::TemporalPlainTimePrototypeToString,
                StandardBuiltinId::TemporalPlainTimePrototypeToJson,
                StandardBuiltinId::TemporalPlainTimePrototypeToLocaleString,
                StandardBuiltinId::TemporalPlainTimePrototypeValueOf,
            ],
            Self::PlainDateTime => &[
                StandardBuiltinId::TemporalPlainDateTimePrototypeWith,
                StandardBuiltinId::TemporalPlainDateTimePrototypeWithPlainTime,
                StandardBuiltinId::TemporalPlainDateTimePrototypeWithCalendar,
                StandardBuiltinId::TemporalPlainDateTimePrototypeAdd,
                StandardBuiltinId::TemporalPlainDateTimePrototypeSubtract,
                StandardBuiltinId::TemporalPlainDateTimePrototypeUntil,
                StandardBuiltinId::TemporalPlainDateTimePrototypeSince,
                StandardBuiltinId::TemporalPlainDateTimePrototypeRound,
                StandardBuiltinId::TemporalPlainDateTimePrototypeEquals,
                StandardBuiltinId::TemporalPlainDateTimePrototypeToString,
                StandardBuiltinId::TemporalPlainDateTimePrototypeToJson,
                StandardBuiltinId::TemporalPlainDateTimePrototypeToLocaleString,
                StandardBuiltinId::TemporalPlainDateTimePrototypeValueOf,
                StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainDate,
                StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainTime,
                StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime,
            ],
            Self::PlainYearMonth => &[
                StandardBuiltinId::TemporalPlainYearMonthPrototypeWith,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeAdd,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeSubtract,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeUntil,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeSince,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeEquals,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeToString,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeToJson,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeToLocaleString,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeValueOf,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeToPlainDate,
            ],
            Self::PlainMonthDay => &[
                StandardBuiltinId::TemporalPlainMonthDayPrototypeWith,
                StandardBuiltinId::TemporalPlainMonthDayPrototypeEquals,
                StandardBuiltinId::TemporalPlainMonthDayPrototypeToString,
                StandardBuiltinId::TemporalPlainMonthDayPrototypeToJson,
                StandardBuiltinId::TemporalPlainMonthDayPrototypeToLocaleString,
                StandardBuiltinId::TemporalPlainMonthDayPrototypeValueOf,
                StandardBuiltinId::TemporalPlainMonthDayPrototypeToPlainDate,
            ],
        };
        fixed.iter().copied().chain(
            TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_METHODS
                .iter()
                .filter(move |_| matches!(self, Self::ZonedDateTime))
                .map(|(_, builtin)| *builtin),
        )
    }
}

// A namespace cannot advertise an implemented constructor that created bootstrap
// omits, or install it under a different ordered family row.
const _: () = {
    assert!(TemporalIntrinsicFamily::ALL.len() == TEMPORAL_NAMESPACE_CONSTRUCTORS.len());
    let mut index = 0;
    while index < TemporalIntrinsicFamily::ALL.len() {
        assert!(
            TemporalIntrinsicFamily::ALL[index].constructor() as usize
                == TEMPORAL_NAMESPACE_CONSTRUCTORS[index].1 as usize
        );
        index += 1;
    }
};

impl FunctionBuilder<'_> {
    pub(crate) fn emit_install_temporal_intrinsic_members(
        &mut self,
        family: TemporalIntrinsicFamily,
        constructor: &crate::gc_types::ValueLocals,
        prototype: &crate::gc_types::ValueLocals,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_install_intrinsic_methods(
            constructor,
            crate::intrinsics::IntrinsicMethodPlan::TemporalConstructor(family),
            realm,
            function,
        )?;
        for builtin in family.getters() {
            self.emit_install_intrinsic_accessor(
                prototype,
                IntrinsicKey::Name(temporal_intrinsic_property_key(*builtin)?),
                AccessorDescriptor::Getter(AccessorGetter::new(*builtin)),
                realm,
                true,
                function,
            )?;
        }
        self.emit_install_intrinsic_methods(
            prototype,
            crate::intrinsics::IntrinsicMethodPlan::TemporalPrototype(family),
            realm,
            function,
        )?;
        self.emit_define_temporal_intrinsic_to_string_tag(
            prototype,
            family.constructor().debug_name(),
            function,
        )
    }

    pub(crate) fn emit_define_temporal_intrinsic_to_string_tag(
        &mut self,
        object: &crate::gc_types::ValueLocals,
        name: &str,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_install_intrinsic_string(
            object,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            name,
            false,
            false,
            true,
            function,
        )
    }
}
