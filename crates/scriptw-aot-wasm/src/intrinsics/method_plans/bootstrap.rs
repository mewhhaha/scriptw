use super::*;

pub(super) const REFLECT: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("construct", StandardBuiltinId::ReflectConstruct),
    IntrinsicMethodSpec::named("apply", StandardBuiltinId::ReflectApply),
    IntrinsicMethodSpec::named("get", StandardBuiltinId::ReflectGet),
    IntrinsicMethodSpec::named("getPrototypeOf", StandardBuiltinId::ReflectGetPrototypeOf),
    IntrinsicMethodSpec::named(
        "getOwnPropertyDescriptor",
        StandardBuiltinId::ReflectGetOwnPropertyDescriptor,
    ),
    IntrinsicMethodSpec::named("set", StandardBuiltinId::ReflectSet),
    IntrinsicMethodSpec::named("has", StandardBuiltinId::ReflectHas),
    IntrinsicMethodSpec::named("defineProperty", StandardBuiltinId::ReflectDefineProperty),
    IntrinsicMethodSpec::named("deleteProperty", StandardBuiltinId::ReflectDeleteProperty),
    IntrinsicMethodSpec::named("isExtensible", StandardBuiltinId::ReflectIsExtensible),
    IntrinsicMethodSpec::named(
        "preventExtensions",
        StandardBuiltinId::ReflectPreventExtensions,
    ),
    IntrinsicMethodSpec::named("setPrototypeOf", StandardBuiltinId::ReflectSetPrototypeOf),
    IntrinsicMethodSpec::named("ownKeys", StandardBuiltinId::ReflectOwnKeys),
];

pub(super) const MATH: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("abs", StandardBuiltinId::MathAbs),
    IntrinsicMethodSpec::named("acos", StandardBuiltinId::MathAcos),
    IntrinsicMethodSpec::named("acosh", StandardBuiltinId::MathAcosh),
    IntrinsicMethodSpec::named("asin", StandardBuiltinId::MathAsin),
    IntrinsicMethodSpec::named("asinh", StandardBuiltinId::MathAsinh),
    IntrinsicMethodSpec::named("atan", StandardBuiltinId::MathAtan),
    IntrinsicMethodSpec::named("atan2", StandardBuiltinId::MathAtan2),
    IntrinsicMethodSpec::named("atanh", StandardBuiltinId::MathAtanh),
    IntrinsicMethodSpec::named("cbrt", StandardBuiltinId::MathCbrt),
    IntrinsicMethodSpec::named("ceil", StandardBuiltinId::MathCeil),
    IntrinsicMethodSpec::named("clz32", StandardBuiltinId::MathClz32),
    IntrinsicMethodSpec::named("cos", StandardBuiltinId::MathCos),
    IntrinsicMethodSpec::named("cosh", StandardBuiltinId::MathCosh),
    IntrinsicMethodSpec::named("exp", StandardBuiltinId::MathExp),
    IntrinsicMethodSpec::named("expm1", StandardBuiltinId::MathExpm1),
    IntrinsicMethodSpec::named("f16round", StandardBuiltinId::MathF16Round),
    IntrinsicMethodSpec::named("floor", StandardBuiltinId::MathFloor),
    IntrinsicMethodSpec::named("fround", StandardBuiltinId::MathFround),
    IntrinsicMethodSpec::named("hypot", StandardBuiltinId::MathHypot),
    IntrinsicMethodSpec::named("imul", StandardBuiltinId::MathImul),
    IntrinsicMethodSpec::named("log", StandardBuiltinId::MathLog),
    IntrinsicMethodSpec::named("log10", StandardBuiltinId::MathLog10),
    IntrinsicMethodSpec::named("log1p", StandardBuiltinId::MathLog1p),
    IntrinsicMethodSpec::named("log2", StandardBuiltinId::MathLog2),
    IntrinsicMethodSpec::named("pow", StandardBuiltinId::MathPow),
    IntrinsicMethodSpec::named("random", StandardBuiltinId::MathRandom),
    IntrinsicMethodSpec::named("round", StandardBuiltinId::MathRound),
    IntrinsicMethodSpec::named("sign", StandardBuiltinId::MathSign),
    IntrinsicMethodSpec::named("sin", StandardBuiltinId::MathSin),
    IntrinsicMethodSpec::named("sinh", StandardBuiltinId::MathSinh),
    IntrinsicMethodSpec::named("sqrt", StandardBuiltinId::MathSqrt),
    IntrinsicMethodSpec::named("sumPrecise", StandardBuiltinId::MathSumPrecise),
    IntrinsicMethodSpec::named("tan", StandardBuiltinId::MathTan),
    IntrinsicMethodSpec::named("tanh", StandardBuiltinId::MathTanh),
    IntrinsicMethodSpec::named("trunc", StandardBuiltinId::MathTrunc),
    IntrinsicMethodSpec::named("min", StandardBuiltinId::MathMin),
    IntrinsicMethodSpec::named("max", StandardBuiltinId::MathMax),
];

