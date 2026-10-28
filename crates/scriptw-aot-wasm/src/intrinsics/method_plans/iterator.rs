use super::*;

pub(super) const ITERATOR_CONSTRUCTOR: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("from", StandardBuiltinId::IteratorFrom),
    IntrinsicMethodSpec::named("concat", StandardBuiltinId::IteratorConcat),
    IntrinsicMethodSpec::named("zip", StandardBuiltinId::IteratorZip),
    IntrinsicMethodSpec::named("zipKeyed", StandardBuiltinId::IteratorZipKeyed),
];

pub(super) const ITERATOR_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("toArray", StandardBuiltinId::IteratorPrototypeToArray),
    IntrinsicMethodSpec::named("includes", StandardBuiltinId::IteratorPrototypeIncludes),
    IntrinsicMethodSpec::named("join", StandardBuiltinId::IteratorPrototypeJoin),
    IntrinsicMethodSpec::named("forEach", StandardBuiltinId::IteratorPrototypeForEach),
    IntrinsicMethodSpec::named("every", StandardBuiltinId::IteratorPrototypeEvery),
    IntrinsicMethodSpec::named("some", StandardBuiltinId::IteratorPrototypeSome),
    IntrinsicMethodSpec::named("find", StandardBuiltinId::IteratorPrototypeFind),
    IntrinsicMethodSpec::named("reduce", StandardBuiltinId::IteratorPrototypeReduce),
    IntrinsicMethodSpec::named("map", StandardBuiltinId::IteratorPrototypeMap),
    IntrinsicMethodSpec::named("filter", StandardBuiltinId::IteratorPrototypeFilter),
    IntrinsicMethodSpec::named("flatMap", StandardBuiltinId::IteratorPrototypeFlatMap),
    IntrinsicMethodSpec::named("take", StandardBuiltinId::IteratorPrototypeTake),
    IntrinsicMethodSpec::named("drop", StandardBuiltinId::IteratorPrototypeDrop),
    IntrinsicMethodSpec::named("chunks", StandardBuiltinId::IteratorPrototypeChunks),
    IntrinsicMethodSpec::named("windows", StandardBuiltinId::IteratorPrototypeWindows),
];
