use super::*;

pub(super) const DATE_CONSTRUCTOR: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("now", StandardBuiltinId::DateNow),
    IntrinsicMethodSpec::named("parse", StandardBuiltinId::DateParse),
    IntrinsicMethodSpec::named("UTC", StandardBuiltinId::DateUtc),
];

pub(super) const DATE_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("getTime", StandardBuiltinId::DatePrototypeGetTime),
    IntrinsicMethodSpec::named("setTime", StandardBuiltinId::DatePrototypeSetTime),
    IntrinsicMethodSpec::named("valueOf", StandardBuiltinId::DatePrototypeValueOf),
    IntrinsicMethodSpec::named("getFullYear", StandardBuiltinId::DatePrototypeGetFullYear),
    IntrinsicMethodSpec::named(
        "getUTCFullYear",
        StandardBuiltinId::DatePrototypeGetUtcFullYear,
    ),
    IntrinsicMethodSpec::named("getMonth", StandardBuiltinId::DatePrototypeGetMonth),
    IntrinsicMethodSpec::named("getUTCMonth", StandardBuiltinId::DatePrototypeGetUtcMonth),
    IntrinsicMethodSpec::named("getDate", StandardBuiltinId::DatePrototypeGetDate),
    IntrinsicMethodSpec::named("getUTCDate", StandardBuiltinId::DatePrototypeGetUtcDate),
    IntrinsicMethodSpec::named("getDay", StandardBuiltinId::DatePrototypeGetDay),
    IntrinsicMethodSpec::named("getUTCDay", StandardBuiltinId::DatePrototypeGetUtcDay),
    IntrinsicMethodSpec::named("getHours", StandardBuiltinId::DatePrototypeGetHours),
    IntrinsicMethodSpec::named("getUTCHours", StandardBuiltinId::DatePrototypeGetUtcHours),
    IntrinsicMethodSpec::named("getMinutes", StandardBuiltinId::DatePrototypeGetMinutes),
    IntrinsicMethodSpec::named(
        "getUTCMinutes",
        StandardBuiltinId::DatePrototypeGetUtcMinutes,
    ),
    IntrinsicMethodSpec::named("getSeconds", StandardBuiltinId::DatePrototypeGetSeconds),
    IntrinsicMethodSpec::named(
        "getUTCSeconds",
        StandardBuiltinId::DatePrototypeGetUtcSeconds,
    ),
    IntrinsicMethodSpec::named(
        "getMilliseconds",
        StandardBuiltinId::DatePrototypeGetMilliseconds,
    ),
    IntrinsicMethodSpec::named(
        "getUTCMilliseconds",
        StandardBuiltinId::DatePrototypeGetUtcMilliseconds,
    ),
    IntrinsicMethodSpec::named(
        "getTimezoneOffset",
        StandardBuiltinId::DatePrototypeGetTimezoneOffset,
    ),
    IntrinsicMethodSpec::named("getYear", StandardBuiltinId::DatePrototypeGetYear),
    IntrinsicMethodSpec::named("setYear", StandardBuiltinId::DatePrototypeSetYear),
    IntrinsicMethodSpec::named("setFullYear", StandardBuiltinId::DatePrototypeSetFullYear),
    IntrinsicMethodSpec::named(
        "setUTCFullYear",
        StandardBuiltinId::DatePrototypeSetUtcFullYear,
    ),
    IntrinsicMethodSpec::named("setMonth", StandardBuiltinId::DatePrototypeSetMonth),
    IntrinsicMethodSpec::named("setUTCMonth", StandardBuiltinId::DatePrototypeSetUtcMonth),
    IntrinsicMethodSpec::named("setDate", StandardBuiltinId::DatePrototypeSetDate),
    IntrinsicMethodSpec::named("setUTCDate", StandardBuiltinId::DatePrototypeSetUtcDate),
    IntrinsicMethodSpec::named("setHours", StandardBuiltinId::DatePrototypeSetHours),
    IntrinsicMethodSpec::named("setUTCHours", StandardBuiltinId::DatePrototypeSetUtcHours),
    IntrinsicMethodSpec::named("setMinutes", StandardBuiltinId::DatePrototypeSetMinutes),
    IntrinsicMethodSpec::named(
        "setUTCMinutes",
        StandardBuiltinId::DatePrototypeSetUtcMinutes,
    ),
    IntrinsicMethodSpec::named("setSeconds", StandardBuiltinId::DatePrototypeSetSeconds),
    IntrinsicMethodSpec::named(
        "setUTCSeconds",
        StandardBuiltinId::DatePrototypeSetUtcSeconds,
    ),
    IntrinsicMethodSpec::named(
        "setMilliseconds",
        StandardBuiltinId::DatePrototypeSetMilliseconds,
    ),
    IntrinsicMethodSpec::named(
        "setUTCMilliseconds",
        StandardBuiltinId::DatePrototypeSetUtcMilliseconds,
    ),
    IntrinsicMethodSpec::named("toISOString", StandardBuiltinId::DatePrototypeToIsoString),
    IntrinsicMethodSpec::named("toJSON", StandardBuiltinId::DatePrototypeToJson),
    IntrinsicMethodSpec::named("toDateString", StandardBuiltinId::DatePrototypeToDateString),
    IntrinsicMethodSpec::named(
        "toLocaleDateString",
        StandardBuiltinId::DatePrototypeToLocaleDateString,
    ),
    IntrinsicMethodSpec::named(
        "toLocaleString",
        StandardBuiltinId::DatePrototypeToLocaleString,
    ),
    IntrinsicMethodSpec::named(
        "toLocaleTimeString",
        StandardBuiltinId::DatePrototypeToLocaleTimeString,
    ),
    IntrinsicMethodSpec::named(
        "toTemporalInstant",
        StandardBuiltinId::DatePrototypeToTemporalInstant,
    ),
    IntrinsicMethodSpec::named("toTimeString", StandardBuiltinId::DatePrototypeToTimeString),
    IntrinsicMethodSpec::named("toString", StandardBuiltinId::DatePrototypeToString),
    IntrinsicMethodSpec::symbol(
        scriptw_ir::WellKnownSymbol::ToPrimitive,
        StandardBuiltinId::DatePrototypeToPrimitive,
        false,
    ),
];
