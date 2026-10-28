use super::*;

pub(super) const OBJECT_CONSTRUCTOR_INITIAL: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("groupBy", StandardBuiltinId::ObjectGroupBy),
    IntrinsicMethodSpec::named("fromEntries", StandardBuiltinId::ObjectFromEntries),
    IntrinsicMethodSpec::named("assign", StandardBuiltinId::ObjectAssign),
];

pub(super) const OBJECT_PROTOTYPE_BEFORE_ACCESSOR: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named(
        "hasOwnProperty",
        StandardBuiltinId::ObjectPrototypeHasOwnProperty,
    ),
    IntrinsicMethodSpec::named(
        "__defineGetter__",
        StandardBuiltinId::ObjectPrototypeDefineGetter,
    ),
    IntrinsicMethodSpec::named(
        "__defineSetter__",
        StandardBuiltinId::ObjectPrototypeDefineSetter,
    ),
    IntrinsicMethodSpec::named(
        "__lookupGetter__",
        StandardBuiltinId::ObjectPrototypeLookupGetter,
    ),
    IntrinsicMethodSpec::named(
        "__lookupSetter__",
        StandardBuiltinId::ObjectPrototypeLookupSetter,
    ),
];

pub(super) const OBJECT_PROTOTYPE_AFTER_ACCESSOR: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named(
        "propertyIsEnumerable",
        StandardBuiltinId::ObjectPrototypePropertyIsEnumerable,
    ),
    IntrinsicMethodSpec::named(
        "isPrototypeOf",
        StandardBuiltinId::ObjectPrototypeIsPrototypeOf,
    ),
    IntrinsicMethodSpec::named("toString", StandardBuiltinId::ObjectPrototypeToString),
    IntrinsicMethodSpec::named(
        "toLocaleString",
        StandardBuiltinId::ObjectPrototypeToLocaleString,
    ),
    IntrinsicMethodSpec::named("valueOf", StandardBuiltinId::ObjectPrototypeValueOf),
];

pub(super) const OBJECT_CONSTRUCTOR_REMAINING: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("create", StandardBuiltinId::ObjectCreate),
    IntrinsicMethodSpec::named("getPrototypeOf", StandardBuiltinId::ObjectGetPrototypeOf),
    IntrinsicMethodSpec::named("setPrototypeOf", StandardBuiltinId::ObjectSetPrototypeOf),
    IntrinsicMethodSpec::named("defineProperty", StandardBuiltinId::ObjectDefineProperty),
    IntrinsicMethodSpec::named(
        "getOwnPropertyDescriptor",
        StandardBuiltinId::ObjectGetOwnPropertyDescriptor,
    ),
    IntrinsicMethodSpec::named(
        "getOwnPropertyDescriptors",
        StandardBuiltinId::ObjectGetOwnPropertyDescriptors,
    ),
    IntrinsicMethodSpec::named(
        "getOwnPropertyNames",
        StandardBuiltinId::ObjectGetOwnPropertyNames,
    ),
    IntrinsicMethodSpec::named(
        "getOwnPropertySymbols",
        StandardBuiltinId::ObjectGetOwnPropertySymbols,
    ),
    IntrinsicMethodSpec::named("keys", StandardBuiltinId::ObjectKeys),
    IntrinsicMethodSpec::named("values", StandardBuiltinId::ObjectValues),
    IntrinsicMethodSpec::named("entries", StandardBuiltinId::ObjectEntries),
    IntrinsicMethodSpec::named("hasOwn", StandardBuiltinId::ObjectHasOwn),
    IntrinsicMethodSpec::named(
        "defineProperties",
        StandardBuiltinId::ObjectDefineProperties,
    ),
    IntrinsicMethodSpec::named("is", StandardBuiltinId::ObjectIs),
    IntrinsicMethodSpec::named("isSealed", StandardBuiltinId::ObjectIsSealed),
    IntrinsicMethodSpec::named("isFrozen", StandardBuiltinId::ObjectIsFrozen),
    IntrinsicMethodSpec::named("seal", StandardBuiltinId::ObjectSeal),
    IntrinsicMethodSpec::named("freeze", StandardBuiltinId::ObjectFreeze),
    IntrinsicMethodSpec::named("isExtensible", StandardBuiltinId::ObjectIsExtensible),
    IntrinsicMethodSpec::named(
        "preventExtensions",
        StandardBuiltinId::ObjectPreventExtensions,
    ),
];
