use super::*;

pub(super) const MAP_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("clear", StandardBuiltinId::MapPrototypeClear),
    IntrinsicMethodSpec::named("delete", StandardBuiltinId::MapPrototypeDelete),
    IntrinsicMethodSpec::named("forEach", StandardBuiltinId::MapPrototypeForEach),
    IntrinsicMethodSpec::named("get", StandardBuiltinId::MapPrototypeGet),
    IntrinsicMethodSpec::named("getOrInsert", StandardBuiltinId::MapPrototypeGetOrInsert),
    IntrinsicMethodSpec::named(
        "getOrInsertComputed",
        StandardBuiltinId::MapPrototypeGetOrInsertComputed,
    ),
    IntrinsicMethodSpec::named("has", StandardBuiltinId::MapPrototypeHas),
    IntrinsicMethodSpec::named("keys", StandardBuiltinId::MapPrototypeKeys),
    IntrinsicMethodSpec::named("set", StandardBuiltinId::MapPrototypeSet),
    IntrinsicMethodSpec::named("values", StandardBuiltinId::MapPrototypeValues),
];

pub(super) const WEAK_MAP_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("delete", StandardBuiltinId::WeakMapPrototypeDelete),
    IntrinsicMethodSpec::named("get", StandardBuiltinId::WeakMapPrototypeGet),
    IntrinsicMethodSpec::named(
        "getOrInsert",
        StandardBuiltinId::WeakMapPrototypeGetOrInsert,
    ),
    IntrinsicMethodSpec::named(
        "getOrInsertComputed",
        StandardBuiltinId::WeakMapPrototypeGetOrInsertComputed,
    ),
    IntrinsicMethodSpec::named("has", StandardBuiltinId::WeakMapPrototypeHas),
    IntrinsicMethodSpec::named("set", StandardBuiltinId::WeakMapPrototypeSet),
];

pub(super) const WEAK_SET_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("add", StandardBuiltinId::WeakSetPrototypeAdd),
    IntrinsicMethodSpec::named("delete", StandardBuiltinId::WeakSetPrototypeDelete),
    IntrinsicMethodSpec::named("has", StandardBuiltinId::WeakSetPrototypeHas),
];

pub(super) const ASYNC_DISPOSABLE_STACK_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named(
        "adopt",
        StandardBuiltinId::AsyncDisposableStackPrototypeAdopt,
    ),
    IntrinsicMethodSpec::named(
        "defer",
        StandardBuiltinId::AsyncDisposableStackPrototypeDefer,
    ),
    IntrinsicMethodSpec::named("move", StandardBuiltinId::AsyncDisposableStackPrototypeMove),
    IntrinsicMethodSpec::named("use", StandardBuiltinId::AsyncDisposableStackPrototypeUse),
];

pub(super) const SET_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("add", StandardBuiltinId::SetPrototypeAdd),
    IntrinsicMethodSpec::named("clear", StandardBuiltinId::SetPrototypeClear),
    IntrinsicMethodSpec::named("delete", StandardBuiltinId::SetPrototypeDelete),
    IntrinsicMethodSpec::named("difference", StandardBuiltinId::SetPrototypeDifference),
    IntrinsicMethodSpec::named("forEach", StandardBuiltinId::SetPrototypeForEach),
    IntrinsicMethodSpec::named("has", StandardBuiltinId::SetPrototypeHas),
    IntrinsicMethodSpec::named("intersection", StandardBuiltinId::SetPrototypeIntersection),
    IntrinsicMethodSpec::named(
        "isDisjointFrom",
        StandardBuiltinId::SetPrototypeIsDisjointFrom,
    ),
    IntrinsicMethodSpec::named("isSubsetOf", StandardBuiltinId::SetPrototypeIsSubsetOf),
    IntrinsicMethodSpec::named("isSupersetOf", StandardBuiltinId::SetPrototypeIsSupersetOf),
    IntrinsicMethodSpec::named(
        "symmetricDifference",
        StandardBuiltinId::SetPrototypeSymmetricDifference,
    ),
    IntrinsicMethodSpec::named("union", StandardBuiltinId::SetPrototypeUnion),
];