pub(super) const JSON: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("parse", StandardBuiltinId::JsonParse),
    IntrinsicMethodSpec::named("stringify", StandardBuiltinId::JsonStringify),
    IntrinsicMethodSpec::named("rawJSON", StandardBuiltinId::JsonRawJson),
    IntrinsicMethodSpec::named("isRawJSON", StandardBuiltinId::JsonIsRawJson),
];

pub(super) const TYPED_ARRAY_PROTOTYPE_BEFORE_ALIASES: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("at", StandardBuiltinId::TypedArrayPrototypeAt),
    IntrinsicMethodSpec::named("includes", StandardBuiltinId::TypedArrayPrototypeIncludes),
    IntrinsicMethodSpec::named("indexOf", StandardBuiltinId::TypedArrayPrototypeIndexOf),
    IntrinsicMethodSpec::named(
        "lastIndexOf",
        StandardBuiltinId::TypedArrayPrototypeLastIndexOf,
    ),
    IntrinsicMethodSpec::named("find", StandardBuiltinId::TypedArrayPrototypeFind),
    IntrinsicMethodSpec::named("findIndex", StandardBuiltinId::TypedArrayPrototypeFindIndex),
    IntrinsicMethodSpec::named("findLast", StandardBuiltinId::TypedArrayPrototypeFindLast),
    IntrinsicMethodSpec::named(
        "findLastIndex",
        StandardBuiltinId::TypedArrayPrototypeFindLastIndex,
    ),
    IntrinsicMethodSpec::named("every", StandardBuiltinId::TypedArrayPrototypeEvery),
    IntrinsicMethodSpec::named("some", StandardBuiltinId::TypedArrayPrototypeSome),
    IntrinsicMethodSpec::named("map", StandardBuiltinId::TypedArrayPrototypeMap),
    IntrinsicMethodSpec::named("filter", StandardBuiltinId::TypedArrayPrototypeFilter),
    IntrinsicMethodSpec::named("forEach", StandardBuiltinId::TypedArrayPrototypeForEach),
    IntrinsicMethodSpec::named("reduce", StandardBuiltinId::TypedArrayPrototypeReduce),
    IntrinsicMethodSpec::named(
        "reduceRight",
        StandardBuiltinId::TypedArrayPrototypeReduceRight,
    ),
];

pub(super) const TYPED_ARRAY_PROTOTYPE_AFTER_ALIASES: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("keys", StandardBuiltinId::TypedArrayPrototypeKeys),
    IntrinsicMethodSpec::named("entries", StandardBuiltinId::TypedArrayPrototypeEntries),
    IntrinsicMethodSpec::named("fill", StandardBuiltinId::TypedArrayPrototypeFill),
    IntrinsicMethodSpec::named("join", StandardBuiltinId::TypedArrayPrototypeJoin),
    IntrinsicMethodSpec::named("subarray", StandardBuiltinId::TypedArrayPrototypeSubarray),
    IntrinsicMethodSpec::named("slice", StandardBuiltinId::TypedArrayPrototypeSlice),
    IntrinsicMethodSpec::named("set", StandardBuiltinId::TypedArrayPrototypeSet),
    IntrinsicMethodSpec::named("reverse", StandardBuiltinId::TypedArrayPrototypeReverse),
    IntrinsicMethodSpec::named(
        "copyWithin",
        StandardBuiltinId::TypedArrayPrototypeCopyWithin,
    ),
    IntrinsicMethodSpec::named("sort", StandardBuiltinId::TypedArrayPrototypeSort),
    IntrinsicMethodSpec::named(
        "toReversed",
        StandardBuiltinId::TypedArrayPrototypeToReversed,
    ),
    IntrinsicMethodSpec::named("toSorted", StandardBuiltinId::TypedArrayPrototypeToSorted),
    IntrinsicMethodSpec::named("with", StandardBuiltinId::TypedArrayPrototypeWith),
];

pub(super) const TYPED_ARRAY_PROTOTYPE_AFTER_TO_STRING: &[IntrinsicMethodSpec] =
    &[IntrinsicMethodSpec::named(
        "toLocaleString",
        StandardBuiltinId::TypedArrayPrototypeToLocaleString,
    )];

pub(super) const TYPED_ARRAY_CONSTRUCTOR: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("from", StandardBuiltinId::TypedArrayFrom),
    IntrinsicMethodSpec::named("of", StandardBuiltinId::TypedArrayOf),
];
