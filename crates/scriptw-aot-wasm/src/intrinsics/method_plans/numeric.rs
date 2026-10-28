use super::*;

pub(super) const BIG_INT_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("toString", StandardBuiltinId::BigIntPrototypeToString),
    IntrinsicMethodSpec::named(
        "toLocaleString",
        StandardBuiltinId::BigIntPrototypeToLocaleString,
    ),
    IntrinsicMethodSpec::named("valueOf", StandardBuiltinId::BigIntPrototypeValueOf),
];

pub(super) const BIG_INT_CONSTRUCTOR: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("asIntN", StandardBuiltinId::BigIntAsIntN),
    IntrinsicMethodSpec::named("asUintN", StandardBuiltinId::BigIntAsUintN),
];

pub(super) const BOOLEAN_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("toString", StandardBuiltinId::BooleanPrototypeToString),
    IntrinsicMethodSpec::named("valueOf", StandardBuiltinId::BooleanPrototypeValueOf),
];

pub(super) const NUMBER_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::native(StandardBuiltinId::NumberPrototypeToFixed),
    IntrinsicMethodSpec::native(StandardBuiltinId::NumberPrototypeToExponential),
    IntrinsicMethodSpec::native(StandardBuiltinId::NumberPrototypeToPrecision),
    IntrinsicMethodSpec::native(StandardBuiltinId::NumberPrototypeToString),
    IntrinsicMethodSpec::native(StandardBuiltinId::NumberPrototypeToLocaleString),
    IntrinsicMethodSpec::native(StandardBuiltinId::NumberPrototypeValueOf),
];

pub(super) const NUMBER_CONSTRUCTOR: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::native(StandardBuiltinId::NumberIsInteger),
    IntrinsicMethodSpec::native(StandardBuiltinId::NumberIsSafeInteger),
    IntrinsicMethodSpec::native(StandardBuiltinId::NumberIsFinite),
    IntrinsicMethodSpec::native(StandardBuiltinId::NumberIsNaN),
];
