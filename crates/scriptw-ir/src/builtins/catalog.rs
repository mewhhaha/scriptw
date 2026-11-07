use crate::names::{
    BUILTIN_INTL_DURATION_FORMAT_FUNCTION_ID,
    BUILTIN_INTL_DURATION_FORMAT_PROTOTYPE_FORMAT_FUNCTION_ID,
    BUILTIN_INTL_DURATION_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID,
    BUILTIN_INTL_DURATION_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
    BUILTIN_INTL_DURATION_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID, INTL_DURATION_FORMAT_NAME,
};
// This registry is the sole source for StandardBuiltinId and its metadata.
//
// Row order is declaration order and therefore the derived `Ord` contract.
// `FunctionOrdinal` preserves the independent Wasm function-index order;
// `GlobalOrdinal` preserves the independent global installation order.
// Choose the ordinals and installer deliberately when adding a builtin. The
// generated const checks reject duplicate or missing ordinals and duplicate
// function IDs; the mandatory installer field makes bootstrap participation a
// compile-time choice rather than an append-only backend catch-all.
use super::*;
use crate::names::{
    BUILTIN_INTL_COLLATOR_BOUND_COMPARE_FUNCTION_ID, BUILTIN_INTL_COLLATOR_FUNCTION_ID,
    BUILTIN_INTL_COLLATOR_PROTOTYPE_COMPARE_GETTER_FUNCTION_ID,
    BUILTIN_INTL_COLLATOR_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
    BUILTIN_INTL_COLLATOR_SUPPORTED_LOCALES_OF_FUNCTION_ID, BUILTIN_INTL_LIST_FORMAT_FUNCTION_ID,
    BUILTIN_INTL_LIST_FORMAT_PROTOTYPE_FORMAT_FUNCTION_ID,
    BUILTIN_INTL_LIST_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID,
    BUILTIN_INTL_LIST_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
    BUILTIN_INTL_LIST_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID,
    BUILTIN_INTL_NUMBER_FORMAT_BOUND_FORMAT_FUNCTION_ID, BUILTIN_INTL_NUMBER_FORMAT_FUNCTION_ID,
    BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_GETTER_FUNCTION_ID,
    BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_RANGE_FUNCTION_ID,
    BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_RANGE_TO_PARTS_FUNCTION_ID,
    BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID,
    BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
    BUILTIN_INTL_NUMBER_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID,
    BUILTIN_INTL_PLURAL_RULES_FUNCTION_ID,
    BUILTIN_INTL_PLURAL_RULES_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
    BUILTIN_INTL_PLURAL_RULES_PROTOTYPE_SELECT_FUNCTION_ID,
    BUILTIN_INTL_PLURAL_RULES_PROTOTYPE_SELECT_RANGE_FUNCTION_ID,
    BUILTIN_INTL_PLURAL_RULES_SUPPORTED_LOCALES_OF_FUNCTION_ID,
    BUILTIN_INTL_SUPPORTED_VALUES_OF_FUNCTION_ID, INTL_COLLATOR_NAME, INTL_LIST_FORMAT_NAME,
    INTL_NUMBER_FORMAT_NAME, INTL_PLURAL_RULES_NAME,
};
use crate::names::{
    BUILTIN_INTL_DISPLAY_NAMES_FUNCTION_ID, BUILTIN_INTL_DISPLAY_NAMES_PROTOTYPE_OF_FUNCTION_ID,
    BUILTIN_INTL_DISPLAY_NAMES_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
    BUILTIN_INTL_DISPLAY_NAMES_SUPPORTED_LOCALES_OF_FUNCTION_ID,
    BUILTIN_INTL_RELATIVE_TIME_FORMAT_FUNCTION_ID,
    BUILTIN_INTL_RELATIVE_TIME_FORMAT_PROTOTYPE_FORMAT_FUNCTION_ID,
    BUILTIN_INTL_RELATIVE_TIME_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID,
    BUILTIN_INTL_RELATIVE_TIME_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
    BUILTIN_INTL_RELATIVE_TIME_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID, INTL_DISPLAY_NAMES_NAME,
    INTL_RELATIVE_TIME_FORMAT_NAME,
};
use crate::names::{
    BUILTIN_INTL_SEGMENTER_FUNCTION_ID,
    BUILTIN_INTL_SEGMENTER_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
    BUILTIN_INTL_SEGMENTER_PROTOTYPE_SEGMENT_FUNCTION_ID,
    BUILTIN_INTL_SEGMENTER_SUPPORTED_LOCALES_OF_FUNCTION_ID,
    BUILTIN_INTL_SEGMENTS_PROTOTYPE_CONTAINING_FUNCTION_ID,
    BUILTIN_INTL_SEGMENTS_PROTOTYPE_ITERATOR_FUNCTION_ID,
    BUILTIN_INTL_SEGMENT_ITERATOR_PROTOTYPE_NEXT_FUNCTION_ID, INTL_SEGMENTER_NAME,
};

/// The family-specific realm installer, if any, run after a builtin's common
/// function/prototype initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandardBuiltinInstaller {
    ShadowRealm,
    None,
    Function,
    Promise,
    Map,
    WeakMap,
    WeakSet,
    WeakRef,
    FinalizationRegistry,
    AsyncDisposableStack,
    DisposableStack,
    TextEncoder,
    TextDecoder,
    Set,
    Object,
    Proxy,
    RegExp,
    Iterator,
    Array,
    String,
    ArrayBuffer,
    DataView,
    Uint8Array,
    TemporalInstant,
    TemporalZonedDateTime,
    TemporalPlainDate,
    TemporalDuration,
    TemporalPlainTime,
    TemporalPlainDateTime,
    TemporalPlainYearMonth,
    TemporalPlainMonthDay,
    IntlLocale,
    IntlDateTimeFormat,
    IntlNumberFormat,
    IntlPluralRules,
    IntlListFormat,
    IntlCollator,
    IntlDisplayNames,
    IntlRelativeTimeFormat,
    IntlSegmenter,
    IntlDurationFormat,
    Date,
    Error,
    BigInt,
    Symbol,
    Number,
    Boolean,
}

standard_builtin_catalog! {
    QueueMicrotask {
        function: FunctionOrdinal(911) => "$builtin.queueMicrotask",
        global: GlobalOrdinal(65),
        global_name: "queueMicrotask",
        debug: "queueMicrotask",
        flags: [],
        installer: None,
        native: "queueMicrotask",
    }
    SetTimeout {
        function: FunctionOrdinal(912) => "$builtin.setTimeout",
        global: GlobalOrdinal(66),
        global_name: "setTimeout",
        debug: "setTimeout",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setTimeout",
    }
    ClearTimeout {
        function: FunctionOrdinal(913) => "$builtin.clearTimeout",
        global: GlobalOrdinal(67),
        global_name: "clearTimeout",
        debug: "clearTimeout",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "clearTimeout",
    }
    TextEncoderConstructor {
        function: FunctionOrdinal(914) => "$builtin.TextEncoderConstructor",
        global: GlobalOrdinal(55),
        global_name: "TextEncoder",
        debug: "TextEncoderConstructor",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: TextEncoder,
        native: "TextEncoder",
    }
    TextEncoderPrototypeEncode {
        function: FunctionOrdinal(915) => "$builtin.TextEncoderPrototypeEncode",
        debug: "TextEncoderPrototypeEncode",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "encode",
    }
    TextEncoderPrototypeEncodeInto {
        function: FunctionOrdinal(916) => "$builtin.TextEncoderPrototypeEncodeInto",
        debug: "TextEncoderPrototypeEncodeInto",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "encodeInto",
    }
    TextEncoderPrototypeEncodingGetter {
        function: FunctionOrdinal(917) => "$builtin.TextEncoderPrototypeEncodingGetter",
        debug: "TextEncoderPrototypeEncodingGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get encoding",
    }
    TextDecoderConstructor {
        function: FunctionOrdinal(918) => "$builtin.TextDecoderConstructor",
        global: GlobalOrdinal(56),
        global_name: "TextDecoder",
        debug: "TextDecoderConstructor",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: TextDecoder,
        native: "TextDecoder",
    }
    TextDecoderPrototypeDecode {
        function: FunctionOrdinal(919) => "$builtin.TextDecoderPrototypeDecode",
        debug: "TextDecoderPrototypeDecode",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "decode",
    }
    TextDecoderPrototypeEncodingGetter {
        function: FunctionOrdinal(920) => "$builtin.TextDecoderPrototypeEncodingGetter",
        debug: "TextDecoderPrototypeEncodingGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get encoding",
    }
    TextDecoderPrototypeFatalGetter {
        function: FunctionOrdinal(921) => "$builtin.TextDecoderPrototypeFatalGetter",
        debug: "TextDecoderPrototypeFatalGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get fatal",
    }
    TextDecoderPrototypeIgnoreBOMGetter {
        function: FunctionOrdinal(922) => "$builtin.TextDecoderPrototypeIgnoreBOMGetter",
        debug: "TextDecoderPrototypeIgnoreBOMGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get ignoreBOM",
    }
    FunctionConstructor {
        function: FunctionOrdinal(0) => BUILTIN_FUNCTION_FUNCTION_ID,
        global: GlobalOrdinal(0),
        global_name: FUNCTION_NAME,
        debug: FUNCTION_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: Function,
        native: FUNCTION_NAME,
    }
    FunctionPrototypeCall {
        function: FunctionOrdinal(1) => BUILTIN_FUNCTION_PROTOTYPE_CALL_FUNCTION_ID,
        debug: "Function.prototype.call",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "call",
    }
    FunctionPrototypeApply {
        function: FunctionOrdinal(2) => BUILTIN_FUNCTION_PROTOTYPE_APPLY_FUNCTION_ID,
        debug: "Function.prototype.apply",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "apply",
    }
    FunctionPrototypeBind {
        function: FunctionOrdinal(3) => BUILTIN_FUNCTION_PROTOTYPE_BIND_FUNCTION_ID,
        debug: "Function.prototype.bind",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "bind",
    }
    FunctionPrototypeToString {
        function: FunctionOrdinal(4) => BUILTIN_FUNCTION_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Function.prototype.toString",
        flags: [],
        installer: None,
        native: "toString",
    }
    EvalFunction {
        function: FunctionOrdinal(5) => BUILTIN_EVAL_FUNCTION_ID,
        global: GlobalOrdinal(1),
        global_name: "eval",
        debug: "eval",
        flags: [],
        installer: None,
        native: "eval",
    }
    ObjectConstructor {
        function: FunctionOrdinal(6) => BUILTIN_OBJECT_FUNCTION_ID,
        global: GlobalOrdinal(2),
        global_name: OBJECT_NAME,
        debug: OBJECT_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: Object,
        native: OBJECT_NAME,
    }
    ObjectGroupBy {
        function: FunctionOrdinal(7) => BUILTIN_OBJECT_GROUP_BY_FUNCTION_ID,
        debug: "Object.groupBy",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "groupBy",
    }
    ObjectFromEntries {
        function: FunctionOrdinal(8) => BUILTIN_OBJECT_FROM_ENTRIES_FUNCTION_ID,
        debug: "Object.fromEntries",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "fromEntries",
    }
    ObjectAssign {
        function: FunctionOrdinal(9) => BUILTIN_OBJECT_ASSIGN_FUNCTION_ID,
        debug: "Object.assign",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "assign",
    }
    ObjectCreate {
        function: FunctionOrdinal(10) => BUILTIN_OBJECT_CREATE_FUNCTION_ID,
        debug: "Object.create",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "create",
    }
    ObjectGetPrototypeOf {
        function: FunctionOrdinal(11) => BUILTIN_OBJECT_GET_PROTOTYPE_OF_FUNCTION_ID,
        debug: "Object.getPrototypeOf",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getPrototypeOf",
    }
    ObjectSetPrototypeOf {
        function: FunctionOrdinal(12) => BUILTIN_OBJECT_SET_PROTOTYPE_OF_FUNCTION_ID,
        debug: "Object.setPrototypeOf",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setPrototypeOf",
    }
    ObjectDefineProperty {
        function: FunctionOrdinal(13) => BUILTIN_OBJECT_DEFINE_PROPERTY_FUNCTION_ID,
        debug: "Object.defineProperty",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "defineProperty",
    }
    ObjectDefineProperties {
        function: FunctionOrdinal(14) => BUILTIN_OBJECT_DEFINE_PROPERTIES_FUNCTION_ID,
        debug: "Object.defineProperties",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "defineProperties",
    }
    ObjectGetOwnPropertyDescriptor {
        function: FunctionOrdinal(15) => BUILTIN_OBJECT_GET_OWN_PROPERTY_DESCRIPTOR_FUNCTION_ID,
        debug: "Object.getOwnPropertyDescriptor",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getOwnPropertyDescriptor",
    }
    ObjectGetOwnPropertyDescriptors {
        function: FunctionOrdinal(16) => BUILTIN_OBJECT_GET_OWN_PROPERTY_DESCRIPTORS_FUNCTION_ID,
        debug: "Object.getOwnPropertyDescriptors",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getOwnPropertyDescriptors",
    }
    ObjectGetOwnPropertyNames {
        function: FunctionOrdinal(17) => BUILTIN_OBJECT_GET_OWN_PROPERTY_NAMES_FUNCTION_ID,
        debug: "Object.getOwnPropertyNames",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getOwnPropertyNames",
    }
    ObjectGetOwnPropertySymbols {
        function: FunctionOrdinal(18) => BUILTIN_OBJECT_GET_OWN_PROPERTY_SYMBOLS_FUNCTION_ID,
        debug: "Object.getOwnPropertySymbols",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getOwnPropertySymbols",
    }
    ObjectKeys {
        function: FunctionOrdinal(19) => BUILTIN_OBJECT_KEYS_FUNCTION_ID,
        debug: "Object.keys",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "keys",
    }
    ObjectValues {
        function: FunctionOrdinal(20) => BUILTIN_OBJECT_VALUES_FUNCTION_ID,
        debug: "Object.values",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "values",
    }
    ObjectEntries {
        function: FunctionOrdinal(21) => BUILTIN_OBJECT_ENTRIES_FUNCTION_ID,
        debug: "Object.entries",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "entries",
    }
    ObjectHasOwn {
        function: FunctionOrdinal(22) => BUILTIN_OBJECT_HAS_OWN_FUNCTION_ID,
        debug: "Object.hasOwn",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "hasOwn",
    }
    ObjectIs {
        function: FunctionOrdinal(23) => BUILTIN_OBJECT_IS_FUNCTION_ID,
        debug: "Object.is",
        flags: [STATIC_METHOD],
        installer: None,
        native: "is",
    }
    ObjectIsSealed {
        function: FunctionOrdinal(24) => BUILTIN_OBJECT_IS_SEALED_FUNCTION_ID,
        debug: "Object.isSealed",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "isSealed",
    }
    ObjectIsFrozen {
        function: FunctionOrdinal(25) => BUILTIN_OBJECT_IS_FROZEN_FUNCTION_ID,
        debug: "Object.isFrozen",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "isFrozen",
    }
    ObjectSeal {
        function: FunctionOrdinal(26) => BUILTIN_OBJECT_SEAL_FUNCTION_ID,
        debug: "Object.seal",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "seal",
    }
    ObjectFreeze {
        function: FunctionOrdinal(27) => BUILTIN_OBJECT_FREEZE_FUNCTION_ID,
        debug: "Object.freeze",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "freeze",
    }
    ObjectIsExtensible {
        function: FunctionOrdinal(28) => BUILTIN_OBJECT_IS_EXTENSIBLE_FUNCTION_ID,
        debug: "Object.isExtensible",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "isExtensible",
    }
    ObjectPreventExtensions {
        function: FunctionOrdinal(29) => BUILTIN_OBJECT_PREVENT_EXTENSIONS_FUNCTION_ID,
        debug: "Object.preventExtensions",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "preventExtensions",
    }
    ObjectPrototypeHasOwnProperty {
        function: FunctionOrdinal(30) => BUILTIN_OBJECT_PROTOTYPE_HAS_OWN_PROPERTY_FUNCTION_ID,
        debug: "Object.prototype.hasOwnProperty",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "hasOwnProperty",
    }
    ObjectPrototypeDefineGetter {
        function: FunctionOrdinal(824) => "$builtin.Object.prototype.__defineGetter__",
        debug: "Object.prototype.__defineGetter__",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "__defineGetter__",
    }
    ObjectPrototypeDefineSetter {
        function: FunctionOrdinal(825) => "$builtin.Object.prototype.__defineSetter__",
        debug: "Object.prototype.__defineSetter__",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "__defineSetter__",
    }
    ObjectPrototypeLookupGetter {
        function: FunctionOrdinal(31) => BUILTIN_OBJECT_PROTOTYPE_LOOKUP_GETTER_FUNCTION_ID,
        debug: "Object.prototype.__lookupGetter__",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "__lookupGetter__",
    }
    ObjectPrototypeLookupSetter {
        function: FunctionOrdinal(32) => BUILTIN_OBJECT_PROTOTYPE_LOOKUP_SETTER_FUNCTION_ID,
        debug: "Object.prototype.__lookupSetter__",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "__lookupSetter__",
    }
    ObjectPrototypeProtoGetter {
        function: FunctionOrdinal(33) => BUILTIN_OBJECT_PROTOTYPE_PROTO_GETTER_FUNCTION_ID,
        debug: "get Object.prototype.__proto__",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get __proto__",
    }
    ObjectPrototypeProtoSetter {
        function: FunctionOrdinal(34) => BUILTIN_OBJECT_PROTOTYPE_PROTO_SETTER_FUNCTION_ID,
        debug: "set Object.prototype.__proto__",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "set __proto__",
    }
    ObjectPrototypePropertyIsEnumerable {
        function: FunctionOrdinal(35) => BUILTIN_OBJECT_PROTOTYPE_PROPERTY_IS_ENUMERABLE_FUNCTION_ID,
        debug: "Object.prototype.propertyIsEnumerable",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "propertyIsEnumerable",
    }
    ObjectPrototypeIsPrototypeOf {
        function: FunctionOrdinal(36) => BUILTIN_OBJECT_PROTOTYPE_IS_PROTOTYPE_OF_FUNCTION_ID,
        debug: "Object.prototype.isPrototypeOf",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "isPrototypeOf",
    }
    ObjectPrototypeToString {
        function: FunctionOrdinal(37) => BUILTIN_OBJECT_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Object.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toString",
    }
    ObjectPrototypeToLocaleString {
        function: FunctionOrdinal(38) => BUILTIN_OBJECT_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID,
        debug: "Object.prototype.toLocaleString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toLocaleString",
    }
    ObjectPrototypeValueOf {
        function: FunctionOrdinal(39) => BUILTIN_OBJECT_PROTOTYPE_VALUE_OF_FUNCTION_ID,
        debug: "Object.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    ProxyConstructor {
        function: FunctionOrdinal(40) => BUILTIN_PROXY_FUNCTION_ID,
        global: GlobalOrdinal(3),
        global_name: PROXY_NAME,
        debug: PROXY_NAME,
        flags: [CONSTRUCTABLE],
        installer: Proxy,
        native: PROXY_NAME,
    }
    ProxyRevocable {
        function: FunctionOrdinal(41) => BUILTIN_PROXY_REVOCABLE_FUNCTION_ID,
        debug: "Proxy.revocable",
        flags: [STATIC_METHOD],
        installer: None,
        native: "revocable",
    }
    ProxyRevoke {
        function: FunctionOrdinal(42) => BUILTIN_PROXY_REVOKE_FUNCTION_ID,
        debug: "[[ProxyRevoke]]",
        flags: [],
        installer: None,
        native: "revoke",
    }
    ReflectConstruct {
        function: FunctionOrdinal(43) => BUILTIN_REFLECT_CONSTRUCT_FUNCTION_ID,
        debug: "Reflect.construct",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "construct",
    }
    ReflectApply {
        function: FunctionOrdinal(44) => BUILTIN_REFLECT_APPLY_FUNCTION_ID,
        debug: "Reflect.apply",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "apply",
    }
    ReflectGet {
        function: FunctionOrdinal(45) => BUILTIN_REFLECT_GET_FUNCTION_ID,
        debug: "Reflect.get",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get",
    }
    ReflectGetPrototypeOf {
        function: FunctionOrdinal(46) => BUILTIN_REFLECT_GET_PROTOTYPE_OF_FUNCTION_ID,
        debug: "Reflect.getPrototypeOf",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getPrototypeOf",
    }
    ReflectGetOwnPropertyDescriptor {
        function: FunctionOrdinal(47) => BUILTIN_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR_FUNCTION_ID,
        debug: "Reflect.getOwnPropertyDescriptor",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getOwnPropertyDescriptor",
    }
    ReflectSet {
        function: FunctionOrdinal(48) => BUILTIN_REFLECT_SET_FUNCTION_ID,
        debug: "Reflect.set",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "set",
    }
    ReflectHas {
        function: FunctionOrdinal(49) => BUILTIN_REFLECT_HAS_FUNCTION_ID,
        debug: "Reflect.has",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "has",
    }
    ReflectDefineProperty {
        function: FunctionOrdinal(50) => BUILTIN_REFLECT_DEFINE_PROPERTY_FUNCTION_ID,
        debug: "Reflect.defineProperty",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "defineProperty",
    }
    ReflectDeleteProperty {
        function: FunctionOrdinal(51) => BUILTIN_REFLECT_DELETE_PROPERTY_FUNCTION_ID,
        debug: "Reflect.deleteProperty",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "deleteProperty",
    }
    ReflectIsExtensible {
        function: FunctionOrdinal(52) => BUILTIN_REFLECT_IS_EXTENSIBLE_FUNCTION_ID,
        debug: "Reflect.isExtensible",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "isExtensible",
    }
    ReflectPreventExtensions {
        function: FunctionOrdinal(53) => BUILTIN_REFLECT_PREVENT_EXTENSIONS_FUNCTION_ID,
        debug: "Reflect.preventExtensions",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "preventExtensions",
    }
    ReflectSetPrototypeOf {
        function: FunctionOrdinal(54) => BUILTIN_REFLECT_SET_PROTOTYPE_OF_FUNCTION_ID,
        debug: "Reflect.setPrototypeOf",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setPrototypeOf",
    }
    ReflectOwnKeys {
        function: FunctionOrdinal(55) => BUILTIN_REFLECT_OWN_KEYS_FUNCTION_ID,
        debug: "Reflect.ownKeys",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "ownKeys",
    }
    ArrayConstructor {
        function: FunctionOrdinal(56) => BUILTIN_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(5),
        global_name: ARRAY_NAME,
        debug: ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: Array,
        native: ARRAY_NAME,
    }
    ArrayFrom {
        function: FunctionOrdinal(57) => BUILTIN_ARRAY_FROM_FUNCTION_ID,
        debug: "Array.from",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "from",
    }
    ArrayFromAsync {
        function: FunctionOrdinal(58) => BUILTIN_ARRAY_FROM_ASYNC_FUNCTION_ID,
        debug: "Array.fromAsync",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "fromAsync",
    }
    ArrayFromAsyncFulfilled {
        function: FunctionOrdinal(59) => BUILTIN_ARRAY_FROM_ASYNC_FULFILLED_FUNCTION_ID,
        debug: "Array.fromAsync Fulfilled Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    ArrayFromAsyncRejected {
        function: FunctionOrdinal(60) => BUILTIN_ARRAY_FROM_ASYNC_REJECTED_FUNCTION_ID,
        debug: "Array.fromAsync Rejected Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    ArrayOf {
        function: FunctionOrdinal(61) => BUILTIN_ARRAY_OF_FUNCTION_ID,
        debug: "Array.of",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "of",
    }
    ArrayIsArray {
        function: FunctionOrdinal(62) => BUILTIN_ARRAY_IS_ARRAY_FUNCTION_ID,
        debug: "Array.isArray",
        flags: [STATIC_METHOD],
        installer: None,
        native: "isArray",
    }
    ArraySpeciesGetter {
        function: FunctionOrdinal(63) => BUILTIN_ARRAY_SPECIES_GETTER_FUNCTION_ID,
        debug: "get Array [Symbol.species]",
        flags: [],
        installer: None,
        native: "get [Symbol.species]",
    }
    ArrayPrototypeConcat {
        function: FunctionOrdinal(64) => BUILTIN_ARRAY_PROTOTYPE_CONCAT_FUNCTION_ID,
        debug: "Array.prototype.concat",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "concat",
    }
    ArrayPrototypeJoin {
        function: FunctionOrdinal(65) => BUILTIN_ARRAY_PROTOTYPE_JOIN_FUNCTION_ID,
        debug: "Array.prototype.join",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "join",
    }
    ArrayPrototypeSlice {
        function: FunctionOrdinal(66) => BUILTIN_ARRAY_PROTOTYPE_SLICE_FUNCTION_ID,
        debug: "Array.prototype.slice",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "slice",
    }
    ArrayPrototypeSplice {
        function: FunctionOrdinal(67) => BUILTIN_ARRAY_PROTOTYPE_SPLICE_FUNCTION_ID,
        debug: "Array.prototype.splice",
        flags: [INDEXED_RECEIVER_MUTATION, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "splice",
    }
    ArrayPrototypeSort {
        function: FunctionOrdinal(68) => BUILTIN_ARRAY_PROTOTYPE_SORT_FUNCTION_ID,
        debug: "Array.prototype.sort",
        flags: [SYNCHRONOUS_USER_CODE, INDEXED_RECEIVER_MUTATION],
        installer: None,
        native: "sort",
    }
    ArrayPrototypeToLocaleString {
        function: FunctionOrdinal(69) => BUILTIN_ARRAY_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID,
        debug: "Array.prototype.toLocaleString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toLocaleString",
    }
    ArrayPrototypeFlat {
        function: FunctionOrdinal(70) => BUILTIN_ARRAY_PROTOTYPE_FLAT_FUNCTION_ID,
        debug: "Array.prototype.flat",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "flat",
    }
    ArrayPrototypeFlatMap {
        function: FunctionOrdinal(71) => BUILTIN_ARRAY_PROTOTYPE_FLAT_MAP_FUNCTION_ID,
        debug: "Array.prototype.flatMap",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "flatMap",
    }
    ArrayPrototypeAt {
        function: FunctionOrdinal(72) => BUILTIN_ARRAY_PROTOTYPE_AT_FUNCTION_ID,
        debug: "Array.prototype.at",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "at",
    }
    ArrayPrototypeToReversed {
        function: FunctionOrdinal(73) => BUILTIN_ARRAY_PROTOTYPE_TO_REVERSED_FUNCTION_ID,
        debug: "Array.prototype.toReversed",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toReversed",
    }
    ArrayPrototypeToSpliced {
        function: FunctionOrdinal(74) => BUILTIN_ARRAY_PROTOTYPE_TO_SPLICED_FUNCTION_ID,
        debug: "Array.prototype.toSpliced",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toSpliced",
    }
    ArrayPrototypeToSorted {
        function: FunctionOrdinal(75) => BUILTIN_ARRAY_PROTOTYPE_TO_SORTED_FUNCTION_ID,
        debug: "Array.prototype.toSorted",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toSorted",
    }
    ArrayPrototypeWith {
        function: FunctionOrdinal(76) => BUILTIN_ARRAY_PROTOTYPE_WITH_FUNCTION_ID,
        debug: "Array.prototype.with",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "with",
    }
    ArrayPrototypeReverse {
        function: FunctionOrdinal(77) => BUILTIN_ARRAY_PROTOTYPE_REVERSE_FUNCTION_ID,
        debug: "Array.prototype.reverse",
        flags: [INDEXED_RECEIVER_MUTATION, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "reverse",
    }
    ArrayPrototypeCopyWithin {
        function: FunctionOrdinal(78) => BUILTIN_ARRAY_PROTOTYPE_COPY_WITHIN_FUNCTION_ID,
        debug: "Array.prototype.copyWithin",
        flags: [INDEXED_RECEIVER_MUTATION, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "copyWithin",
    }
    ArrayPrototypeIncludes {
        function: FunctionOrdinal(79) => BUILTIN_ARRAY_PROTOTYPE_INCLUDES_FUNCTION_ID,
        debug: "Array.prototype.includes",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "includes",
    }
    ArrayPrototypeIndexOf {
        function: FunctionOrdinal(80) => BUILTIN_ARRAY_PROTOTYPE_INDEX_OF_FUNCTION_ID,
        debug: "Array.prototype.indexOf",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "indexOf",
    }
    ArrayPrototypeLastIndexOf {
        function: FunctionOrdinal(81) => BUILTIN_ARRAY_PROTOTYPE_LAST_INDEX_OF_FUNCTION_ID,
        debug: "Array.prototype.lastIndexOf",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "lastIndexOf",
    }
    ArrayPrototypeFind {
        function: FunctionOrdinal(82) => BUILTIN_ARRAY_PROTOTYPE_FIND_FUNCTION_ID,
        debug: "Array.prototype.find",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "find",
    }
    ArrayPrototypeFindIndex {
        function: FunctionOrdinal(83) => BUILTIN_ARRAY_PROTOTYPE_FIND_INDEX_FUNCTION_ID,
        debug: "Array.prototype.findIndex",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "findIndex",
    }
    ArrayPrototypeFindLast {
        function: FunctionOrdinal(84) => BUILTIN_ARRAY_PROTOTYPE_FIND_LAST_FUNCTION_ID,
        debug: "Array.prototype.findLast",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "findLast",
    }
    ArrayPrototypeFindLastIndex {
        function: FunctionOrdinal(85) => BUILTIN_ARRAY_PROTOTYPE_FIND_LAST_INDEX_FUNCTION_ID,
        debug: "Array.prototype.findLastIndex",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "findLastIndex",
    }
    ArrayPrototypeEvery {
        function: FunctionOrdinal(86) => BUILTIN_ARRAY_PROTOTYPE_EVERY_FUNCTION_ID,
        debug: "Array.prototype.every",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "every",
    }
    ArrayPrototypeSome {
        function: FunctionOrdinal(87) => BUILTIN_ARRAY_PROTOTYPE_SOME_FUNCTION_ID,
        debug: "Array.prototype.some",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "some",
    }
    ArrayPrototypeForEach {
        function: FunctionOrdinal(88) => BUILTIN_ARRAY_PROTOTYPE_FOR_EACH_FUNCTION_ID,
        debug: "Array.prototype.forEach",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "forEach",
    }
    ArrayPrototypeFilter {
        function: FunctionOrdinal(89) => BUILTIN_ARRAY_PROTOTYPE_FILTER_FUNCTION_ID,
        debug: "Array.prototype.filter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "filter",
    }
    ArrayPrototypeMap {
        function: FunctionOrdinal(90) => BUILTIN_ARRAY_PROTOTYPE_MAP_FUNCTION_ID,
        debug: "Array.prototype.map",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "map",
    }
    ArrayPrototypeReduce {
        function: FunctionOrdinal(91) => BUILTIN_ARRAY_PROTOTYPE_REDUCE_FUNCTION_ID,
        debug: "Array.prototype.reduce",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "reduce",
    }
    ArrayPrototypeReduceRight {
        function: FunctionOrdinal(92) => BUILTIN_ARRAY_PROTOTYPE_REDUCE_RIGHT_FUNCTION_ID,
        debug: "Array.prototype.reduceRight",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "reduceRight",
    }
    ArrayPrototypePop {
        function: FunctionOrdinal(93) => BUILTIN_ARRAY_PROTOTYPE_POP_FUNCTION_ID,
        debug: "Array.prototype.pop",
        flags: [INDEXED_RECEIVER_MUTATION, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "pop",
    }
    ArrayPrototypePush {
        function: FunctionOrdinal(94) => BUILTIN_ARRAY_PROTOTYPE_PUSH_FUNCTION_ID,
        debug: "Array.prototype.push",
        flags: [INDEXED_RECEIVER_MUTATION, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "push",
    }
    ArrayPrototypeShift {
        function: FunctionOrdinal(95) => BUILTIN_ARRAY_PROTOTYPE_SHIFT_FUNCTION_ID,
        debug: "Array.prototype.shift",
        flags: [INDEXED_RECEIVER_MUTATION, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "shift",
    }
    ArrayPrototypeUnshift {
        function: FunctionOrdinal(96) => BUILTIN_ARRAY_PROTOTYPE_UNSHIFT_FUNCTION_ID,
        debug: "Array.prototype.unshift",
        flags: [INDEXED_RECEIVER_MUTATION, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "unshift",
    }
    ArrayPrototypeFill {
        function: FunctionOrdinal(97) => BUILTIN_ARRAY_PROTOTYPE_FILL_FUNCTION_ID,
        debug: "Array.prototype.fill",
        flags: [INDEXED_RECEIVER_MUTATION, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "fill",
    }
    ArrayPrototypeKeys {
        function: FunctionOrdinal(98) => BUILTIN_ARRAY_PROTOTYPE_KEYS_FUNCTION_ID,
        debug: "Array.prototype.keys",
        flags: [],
        installer: None,
        native: "keys",
    }
    ArrayPrototypeEntries {
        function: FunctionOrdinal(99) => BUILTIN_ARRAY_PROTOTYPE_ENTRIES_FUNCTION_ID,
        debug: "Array.prototype.entries",
        flags: [],
        installer: None,
        native: "entries",
    }
    ArrayPrototypeValues {
        function: FunctionOrdinal(100) => BUILTIN_ARRAY_PROTOTYPE_VALUES_FUNCTION_ID,
        debug: "Array.prototype.values",
        flags: [],
        installer: None,
        native: "values",
    }
    ArrayIteratorNext {
        function: FunctionOrdinal(101) => BUILTIN_ARRAY_ITERATOR_NEXT_FUNCTION_ID,
        debug: "Array Iterator.prototype.next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    ArrayIteratorIdentity {
        function: FunctionOrdinal(102) => BUILTIN_ARRAY_ITERATOR_IDENTITY_FUNCTION_ID,
        debug: "Array Iterator.prototype [Symbol.iterator]",
        flags: [],
        installer: None,
        native: "[Symbol.iterator]",
    }
    StringIteratorNext {
        function: FunctionOrdinal(103) => BUILTIN_STRING_ITERATOR_NEXT_FUNCTION_ID,
        debug: "String Iterator.prototype.next",
        flags: [],
        installer: None,
        native: "next",
    }
    GeneratorPrototypeNext {
        function: FunctionOrdinal(104) => BUILTIN_GENERATOR_PROTOTYPE_NEXT_FUNCTION_ID,
        debug: "Generator.prototype.next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    GeneratorPrototypeReturn {
        function: FunctionOrdinal(105) => BUILTIN_GENERATOR_PROTOTYPE_RETURN_FUNCTION_ID,
        debug: "Generator.prototype.return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    GeneratorPrototypeThrow {
        function: FunctionOrdinal(106) => BUILTIN_GENERATOR_PROTOTYPE_THROW_FUNCTION_ID,
        debug: "Generator.prototype.throw",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "throw",
    }
    AsyncGeneratorPrototypeNext {
        function: FunctionOrdinal(107) => BUILTIN_ASYNC_GENERATOR_PROTOTYPE_NEXT_FUNCTION_ID,
        debug: "AsyncGenerator.prototype.next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    AsyncGeneratorPrototypeReturn {
        function: FunctionOrdinal(108) => BUILTIN_ASYNC_GENERATOR_PROTOTYPE_RETURN_FUNCTION_ID,
        debug: "AsyncGenerator.prototype.return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    AsyncGeneratorPrototypeThrow {
        function: FunctionOrdinal(109) => BUILTIN_ASYNC_GENERATOR_PROTOTYPE_THROW_FUNCTION_ID,
        debug: "AsyncGenerator.prototype.throw",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "throw",
    }
    AsyncIteratorPrototypeAsyncDispose {
        function: FunctionOrdinal(110) => BUILTIN_ASYNC_ITERATOR_PROTOTYPE_ASYNC_DISPOSE_FUNCTION_ID,
        debug: "AsyncIterator.prototype[Symbol.asyncDispose]",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "[Symbol.asyncDispose]",
    }
    AsyncIteratorPrototypeAsyncDisposeFulfilled {
        function: FunctionOrdinal(111) => BUILTIN_ASYNC_ITERATOR_PROTOTYPE_ASYNC_DISPOSE_FULFILLED_FUNCTION_ID,
        debug: "AsyncIterator asyncDispose Fulfilled Function",
        flags: [],
        installer: None,
        native: "",
    }
    IteratorConstructor {
        function: FunctionOrdinal(112) => BUILTIN_ITERATOR_FUNCTION_ID,
        global: GlobalOrdinal(4),
        global_name: "Iterator",
        debug: "Iterator",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: Iterator,
        native: "Iterator",
    }
    IteratorFrom {
        function: FunctionOrdinal(113) => BUILTIN_ITERATOR_FROM_FUNCTION_ID,
        debug: "Iterator.from",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "from",
    }
    IteratorConcat {
        function: FunctionOrdinal(114) => BUILTIN_ITERATOR_CONCAT_FUNCTION_ID,
        debug: "Iterator.concat",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "concat",
    }
    IteratorConcatNext {
        function: FunctionOrdinal(115) => BUILTIN_ITERATOR_CONCAT_NEXT_FUNCTION_ID,
        debug: "Iterator concat helper next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    IteratorConcatReturn {
        function: FunctionOrdinal(116) => BUILTIN_ITERATOR_CONCAT_RETURN_FUNCTION_ID,
        debug: "Iterator concat helper return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    IteratorZip {
        function: FunctionOrdinal(117) => BUILTIN_ITERATOR_ZIP_FUNCTION_ID,
        debug: "Iterator.zip",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "zip",
    }
    IteratorZipKeyed {
        function: FunctionOrdinal(118) => BUILTIN_ITERATOR_ZIP_KEYED_FUNCTION_ID,
        debug: "Iterator.zipKeyed",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "zipKeyed",
    }
    IteratorZipNext {
        function: FunctionOrdinal(119) => BUILTIN_ITERATOR_ZIP_NEXT_FUNCTION_ID,
        debug: "Iterator zip helper next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    IteratorZipReturn {
        function: FunctionOrdinal(120) => BUILTIN_ITERATOR_ZIP_RETURN_FUNCTION_ID,
        debug: "Iterator zip helper return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    IteratorHelperNext {
        function: FunctionOrdinal(121) => BUILTIN_ITERATOR_HELPER_NEXT_FUNCTION_ID,
        debug: "%IteratorHelperPrototype%.next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    IteratorHelperReturn {
        function: FunctionOrdinal(122) => BUILTIN_ITERATOR_HELPER_RETURN_FUNCTION_ID,
        debug: "%IteratorHelperPrototype%.return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    IteratorPrototypeToArray {
        function: FunctionOrdinal(123) => BUILTIN_ITERATOR_PROTOTYPE_TO_ARRAY_FUNCTION_ID,
        debug: "Iterator.prototype.toArray",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toArray",
    }
    IteratorPrototypeForEach {
        function: FunctionOrdinal(124) => BUILTIN_ITERATOR_PROTOTYPE_FOR_EACH_FUNCTION_ID,
        debug: "Iterator.prototype.forEach",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "forEach",
    }
    IteratorPrototypeEvery {
        function: FunctionOrdinal(125) => BUILTIN_ITERATOR_PROTOTYPE_EVERY_FUNCTION_ID,
        debug: "Iterator.prototype.every",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "every",
    }
    IteratorPrototypeSome {
        function: FunctionOrdinal(126) => BUILTIN_ITERATOR_PROTOTYPE_SOME_FUNCTION_ID,
        debug: "Iterator.prototype.some",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "some",
    }
    IteratorPrototypeFind {
        function: FunctionOrdinal(127) => BUILTIN_ITERATOR_PROTOTYPE_FIND_FUNCTION_ID,
        debug: "Iterator.prototype.find",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "find",
    }
    IteratorPrototypeReduce {
        function: FunctionOrdinal(128) => BUILTIN_ITERATOR_PROTOTYPE_REDUCE_FUNCTION_ID,
        debug: "Iterator.prototype.reduce",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "reduce",
    }
    IteratorPrototypeMap {
        function: FunctionOrdinal(129) => BUILTIN_ITERATOR_PROTOTYPE_MAP_FUNCTION_ID,
        debug: "Iterator.prototype.map",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "map",
    }
    IteratorMapNext {
        function: FunctionOrdinal(130) => BUILTIN_ITERATOR_MAP_NEXT_FUNCTION_ID,
        debug: "Iterator map helper next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    IteratorMapReturn {
        function: FunctionOrdinal(131) => BUILTIN_ITERATOR_MAP_RETURN_FUNCTION_ID,
        debug: "Iterator map helper return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    IteratorPrototypeFilter {
        function: FunctionOrdinal(132) => BUILTIN_ITERATOR_PROTOTYPE_FILTER_FUNCTION_ID,
        debug: "Iterator.prototype.filter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "filter",
    }
    IteratorFilterNext {
        function: FunctionOrdinal(133) => BUILTIN_ITERATOR_FILTER_NEXT_FUNCTION_ID,
        debug: "Iterator filter helper next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    IteratorFilterReturn {
        function: FunctionOrdinal(134) => BUILTIN_ITERATOR_FILTER_RETURN_FUNCTION_ID,
        debug: "Iterator filter helper return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    IteratorPrototypeFlatMap {
        function: FunctionOrdinal(135) => BUILTIN_ITERATOR_PROTOTYPE_FLAT_MAP_FUNCTION_ID,
        debug: "Iterator.prototype.flatMap",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "flatMap",
    }
    IteratorFlatMapNext {
        function: FunctionOrdinal(136) => BUILTIN_ITERATOR_FLAT_MAP_NEXT_FUNCTION_ID,
        debug: "Iterator flatMap helper next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    IteratorFlatMapReturn {
        function: FunctionOrdinal(137) => BUILTIN_ITERATOR_FLAT_MAP_RETURN_FUNCTION_ID,
        debug: "Iterator flatMap helper return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    IteratorPrototypeTake {
        function: FunctionOrdinal(138) => BUILTIN_ITERATOR_PROTOTYPE_TAKE_FUNCTION_ID,
        debug: "Iterator.prototype.take",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "take",
    }
    IteratorTakeNext {
        function: FunctionOrdinal(139) => BUILTIN_ITERATOR_TAKE_NEXT_FUNCTION_ID,
        debug: "Iterator take helper next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    IteratorTakeReturn {
        function: FunctionOrdinal(140) => BUILTIN_ITERATOR_TAKE_RETURN_FUNCTION_ID,
        debug: "Iterator take helper return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    IteratorPrototypeDrop {
        function: FunctionOrdinal(141) => BUILTIN_ITERATOR_PROTOTYPE_DROP_FUNCTION_ID,
        debug: "Iterator.prototype.drop",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "drop",
    }
    IteratorDropNext {
        function: FunctionOrdinal(142) => BUILTIN_ITERATOR_DROP_NEXT_FUNCTION_ID,
        debug: "Iterator drop helper next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    IteratorDropReturn {
        function: FunctionOrdinal(143) => BUILTIN_ITERATOR_DROP_RETURN_FUNCTION_ID,
        debug: "Iterator drop helper return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    IteratorPrototypeConstructorGetter {
        function: FunctionOrdinal(144) => BUILTIN_ITERATOR_PROTOTYPE_CONSTRUCTOR_GETTER_FUNCTION_ID,
        debug: "get Iterator.prototype.constructor",
        flags: [],
        installer: None,
        native: "get constructor",
    }
    IteratorPrototypeConstructorSetter {
        function: FunctionOrdinal(145) => BUILTIN_ITERATOR_PROTOTYPE_CONSTRUCTOR_SETTER_FUNCTION_ID,
        debug: "set Iterator.prototype.constructor",
        flags: [],
        installer: None,
        native: "set constructor",
    }
    IteratorPrototypeSymbolDispose {
        function: FunctionOrdinal(146) => BUILTIN_ITERATOR_PROTOTYPE_SYMBOL_DISPOSE_FUNCTION_ID,
        debug: "Iterator.prototype[Symbol.dispose]",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "[Symbol.dispose]",
    }
    IteratorPrototypeToStringTagGetter {
        function: FunctionOrdinal(147) => BUILTIN_ITERATOR_PROTOTYPE_TO_STRING_TAG_GETTER_FUNCTION_ID,
        debug: "get Iterator.prototype[Symbol.toStringTag]",
        flags: [],
        installer: None,
        native: "get [Symbol.toStringTag]",
    }
    IteratorPrototypeToStringTagSetter {
        function: FunctionOrdinal(148) => BUILTIN_ITERATOR_PROTOTYPE_TO_STRING_TAG_SETTER_FUNCTION_ID,
        debug: "set Iterator.prototype[Symbol.toStringTag]",
        flags: [],
        installer: None,
        native: "set [Symbol.toStringTag]",
    }
    IteratorFromWrapperNext {
        function: FunctionOrdinal(149) => BUILTIN_ITERATOR_FROM_WRAPPER_NEXT_FUNCTION_ID,
        debug: "%WrapForValidIteratorPrototype%.next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    IteratorFromWrapperReturn {
        function: FunctionOrdinal(150) => BUILTIN_ITERATOR_FROM_WRAPPER_RETURN_FUNCTION_ID,
        debug: "%WrapForValidIteratorPrototype%.return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    ArrayBufferConstructor {
        function: FunctionOrdinal(151) => BUILTIN_ARRAY_BUFFER_FUNCTION_ID,
        global: GlobalOrdinal(6),
        global_name: ARRAY_BUFFER_NAME,
        debug: ARRAY_BUFFER_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: ArrayBuffer,
        native: ARRAY_BUFFER_NAME,
    }
    SharedArrayBufferConstructor {
        function: FunctionOrdinal(152) => BUILTIN_SHARED_ARRAY_BUFFER_FUNCTION_ID,
        global: GlobalOrdinal(7),
        global_name: SHARED_ARRAY_BUFFER_NAME,
        debug: SHARED_ARRAY_BUFFER_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: ArrayBuffer,
        native: SHARED_ARRAY_BUFFER_NAME,
    }
    ArrayBufferIsView {
        function: FunctionOrdinal(153) => BUILTIN_ARRAY_BUFFER_IS_VIEW_FUNCTION_ID,
        debug: "ArrayBuffer.isView",
        flags: [STATIC_METHOD],
        installer: None,
        native: "isView",
    }
    ArrayBufferSpeciesGetter {
        function: FunctionOrdinal(154) => BUILTIN_ARRAY_BUFFER_SPECIES_GETTER_FUNCTION_ID,
        debug: "get ArrayBuffer [Symbol.species]",
        flags: [],
        installer: None,
        native: "get [Symbol.species]",
    }
    ArrayBufferPrototypeByteLengthGetter {
        function: FunctionOrdinal(155) => BUILTIN_ARRAY_BUFFER_PROTOTYPE_BYTE_LENGTH_GETTER_FUNCTION_ID,
        debug: "get ArrayBuffer.prototype.byteLength",
        flags: [],
        installer: None,
        native: "get byteLength",
    }
    SharedArrayBufferPrototypeByteLengthGetter {
        function: FunctionOrdinal(156) => BUILTIN_SHARED_ARRAY_BUFFER_PROTOTYPE_BYTE_LENGTH_GETTER_FUNCTION_ID,
        debug: "get SharedArrayBuffer.prototype.byteLength",
        flags: [],
        installer: None,
        native: "get byteLength",
    }
    SharedArrayBufferPrototypeMaxByteLengthGetter {
        function: FunctionOrdinal(157) => BUILTIN_SHARED_ARRAY_BUFFER_PROTOTYPE_MAX_BYTE_LENGTH_GETTER_FUNCTION_ID,
        debug: "get SharedArrayBuffer.prototype.maxByteLength",
        flags: [],
        installer: None,
        native: "get maxByteLength",
    }
    SharedArrayBufferPrototypeGrowableGetter {
        function: FunctionOrdinal(158) => BUILTIN_SHARED_ARRAY_BUFFER_PROTOTYPE_GROWABLE_GETTER_FUNCTION_ID,
        debug: "get SharedArrayBuffer.prototype.growable",
        flags: [],
        installer: None,
        native: "get growable",
    }
    SharedArrayBufferPrototypeGrow {
        function: FunctionOrdinal(159) => BUILTIN_SHARED_ARRAY_BUFFER_PROTOTYPE_GROW_FUNCTION_ID,
        debug: "SharedArrayBuffer.prototype.grow",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "grow",
    }
    ArrayBufferPrototypeDetachedGetter {
        function: FunctionOrdinal(160) => BUILTIN_ARRAY_BUFFER_PROTOTYPE_DETACHED_GETTER_FUNCTION_ID,
        debug: "get ArrayBuffer.prototype.detached",
        flags: [],
        installer: None,
        native: "get detached",
    }
    ArrayBufferPrototypeMaxByteLengthGetter {
        function: FunctionOrdinal(161) => BUILTIN_ARRAY_BUFFER_PROTOTYPE_MAX_BYTE_LENGTH_GETTER_FUNCTION_ID,
        debug: "get ArrayBuffer.prototype.maxByteLength",
        flags: [],
        installer: None,
        native: "get maxByteLength",
    }
    ArrayBufferPrototypeResizableGetter {
        function: FunctionOrdinal(162) => BUILTIN_ARRAY_BUFFER_PROTOTYPE_RESIZABLE_GETTER_FUNCTION_ID,
        debug: "get ArrayBuffer.prototype.resizable",
        flags: [],
        installer: None,
        native: "get resizable",
    }
    ArrayBufferPrototypeResize {
        function: FunctionOrdinal(163) => BUILTIN_ARRAY_BUFFER_PROTOTYPE_RESIZE_FUNCTION_ID,
        debug: "ArrayBuffer.prototype.resize",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "resize",
    }
    ArrayBufferPrototypeSlice {
        function: FunctionOrdinal(164) => BUILTIN_ARRAY_BUFFER_PROTOTYPE_SLICE_FUNCTION_ID,
        debug: "ArrayBuffer.prototype.slice",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "slice",
    }
    SharedArrayBufferPrototypeSlice {
        function: FunctionOrdinal(165) => BUILTIN_SHARED_ARRAY_BUFFER_PROTOTYPE_SLICE_FUNCTION_ID,
        debug: "SharedArrayBuffer.prototype.slice",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "slice",
    }
    ArrayBufferPrototypeTransfer {
        function: FunctionOrdinal(166) => BUILTIN_ARRAY_BUFFER_PROTOTYPE_TRANSFER_FUNCTION_ID,
        debug: "ArrayBuffer.prototype.transfer",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "transfer",
    }
    ArrayBufferPrototypeTransferToFixedLength {
        function: FunctionOrdinal(167) => BUILTIN_ARRAY_BUFFER_PROTOTYPE_TRANSFER_TO_FIXED_LENGTH_FUNCTION_ID,
        debug: "ArrayBuffer.prototype.transferToFixedLength",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "transferToFixedLength",
    }
    ArrayBufferPrototypeTransferToImmutable {
        function: FunctionOrdinal(168) => BUILTIN_ARRAY_BUFFER_PROTOTYPE_TRANSFER_TO_IMMUTABLE_FUNCTION_ID,
        debug: "ArrayBuffer.prototype.transferToImmutable",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "transferToImmutable",
    }
    ArrayBufferPrototypeSliceToImmutable {
        function: FunctionOrdinal(169) => BUILTIN_ARRAY_BUFFER_PROTOTYPE_SLICE_TO_IMMUTABLE_FUNCTION_ID,
        debug: "ArrayBuffer.prototype.sliceToImmutable",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "sliceToImmutable",
    }
    DataViewConstructor {
        function: FunctionOrdinal(170) => BUILTIN_DATA_VIEW_FUNCTION_ID,
        global: GlobalOrdinal(8),
        global_name: DATA_VIEW_NAME,
        debug: DATA_VIEW_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: DataView,
        native: DATA_VIEW_NAME,
    }
    DataViewPrototypeBufferGetter {
        function: FunctionOrdinal(171) => BUILTIN_DATA_VIEW_PROTOTYPE_BUFFER_GETTER_FUNCTION_ID,
        debug: "get DataView.prototype.buffer",
        flags: [],
        installer: None,
        native: "get buffer",
    }
    DataViewPrototypeByteLengthGetter {
        function: FunctionOrdinal(172) => BUILTIN_DATA_VIEW_PROTOTYPE_BYTE_LENGTH_GETTER_FUNCTION_ID,
        debug: "get DataView.prototype.byteLength",
        flags: [],
        installer: None,
        native: "get byteLength",
    }
    DataViewPrototypeByteOffsetGetter {
        function: FunctionOrdinal(173) => BUILTIN_DATA_VIEW_PROTOTYPE_BYTE_OFFSET_GETTER_FUNCTION_ID,
        debug: "get DataView.prototype.byteOffset",
        flags: [],
        installer: None,
        native: "get byteOffset",
    }
    TypedArraySpeciesGetter {
        function: FunctionOrdinal(174) => BUILTIN_TYPED_ARRAY_SPECIES_GETTER_FUNCTION_ID,
        debug: "get TypedArray [Symbol.species]",
        flags: [],
        installer: None,
        native: "get [Symbol.species]",
    }
    TypedArrayPrototypeBufferGetter {
        function: FunctionOrdinal(175) => BUILTIN_TYPED_ARRAY_PROTOTYPE_BUFFER_GETTER_FUNCTION_ID,
        debug: "get TypedArray.prototype.buffer",
        flags: [],
        installer: None,
        native: "get buffer",
    }
    TypedArrayPrototypeByteLengthGetter {
        function: FunctionOrdinal(176) => BUILTIN_TYPED_ARRAY_PROTOTYPE_BYTE_LENGTH_GETTER_FUNCTION_ID,
        debug: "get TypedArray.prototype.byteLength",
        flags: [],
        installer: None,
        native: "get byteLength",
    }
    TypedArrayPrototypeByteOffsetGetter {
        function: FunctionOrdinal(177) => BUILTIN_TYPED_ARRAY_PROTOTYPE_BYTE_OFFSET_GETTER_FUNCTION_ID,
        debug: "get TypedArray.prototype.byteOffset",
        flags: [],
        installer: None,
        native: "get byteOffset",
    }
    TypedArrayPrototypeLengthGetter {
        function: FunctionOrdinal(178) => BUILTIN_TYPED_ARRAY_PROTOTYPE_LENGTH_GETTER_FUNCTION_ID,
        debug: "get TypedArray.prototype.length",
        flags: [],
        installer: None,
        native: "get length",
    }
    TypedArrayPrototypeToStringTagGetter {
        function: FunctionOrdinal(179) => BUILTIN_TYPED_ARRAY_PROTOTYPE_TO_STRING_TAG_GETTER_FUNCTION_ID,
        debug: "get TypedArray.prototype[Symbol.toStringTag]",
        flags: [],
        installer: None,
        native: "get [Symbol.toStringTag]",
    }
    TypedArrayPrototypeToString {
        function: FunctionOrdinal(180) => BUILTIN_TYPED_ARRAY_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "TypedArray.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toString",
    }
    TypedArrayPrototypeAt {
        function: FunctionOrdinal(181) => BUILTIN_TYPED_ARRAY_PROTOTYPE_AT_FUNCTION_ID,
        debug: "TypedArray.prototype.at",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "at",
    }
    TypedArrayPrototypeCopyWithin {
        function: FunctionOrdinal(182) => BUILTIN_TYPED_ARRAY_PROTOTYPE_COPY_WITHIN_FUNCTION_ID,
        debug: "TypedArray.prototype.copyWithin",
        flags: [INDEXED_RECEIVER_MUTATION, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "copyWithin",
    }
    TypedArrayPrototypeIncludes {
        function: FunctionOrdinal(183) => BUILTIN_TYPED_ARRAY_PROTOTYPE_INCLUDES_FUNCTION_ID,
        debug: "TypedArray.prototype.includes",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "includes",
    }
    TypedArrayPrototypeIndexOf {
        function: FunctionOrdinal(184) => BUILTIN_TYPED_ARRAY_PROTOTYPE_INDEX_OF_FUNCTION_ID,
        debug: "TypedArray.prototype.indexOf",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "indexOf",
    }
    TypedArrayPrototypeLastIndexOf {
        function: FunctionOrdinal(185) => BUILTIN_TYPED_ARRAY_PROTOTYPE_LAST_INDEX_OF_FUNCTION_ID,
        debug: "TypedArray.prototype.lastIndexOf",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "lastIndexOf",
    }
    TypedArrayPrototypeFind {
        function: FunctionOrdinal(186) => BUILTIN_TYPED_ARRAY_PROTOTYPE_FIND_FUNCTION_ID,
        debug: "TypedArray.prototype.find",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "find",
    }
    TypedArrayPrototypeFindIndex {
        function: FunctionOrdinal(187) => BUILTIN_TYPED_ARRAY_PROTOTYPE_FIND_INDEX_FUNCTION_ID,
        debug: "TypedArray.prototype.findIndex",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "findIndex",
    }
    TypedArrayPrototypeFindLast {
        function: FunctionOrdinal(188) => BUILTIN_TYPED_ARRAY_PROTOTYPE_FIND_LAST_FUNCTION_ID,
        debug: "TypedArray.prototype.findLast",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "findLast",
    }
    TypedArrayPrototypeFindLastIndex {
        function: FunctionOrdinal(189) => BUILTIN_TYPED_ARRAY_PROTOTYPE_FIND_LAST_INDEX_FUNCTION_ID,
        debug: "TypedArray.prototype.findLastIndex",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "findLastIndex",
    }
    TypedArrayPrototypeEvery {
        function: FunctionOrdinal(190) => BUILTIN_TYPED_ARRAY_PROTOTYPE_EVERY_FUNCTION_ID,
        debug: "TypedArray.prototype.every",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "every",
    }
    TypedArrayPrototypeSome {
        function: FunctionOrdinal(191) => BUILTIN_TYPED_ARRAY_PROTOTYPE_SOME_FUNCTION_ID,
        debug: "TypedArray.prototype.some",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "some",
    }
    TypedArrayPrototypeMap {
        function: FunctionOrdinal(192) => BUILTIN_TYPED_ARRAY_PROTOTYPE_MAP_FUNCTION_ID,
        debug: "TypedArray.prototype.map",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "map",
    }
    TypedArrayPrototypeFilter {
        function: FunctionOrdinal(193) => BUILTIN_TYPED_ARRAY_PROTOTYPE_FILTER_FUNCTION_ID,
        debug: "TypedArray.prototype.filter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "filter",
    }
    TypedArrayPrototypeForEach {
        function: FunctionOrdinal(194) => BUILTIN_TYPED_ARRAY_PROTOTYPE_FOR_EACH_FUNCTION_ID,
        debug: "TypedArray.prototype.forEach",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "forEach",
    }
    TypedArrayPrototypeReduce {
        function: FunctionOrdinal(195) => BUILTIN_TYPED_ARRAY_PROTOTYPE_REDUCE_FUNCTION_ID,
        debug: "TypedArray.prototype.reduce",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "reduce",
    }
    TypedArrayPrototypeReduceRight {
        function: FunctionOrdinal(196) => BUILTIN_TYPED_ARRAY_PROTOTYPE_REDUCE_RIGHT_FUNCTION_ID,
        debug: "TypedArray.prototype.reduceRight",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "reduceRight",
    }
    TypedArrayPrototypeValues {
        function: FunctionOrdinal(197) => BUILTIN_TYPED_ARRAY_PROTOTYPE_VALUES_FUNCTION_ID,
        debug: "TypedArray.prototype.values",
        flags: [],
        installer: None,
        native: "values",
    }
    TypedArrayPrototypeKeys {
        function: FunctionOrdinal(198) => BUILTIN_TYPED_ARRAY_PROTOTYPE_KEYS_FUNCTION_ID,
        debug: "TypedArray.prototype.keys",
        flags: [],
        installer: None,
        native: "keys",
    }
    TypedArrayPrototypeEntries {
        function: FunctionOrdinal(199) => BUILTIN_TYPED_ARRAY_PROTOTYPE_ENTRIES_FUNCTION_ID,
        debug: "TypedArray.prototype.entries",
        flags: [],
        installer: None,
        native: "entries",
    }
    TypedArrayPrototypeJoin {
        function: FunctionOrdinal(200) => BUILTIN_TYPED_ARRAY_PROTOTYPE_JOIN_FUNCTION_ID,
        debug: "TypedArray.prototype.join",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "join",
    }
    TypedArrayPrototypeToLocaleString {
        function: FunctionOrdinal(201) => BUILTIN_TYPED_ARRAY_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID,
        debug: "TypedArray.prototype.toLocaleString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toLocaleString",
    }
    TypedArrayPrototypeSubarray {
        function: FunctionOrdinal(202) => BUILTIN_TYPED_ARRAY_PROTOTYPE_SUBARRAY_FUNCTION_ID,
        debug: "TypedArray.prototype.subarray",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "subarray",
    }
    TypedArrayPrototypeSlice {
        function: FunctionOrdinal(203) => BUILTIN_TYPED_ARRAY_PROTOTYPE_SLICE_FUNCTION_ID,
        debug: "TypedArray.prototype.slice",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "slice",
    }
    TypedArrayPrototypeSet {
        function: FunctionOrdinal(204) => BUILTIN_TYPED_ARRAY_PROTOTYPE_SET_FUNCTION_ID,
        debug: "TypedArray.prototype.set",
        flags: [INDEXED_RECEIVER_MUTATION, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "set",
    }
    TypedArrayPrototypeReverse {
        function: FunctionOrdinal(205) => BUILTIN_TYPED_ARRAY_PROTOTYPE_REVERSE_FUNCTION_ID,
        debug: "TypedArray.prototype.reverse",
        flags: [INDEXED_RECEIVER_MUTATION],
        installer: None,
        native: "reverse",
    }
    TypedArrayPrototypeSort {
        function: FunctionOrdinal(206) => BUILTIN_TYPED_ARRAY_PROTOTYPE_SORT_FUNCTION_ID,
        debug: "TypedArray.prototype.sort",
        flags: [SYNCHRONOUS_USER_CODE, INDEXED_RECEIVER_MUTATION],
        installer: None,
        native: "sort",
    }
    TypedArrayPrototypeToReversed {
        function: FunctionOrdinal(207) => BUILTIN_TYPED_ARRAY_PROTOTYPE_TO_REVERSED_FUNCTION_ID,
        debug: "TypedArray.prototype.toReversed",
        flags: [],
        installer: None,
        native: "toReversed",
    }
    TypedArrayPrototypeToSorted {
        function: FunctionOrdinal(208) => BUILTIN_TYPED_ARRAY_PROTOTYPE_TO_SORTED_FUNCTION_ID,
        debug: "TypedArray.prototype.toSorted",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toSorted",
    }
    TypedArrayPrototypeWith {
        function: FunctionOrdinal(209) => BUILTIN_TYPED_ARRAY_PROTOTYPE_WITH_FUNCTION_ID,
        debug: "TypedArray.prototype.with",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "with",
    }
    TypedArrayFrom {
        function: FunctionOrdinal(210) => BUILTIN_TYPED_ARRAY_FROM_FUNCTION_ID,
        debug: "TypedArray.from",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "from",
    }
    TypedArrayOf {
        function: FunctionOrdinal(211) => BUILTIN_TYPED_ARRAY_OF_FUNCTION_ID,
        debug: "TypedArray.of",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "of",
    }
    DataViewPrototypeGetUint8 {
        function: FunctionOrdinal(212) => BUILTIN_DATA_VIEW_PROTOTYPE_GET_UINT8_FUNCTION_ID,
        debug: "DataView.prototype.getUint8",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getUint8",
    }
    DataViewPrototypeSetUint8 {
        function: FunctionOrdinal(213) => BUILTIN_DATA_VIEW_PROTOTYPE_SET_UINT8_FUNCTION_ID,
        debug: "DataView.prototype.setUint8",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setUint8",
    }
    DataViewPrototypeGetInt8 {
        function: FunctionOrdinal(214) => BUILTIN_DATA_VIEW_PROTOTYPE_GET_INT8_FUNCTION_ID,
        debug: "DataView.prototype.getInt8",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getInt8",
    }
    DataViewPrototypeSetInt8 {
        function: FunctionOrdinal(215) => BUILTIN_DATA_VIEW_PROTOTYPE_SET_INT8_FUNCTION_ID,
        debug: "DataView.prototype.setInt8",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setInt8",
    }
    DataViewPrototypeGetUint16 {
        function: FunctionOrdinal(216) => BUILTIN_DATA_VIEW_PROTOTYPE_GET_UINT16_FUNCTION_ID,
        debug: "DataView.prototype.getUint16",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getUint16",
    }
    DataViewPrototypeSetUint16 {
        function: FunctionOrdinal(217) => BUILTIN_DATA_VIEW_PROTOTYPE_SET_UINT16_FUNCTION_ID,
        debug: "DataView.prototype.setUint16",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setUint16",
    }
    DataViewPrototypeGetInt16 {
        function: FunctionOrdinal(218) => BUILTIN_DATA_VIEW_PROTOTYPE_GET_INT16_FUNCTION_ID,
        debug: "DataView.prototype.getInt16",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getInt16",
    }
    DataViewPrototypeSetInt16 {
        function: FunctionOrdinal(219) => BUILTIN_DATA_VIEW_PROTOTYPE_SET_INT16_FUNCTION_ID,
        debug: "DataView.prototype.setInt16",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setInt16",
    }
    DataViewPrototypeGetUint32 {
        function: FunctionOrdinal(220) => BUILTIN_DATA_VIEW_PROTOTYPE_GET_UINT32_FUNCTION_ID,
        debug: "DataView.prototype.getUint32",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getUint32",
    }
    DataViewPrototypeSetUint32 {
        function: FunctionOrdinal(221) => BUILTIN_DATA_VIEW_PROTOTYPE_SET_UINT32_FUNCTION_ID,
        debug: "DataView.prototype.setUint32",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setUint32",
    }
    DataViewPrototypeGetInt32 {
        function: FunctionOrdinal(222) => BUILTIN_DATA_VIEW_PROTOTYPE_GET_INT32_FUNCTION_ID,
        debug: "DataView.prototype.getInt32",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getInt32",
    }
    DataViewPrototypeSetInt32 {
        function: FunctionOrdinal(223) => BUILTIN_DATA_VIEW_PROTOTYPE_SET_INT32_FUNCTION_ID,
        debug: "DataView.prototype.setInt32",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setInt32",
    }
    DataViewPrototypeGetFloat16 {
        function: FunctionOrdinal(224) => BUILTIN_DATA_VIEW_PROTOTYPE_GET_FLOAT16_FUNCTION_ID,
        debug: "DataView.prototype.getFloat16",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getFloat16",
    }
    DataViewPrototypeSetFloat16 {
        function: FunctionOrdinal(225) => BUILTIN_DATA_VIEW_PROTOTYPE_SET_FLOAT16_FUNCTION_ID,
        debug: "DataView.prototype.setFloat16",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setFloat16",
    }
    DataViewPrototypeGetFloat32 {
        function: FunctionOrdinal(226) => BUILTIN_DATA_VIEW_PROTOTYPE_GET_FLOAT32_FUNCTION_ID,
        debug: "DataView.prototype.getFloat32",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getFloat32",
    }
    DataViewPrototypeSetFloat32 {
        function: FunctionOrdinal(227) => BUILTIN_DATA_VIEW_PROTOTYPE_SET_FLOAT32_FUNCTION_ID,
        debug: "DataView.prototype.setFloat32",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setFloat32",
    }
    DataViewPrototypeGetFloat64 {
        function: FunctionOrdinal(228) => BUILTIN_DATA_VIEW_PROTOTYPE_GET_FLOAT64_FUNCTION_ID,
        debug: "DataView.prototype.getFloat64",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getFloat64",
    }
    DataViewPrototypeSetFloat64 {
        function: FunctionOrdinal(229) => BUILTIN_DATA_VIEW_PROTOTYPE_SET_FLOAT64_FUNCTION_ID,
        debug: "DataView.prototype.setFloat64",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setFloat64",
    }
    DataViewPrototypeGetBigInt64 {
        function: FunctionOrdinal(230) => BUILTIN_DATA_VIEW_PROTOTYPE_GET_BIGINT64_FUNCTION_ID,
        debug: "DataView.prototype.getBigInt64",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getBigInt64",
    }
    DataViewPrototypeSetBigInt64 {
        function: FunctionOrdinal(231) => BUILTIN_DATA_VIEW_PROTOTYPE_SET_BIGINT64_FUNCTION_ID,
        debug: "DataView.prototype.setBigInt64",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setBigInt64",
    }
    DataViewPrototypeGetBigUint64 {
        function: FunctionOrdinal(232) => BUILTIN_DATA_VIEW_PROTOTYPE_GET_BIGUINT64_FUNCTION_ID,
        debug: "DataView.prototype.getBigUint64",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getBigUint64",
    }
    DataViewPrototypeSetBigUint64 {
        function: FunctionOrdinal(233) => BUILTIN_DATA_VIEW_PROTOTYPE_SET_BIGUINT64_FUNCTION_ID,
        debug: "DataView.prototype.setBigUint64",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "setBigUint64",
    }
    DateConstructor {
        function: FunctionOrdinal(234) => BUILTIN_DATE_FUNCTION_ID,
        global: GlobalOrdinal(9),
        global_name: DATE_NAME,
        debug: DATE_NAME,
        flags: [WALL_CLOCK, CONSTRUCTABLE, SYSTEM_TIME_ZONE, INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: Date,
        native: DATE_NAME,
    }
    DateNow {
        function: FunctionOrdinal(235) => BUILTIN_DATE_NOW_FUNCTION_ID,
        debug: "Date.now",
        flags: [WALL_CLOCK, STATIC_METHOD],
        installer: None,
        native: "now",
    }
    DateParse {
        function: FunctionOrdinal(236) => BUILTIN_DATE_PARSE_FUNCTION_ID,
        debug: "Date.parse",
        flags: [STATIC_METHOD, SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "parse",
    }
    DateUtc {
        function: FunctionOrdinal(237) => BUILTIN_DATE_UTC_FUNCTION_ID,
        debug: "Date.UTC",
        flags: [STATIC_METHOD],
        installer: None,
        native: "UTC",
    }
    DatePrototypeGetTime {
        function: FunctionOrdinal(238) => BUILTIN_DATE_PROTOTYPE_GET_TIME_FUNCTION_ID,
        debug: "Date.prototype.getTime",
        flags: [],
        installer: None,
        native: "getTime",
    }
    DatePrototypeSetTime {
        function: FunctionOrdinal(239) => BUILTIN_DATE_PROTOTYPE_SET_TIME_FUNCTION_ID,
        debug: "Date.prototype.setTime",
        flags: [],
        installer: None,
        native: "setTime",
    }
    DatePrototypeValueOf {
        function: FunctionOrdinal(240) => BUILTIN_DATE_PROTOTYPE_VALUE_OF_FUNCTION_ID,
        debug: "Date.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    DatePrototypeGetFullYear {
        function: FunctionOrdinal(241) => BUILTIN_DATE_PROTOTYPE_GET_FULL_YEAR_FUNCTION_ID,
        debug: "Date.prototype.getFullYear",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "getFullYear",
    }
    DatePrototypeGetUtcFullYear {
        function: FunctionOrdinal(242) => BUILTIN_DATE_PROTOTYPE_GET_UTC_FULL_YEAR_FUNCTION_ID,
        debug: "Date.prototype.getUTCFullYear",
        flags: [],
        installer: None,
        native: "getUTCFullYear",
    }
    DatePrototypeGetMonth {
        function: FunctionOrdinal(243) => BUILTIN_DATE_PROTOTYPE_GET_MONTH_FUNCTION_ID,
        debug: "Date.prototype.getMonth",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "getMonth",
    }
    DatePrototypeGetUtcMonth {
        function: FunctionOrdinal(244) => BUILTIN_DATE_PROTOTYPE_GET_UTC_MONTH_FUNCTION_ID,
        debug: "Date.prototype.getUTCMonth",
        flags: [],
        installer: None,
        native: "getUTCMonth",
    }
    DatePrototypeGetDate {
        function: FunctionOrdinal(245) => BUILTIN_DATE_PROTOTYPE_GET_DATE_FUNCTION_ID,
        debug: "Date.prototype.getDate",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "getDate",
    }
    DatePrototypeGetUtcDate {
        function: FunctionOrdinal(246) => BUILTIN_DATE_PROTOTYPE_GET_UTC_DATE_FUNCTION_ID,
        debug: "Date.prototype.getUTCDate",
        flags: [],
        installer: None,
        native: "getUTCDate",
    }
    DatePrototypeGetDay {
        function: FunctionOrdinal(247) => BUILTIN_DATE_PROTOTYPE_GET_DAY_FUNCTION_ID,
        debug: "Date.prototype.getDay",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "getDay",
    }
    DatePrototypeGetUtcDay {
        function: FunctionOrdinal(248) => BUILTIN_DATE_PROTOTYPE_GET_UTC_DAY_FUNCTION_ID,
        debug: "Date.prototype.getUTCDay",
        flags: [],
        installer: None,
        native: "getUTCDay",
    }
    DatePrototypeGetHours {
        function: FunctionOrdinal(249) => BUILTIN_DATE_PROTOTYPE_GET_HOURS_FUNCTION_ID,
        debug: "Date.prototype.getHours",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "getHours",
    }
    DatePrototypeGetUtcHours {
        function: FunctionOrdinal(250) => BUILTIN_DATE_PROTOTYPE_GET_UTC_HOURS_FUNCTION_ID,
        debug: "Date.prototype.getUTCHours",
        flags: [],
        installer: None,
        native: "getUTCHours",
    }
    DatePrototypeGetMinutes {
        function: FunctionOrdinal(251) => BUILTIN_DATE_PROTOTYPE_GET_MINUTES_FUNCTION_ID,
        debug: "Date.prototype.getMinutes",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "getMinutes",
    }
    DatePrototypeGetUtcMinutes {
        function: FunctionOrdinal(252) => BUILTIN_DATE_PROTOTYPE_GET_UTC_MINUTES_FUNCTION_ID,
        debug: "Date.prototype.getUTCMinutes",
        flags: [],
        installer: None,
        native: "getUTCMinutes",
    }
    DatePrototypeGetSeconds {
        function: FunctionOrdinal(253) => BUILTIN_DATE_PROTOTYPE_GET_SECONDS_FUNCTION_ID,
        debug: "Date.prototype.getSeconds",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "getSeconds",
    }
    DatePrototypeGetUtcSeconds {
        function: FunctionOrdinal(254) => BUILTIN_DATE_PROTOTYPE_GET_UTC_SECONDS_FUNCTION_ID,
        debug: "Date.prototype.getUTCSeconds",
        flags: [],
        installer: None,
        native: "getUTCSeconds",
    }
    DatePrototypeGetMilliseconds {
        function: FunctionOrdinal(255) => BUILTIN_DATE_PROTOTYPE_GET_MILLISECONDS_FUNCTION_ID,
        debug: "Date.prototype.getMilliseconds",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "getMilliseconds",
    }
    DatePrototypeGetUtcMilliseconds {
        function: FunctionOrdinal(256) => BUILTIN_DATE_PROTOTYPE_GET_UTC_MILLISECONDS_FUNCTION_ID,
        debug: "Date.prototype.getUTCMilliseconds",
        flags: [],
        installer: None,
        native: "getUTCMilliseconds",
    }
    DatePrototypeGetTimezoneOffset {
        function: FunctionOrdinal(257) => BUILTIN_DATE_PROTOTYPE_GET_TIMEZONE_OFFSET_FUNCTION_ID,
        debug: "Date.prototype.getTimezoneOffset",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "getTimezoneOffset",
    }
    DatePrototypeGetYear {
        function: FunctionOrdinal(258) => BUILTIN_DATE_PROTOTYPE_GET_YEAR_FUNCTION_ID,
        debug: "Date.prototype.getYear",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "getYear",
    }
    DatePrototypeSetYear {
        function: FunctionOrdinal(259) => BUILTIN_DATE_PROTOTYPE_SET_YEAR_FUNCTION_ID,
        debug: "Date.prototype.setYear",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "setYear",
    }
    DatePrototypeSetFullYear {
        function: FunctionOrdinal(260) => BUILTIN_DATE_PROTOTYPE_SET_FULL_YEAR_FUNCTION_ID,
        debug: "Date.prototype.setFullYear",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "setFullYear",
    }
    DatePrototypeSetUtcFullYear {
        function: FunctionOrdinal(261) => BUILTIN_DATE_PROTOTYPE_SET_UTC_FULL_YEAR_FUNCTION_ID,
        debug: "Date.prototype.setUTCFullYear",
        flags: [],
        installer: None,
        native: "setUTCFullYear",
    }
    DatePrototypeSetMonth {
        function: FunctionOrdinal(262) => BUILTIN_DATE_PROTOTYPE_SET_MONTH_FUNCTION_ID,
        debug: "Date.prototype.setMonth",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "setMonth",
    }
    DatePrototypeSetUtcMonth {
        function: FunctionOrdinal(263) => BUILTIN_DATE_PROTOTYPE_SET_UTC_MONTH_FUNCTION_ID,
        debug: "Date.prototype.setUTCMonth",
        flags: [],
        installer: None,
        native: "setUTCMonth",
    }
    DatePrototypeSetDate {
        function: FunctionOrdinal(264) => BUILTIN_DATE_PROTOTYPE_SET_DATE_FUNCTION_ID,
        debug: "Date.prototype.setDate",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "setDate",
    }
    DatePrototypeSetUtcDate {
        function: FunctionOrdinal(265) => BUILTIN_DATE_PROTOTYPE_SET_UTC_DATE_FUNCTION_ID,
        debug: "Date.prototype.setUTCDate",
        flags: [],
        installer: None,
        native: "setUTCDate",
    }
    DatePrototypeSetHours {
        function: FunctionOrdinal(266) => BUILTIN_DATE_PROTOTYPE_SET_HOURS_FUNCTION_ID,
        debug: "Date.prototype.setHours",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "setHours",
    }
    DatePrototypeSetUtcHours {
        function: FunctionOrdinal(267) => BUILTIN_DATE_PROTOTYPE_SET_UTC_HOURS_FUNCTION_ID,
        debug: "Date.prototype.setUTCHours",
        flags: [],
        installer: None,
        native: "setUTCHours",
    }
    DatePrototypeSetMinutes {
        function: FunctionOrdinal(268) => BUILTIN_DATE_PROTOTYPE_SET_MINUTES_FUNCTION_ID,
        debug: "Date.prototype.setMinutes",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "setMinutes",
    }
    DatePrototypeSetUtcMinutes {
        function: FunctionOrdinal(269) => BUILTIN_DATE_PROTOTYPE_SET_UTC_MINUTES_FUNCTION_ID,
        debug: "Date.prototype.setUTCMinutes",
        flags: [],
        installer: None,
        native: "setUTCMinutes",
    }
    DatePrototypeSetSeconds {
        function: FunctionOrdinal(270) => BUILTIN_DATE_PROTOTYPE_SET_SECONDS_FUNCTION_ID,
        debug: "Date.prototype.setSeconds",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "setSeconds",
    }
    DatePrototypeSetUtcSeconds {
        function: FunctionOrdinal(271) => BUILTIN_DATE_PROTOTYPE_SET_UTC_SECONDS_FUNCTION_ID,
        debug: "Date.prototype.setUTCSeconds",
        flags: [],
        installer: None,
        native: "setUTCSeconds",
    }
    DatePrototypeSetMilliseconds {
        function: FunctionOrdinal(272) => BUILTIN_DATE_PROTOTYPE_SET_MILLISECONDS_FUNCTION_ID,
        debug: "Date.prototype.setMilliseconds",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "setMilliseconds",
    }
    DatePrototypeSetUtcMilliseconds {
        function: FunctionOrdinal(273) => BUILTIN_DATE_PROTOTYPE_SET_UTC_MILLISECONDS_FUNCTION_ID,
        debug: "Date.prototype.setUTCMilliseconds",
        flags: [],
        installer: None,
        native: "setUTCMilliseconds",
    }
    DatePrototypeToIsoString {
        function: FunctionOrdinal(274) => BUILTIN_DATE_PROTOTYPE_TO_ISO_STRING_FUNCTION_ID,
        debug: "Date.prototype.toISOString",
        flags: [],
        installer: None,
        native: "toISOString",
    }
    DatePrototypeToJson {
        function: FunctionOrdinal(275) => BUILTIN_DATE_PROTOTYPE_TO_JSON_FUNCTION_ID,
        debug: "Date.prototype.toJSON",
        flags: [],
        installer: None,
        native: "toJSON",
    }
    DatePrototypeToPrimitive {
        function: FunctionOrdinal(276) => BUILTIN_DATE_PROTOTYPE_TO_PRIMITIVE_FUNCTION_ID,
        debug: "Date.prototype[Symbol.toPrimitive]",
        flags: [],
        installer: None,
        native: "[Symbol.toPrimitive]",
    }
    DatePrototypeToDateString {
        function: FunctionOrdinal(277) => BUILTIN_DATE_PROTOTYPE_TO_DATE_STRING_FUNCTION_ID,
        debug: "Date.prototype.toDateString",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "toDateString",
    }
    DatePrototypeToLocaleDateString {
        function: FunctionOrdinal(278) => BUILTIN_DATE_PROTOTYPE_TO_LOCALE_DATE_STRING_FUNCTION_ID,
        debug: "Date.prototype.toLocaleDateString",
        flags: [INTL_HOST, SYSTEM_TIME_ZONE],
        installer: None,
        native: "toLocaleDateString",
    }
    DatePrototypeToLocaleString {
        function: FunctionOrdinal(279) => BUILTIN_DATE_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID,
        debug: "Date.prototype.toLocaleString",
        flags: [INTL_HOST, SYSTEM_TIME_ZONE],
        installer: None,
        native: "toLocaleString",
    }
    DatePrototypeToLocaleTimeString {
        function: FunctionOrdinal(280) => BUILTIN_DATE_PROTOTYPE_TO_LOCALE_TIME_STRING_FUNCTION_ID,
        debug: "Date.prototype.toLocaleTimeString",
        flags: [INTL_HOST, SYSTEM_TIME_ZONE],
        installer: None,
        native: "toLocaleTimeString",
    }
    DatePrototypeToTemporalInstant {
        function: FunctionOrdinal(281) => BUILTIN_DATE_PROTOTYPE_TO_TEMPORAL_INSTANT_FUNCTION_ID,
        debug: "Date.prototype.toTemporalInstant",
        flags: [],
        installer: None,
        native: "toTemporalInstant",
    }
    DatePrototypeToTimeString {
        function: FunctionOrdinal(282) => BUILTIN_DATE_PROTOTYPE_TO_TIME_STRING_FUNCTION_ID,
        debug: "Date.prototype.toTimeString",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "toTimeString",
    }
    DatePrototypeToString {
        function: FunctionOrdinal(283) => BUILTIN_DATE_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Date.prototype.toString",
        flags: [SYSTEM_TIME_ZONE, INTL_HOST],
        installer: None,
        native: "toString",
    }
    DatePrototypeToUtcString {
        function: FunctionOrdinal(284) => BUILTIN_DATE_PROTOTYPE_TO_UTC_STRING_FUNCTION_ID,
        debug: "Date.prototype.toUTCString",
        flags: [],
        installer: None,
        native: "toUTCString",
    }
    TemporalPlainDateConstructor {
        function: FunctionOrdinal(285) => BUILTIN_TEMPORAL_PLAIN_DATE_FUNCTION_ID,
        debug: "Temporal.PlainDate",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: TemporalPlainDate,
        native: "PlainDate",
    }
    TemporalPlainDateFrom {
        function: FunctionOrdinal(286) => BUILTIN_TEMPORAL_PLAIN_DATE_FROM_FUNCTION_ID,
        debug: "Temporal.PlainDate.from",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "from",
    }
    TemporalPlainDateCompare {
        function: FunctionOrdinal(287) => BUILTIN_TEMPORAL_PLAIN_DATE_COMPARE_FUNCTION_ID,
        debug: "Temporal.PlainDate.compare",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "compare",
    }
    TemporalPlainDatePrototypeCalendarIdGetter {
        function: FunctionOrdinal(288) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_CALENDAR_ID_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.calendarId",
        flags: [],
        installer: None,
        native: "get calendarId",
    }
    TemporalPlainDatePrototypeEraGetter {
        function: FunctionOrdinal(289) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_ERA_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.era",
        flags: [],
        installer: None,
        native: "get era",
    }
    TemporalPlainDatePrototypeEraYearGetter {
        function: FunctionOrdinal(290) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_ERA_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.eraYear",
        flags: [],
        installer: None,
        native: "get eraYear",
    }
    TemporalPlainDatePrototypeYearGetter {
        function: FunctionOrdinal(291) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.year",
        flags: [],
        installer: None,
        native: "get year",
    }
    TemporalPlainDatePrototypeMonthGetter {
        function: FunctionOrdinal(292) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_MONTH_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.month",
        flags: [],
        installer: None,
        native: "get month",
    }
    TemporalPlainDatePrototypeMonthCodeGetter {
        function: FunctionOrdinal(293) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_MONTH_CODE_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.monthCode",
        flags: [],
        installer: None,
        native: "get monthCode",
    }
    TemporalPlainDatePrototypeDayGetter {
        function: FunctionOrdinal(294) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAY_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.day",
        flags: [],
        installer: None,
        native: "get day",
    }
    TemporalPlainDatePrototypeDayOfWeekGetter {
        function: FunctionOrdinal(295) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAY_OF_WEEK_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.dayOfWeek",
        flags: [],
        installer: None,
        native: "get dayOfWeek",
    }
    TemporalPlainDatePrototypeDayOfYearGetter {
        function: FunctionOrdinal(296) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAY_OF_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.dayOfYear",
        flags: [],
        installer: None,
        native: "get dayOfYear",
    }
    TemporalPlainDatePrototypeWeekOfYearGetter {
        function: FunctionOrdinal(297) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_WEEK_OF_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.weekOfYear",
        flags: [],
        installer: None,
        native: "get weekOfYear",
    }
    TemporalPlainDatePrototypeYearOfWeekGetter {
        function: FunctionOrdinal(298) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_YEAR_OF_WEEK_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.yearOfWeek",
        flags: [],
        installer: None,
        native: "get yearOfWeek",
    }
    TemporalPlainDatePrototypeDaysInWeekGetter {
        function: FunctionOrdinal(299) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAYS_IN_WEEK_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.daysInWeek",
        flags: [],
        installer: None,
        native: "get daysInWeek",
    }
    TemporalPlainDatePrototypeDaysInMonthGetter {
        function: FunctionOrdinal(300) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAYS_IN_MONTH_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.daysInMonth",
        flags: [],
        installer: None,
        native: "get daysInMonth",
    }
    TemporalPlainDatePrototypeDaysInYearGetter {
        function: FunctionOrdinal(301) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAYS_IN_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.daysInYear",
        flags: [],
        installer: None,
        native: "get daysInYear",
    }
    TemporalPlainDatePrototypeMonthsInYearGetter {
        function: FunctionOrdinal(302) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_MONTHS_IN_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.monthsInYear",
        flags: [],
        installer: None,
        native: "get monthsInYear",
    }
    TemporalPlainDatePrototypeInLeapYearGetter {
        function: FunctionOrdinal(303) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_IN_LEAP_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDate.prototype.inLeapYear",
        flags: [],
        installer: None,
        native: "get inLeapYear",
    }
    TemporalPlainDatePrototypeWith {
        function: FunctionOrdinal(304) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_WITH_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.with",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "with",
    }
    TemporalPlainDatePrototypeWithCalendar {
        function: FunctionOrdinal(305) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_WITH_CALENDAR_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.withCalendar",
        flags: [],
        installer: None,
        native: "withCalendar",
    }
    TemporalPlainDatePrototypeEquals {
        function: FunctionOrdinal(306) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_EQUALS_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.equals",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "equals",
    }
    TemporalPlainDatePrototypeToString {
        function: FunctionOrdinal(307) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toString",
    }
    TemporalPlainDatePrototypeToJson {
        function: FunctionOrdinal(308) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_JSON_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.toJSON",
        flags: [],
        installer: None,
        native: "toJSON",
    }
    TemporalPlainDatePrototypeToLocaleString {
        function: FunctionOrdinal(309) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.toLocaleString",
        flags: [INTL_HOST, SYSTEM_TIME_ZONE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toLocaleString",
    }
    TemporalPlainDatePrototypeValueOf {
        function: FunctionOrdinal(310) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_VALUE_OF_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    TemporalPlainDatePrototypeAdd {
        function: FunctionOrdinal(311) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_ADD_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.add",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "add",
    }
    TemporalPlainDatePrototypeSubtract {
        function: FunctionOrdinal(312) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_SUBTRACT_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.subtract",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "subtract",
    }
    TemporalPlainDatePrototypeUntil {
        function: FunctionOrdinal(313) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_UNTIL_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.until",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "until",
    }
    TemporalPlainDatePrototypeSince {
        function: FunctionOrdinal(314) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_SINCE_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.since",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "since",
    }
    TemporalPlainDatePrototypeToPlainDateTime {
        function: FunctionOrdinal(315) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_PLAIN_DATE_TIME_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.toPlainDateTime",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toPlainDateTime",
    }
    TemporalPlainDatePrototypeToPlainYearMonth {
        function: FunctionOrdinal(316) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_PLAIN_YEAR_MONTH_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.toPlainYearMonth",
        flags: [],
        installer: None,
        native: "toPlainYearMonth",
    }
    TemporalPlainDatePrototypeToPlainMonthDay {
        function: FunctionOrdinal(317) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_PLAIN_MONTH_DAY_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.toPlainMonthDay",
        flags: [],
        installer: None,
        native: "toPlainMonthDay",
    }
    TemporalPlainTimeConstructor {
        function: FunctionOrdinal(354) => BUILTIN_TEMPORAL_PLAIN_TIME_FUNCTION_ID,
        debug: "Temporal.PlainTime",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: TemporalPlainTime,
        native: "PlainTime",
    }
    TemporalPlainTimeFrom {
        function: FunctionOrdinal(355) => BUILTIN_TEMPORAL_PLAIN_TIME_FROM_FUNCTION_ID,
        debug: "Temporal.PlainTime.from",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "from",
    }
    TemporalPlainTimeCompare {
        function: FunctionOrdinal(356) => BUILTIN_TEMPORAL_PLAIN_TIME_COMPARE_FUNCTION_ID,
        debug: "Temporal.PlainTime.compare",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "compare",
    }
    TemporalPlainTimePrototypeHourGetter {
        function: FunctionOrdinal(357) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_HOUR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainTime.prototype.hour",
        flags: [],
        installer: None,
        native: "get hour",
    }
    TemporalPlainTimePrototypeMinuteGetter {
        function: FunctionOrdinal(358) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_MINUTE_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainTime.prototype.minute",
        flags: [],
        installer: None,
        native: "get minute",
    }
    TemporalPlainTimePrototypeSecondGetter {
        function: FunctionOrdinal(359) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_SECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainTime.prototype.second",
        flags: [],
        installer: None,
        native: "get second",
    }
    TemporalPlainTimePrototypeMillisecondGetter {
        function: FunctionOrdinal(360) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_MILLISECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainTime.prototype.millisecond",
        flags: [],
        installer: None,
        native: "get millisecond",
    }
    TemporalPlainTimePrototypeMicrosecondGetter {
        function: FunctionOrdinal(361) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_MICROSECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainTime.prototype.microsecond",
        flags: [],
        installer: None,
        native: "get microsecond",
    }
    TemporalPlainTimePrototypeNanosecondGetter {
        function: FunctionOrdinal(362) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_NANOSECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainTime.prototype.nanosecond",
        flags: [],
        installer: None,
        native: "get nanosecond",
    }
    TemporalPlainTimePrototypeWith {
        function: FunctionOrdinal(363) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_WITH_FUNCTION_ID,
        debug: "Temporal.PlainTime.prototype.with",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "with",
    }
    TemporalPlainTimePrototypeAdd {
        function: FunctionOrdinal(364) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_ADD_FUNCTION_ID,
        debug: "Temporal.PlainTime.prototype.add",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "add",
    }
    TemporalPlainTimePrototypeSubtract {
        function: FunctionOrdinal(365) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_SUBTRACT_FUNCTION_ID,
        debug: "Temporal.PlainTime.prototype.subtract",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "subtract",
    }
    TemporalPlainTimePrototypeUntil {
        function: FunctionOrdinal(366) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_UNTIL_FUNCTION_ID,
        debug: "Temporal.PlainTime.prototype.until",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "until",
    }
    TemporalPlainTimePrototypeSince {
        function: FunctionOrdinal(367) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_SINCE_FUNCTION_ID,
        debug: "Temporal.PlainTime.prototype.since",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "since",
    }
    TemporalPlainTimePrototypeRound {
        function: FunctionOrdinal(368) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_ROUND_FUNCTION_ID,
        debug: "Temporal.PlainTime.prototype.round",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "round",
    }
    TemporalPlainTimePrototypeEquals {
        function: FunctionOrdinal(369) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_EQUALS_FUNCTION_ID,
        debug: "Temporal.PlainTime.prototype.equals",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "equals",
    }
    TemporalPlainTimePrototypeToString {
        function: FunctionOrdinal(370) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Temporal.PlainTime.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toString",
    }
    TemporalPlainTimePrototypeToJson {
        function: FunctionOrdinal(371) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_TO_JSON_FUNCTION_ID,
        debug: "Temporal.PlainTime.prototype.toJSON",
        flags: [],
        installer: None,
        native: "toJSON",
    }
    TemporalPlainTimePrototypeToLocaleString {
        function: FunctionOrdinal(372) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID,
        debug: "Temporal.PlainTime.prototype.toLocaleString",
        flags: [INTL_HOST, SYSTEM_TIME_ZONE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toLocaleString",
    }
    TemporalPlainTimePrototypeValueOf {
        function: FunctionOrdinal(373) => BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_VALUE_OF_FUNCTION_ID,
        debug: "Temporal.PlainTime.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    TemporalPlainYearMonthConstructor {
        function: FunctionOrdinal(318) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: TemporalPlainYearMonth,
        native: "PlainYearMonth",
    }
    TemporalPlainYearMonthFrom {
        function: FunctionOrdinal(319) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_FROM_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.from",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "from",
    }
    TemporalPlainYearMonthCompare {
        function: FunctionOrdinal(320) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_COMPARE_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.compare",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "compare",
    }
    TemporalPlainYearMonthPrototypeCalendarIdGetter {
        function: FunctionOrdinal(321) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_CALENDAR_ID_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.calendarId",
        flags: [],
        installer: None,
        native: "get calendarId",
    }
    TemporalPlainYearMonthPrototypeEraGetter {
        function: FunctionOrdinal(322) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_ERA_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.era",
        flags: [],
        installer: None,
        native: "get era",
    }
    TemporalPlainYearMonthPrototypeEraYearGetter {
        function: FunctionOrdinal(323) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_ERA_YEAR_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.eraYear",
        flags: [],
        installer: None,
        native: "get eraYear",
    }
    TemporalPlainYearMonthPrototypeYearGetter {
        function: FunctionOrdinal(324) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_YEAR_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.year",
        flags: [],
        installer: None,
        native: "get year",
    }
    TemporalPlainYearMonthPrototypeMonthGetter {
        function: FunctionOrdinal(325) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_MONTH_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.month",
        flags: [],
        installer: None,
        native: "get month",
    }
    TemporalPlainYearMonthPrototypeMonthCodeGetter {
        function: FunctionOrdinal(326) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_MONTH_CODE_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.monthCode",
        flags: [],
        installer: None,
        native: "get monthCode",
    }
    TemporalPlainYearMonthPrototypeDaysInYearGetter {
        function: FunctionOrdinal(327) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_DAYS_IN_YEAR_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.daysInYear",
        flags: [],
        installer: None,
        native: "get daysInYear",
    }
    TemporalPlainYearMonthPrototypeDaysInMonthGetter {
        function: FunctionOrdinal(328) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_DAYS_IN_MONTH_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.daysInMonth",
        flags: [],
        installer: None,
        native: "get daysInMonth",
    }
    TemporalPlainYearMonthPrototypeMonthsInYearGetter {
        function: FunctionOrdinal(329) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_MONTHS_IN_YEAR_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.monthsInYear",
        flags: [],
        installer: None,
        native: "get monthsInYear",
    }
    TemporalPlainYearMonthPrototypeInLeapYearGetter {
        function: FunctionOrdinal(330) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_IN_LEAP_YEAR_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.inLeapYear",
        flags: [],
        installer: None,
        native: "get inLeapYear",
    }
    TemporalPlainYearMonthPrototypeWith {
        function: FunctionOrdinal(331) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_WITH_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.with",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "with",
    }
    TemporalPlainYearMonthPrototypeAdd {
        function: FunctionOrdinal(332) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_ADD_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.add",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "add",
    }
    TemporalPlainYearMonthPrototypeSubtract {
        function: FunctionOrdinal(333) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_SUBTRACT_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.subtract",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "subtract",
    }
    TemporalPlainYearMonthPrototypeUntil {
        function: FunctionOrdinal(334) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_UNTIL_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.until",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "until",
    }
    TemporalPlainYearMonthPrototypeSince {
        function: FunctionOrdinal(335) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_SINCE_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.since",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "since",
    }
    TemporalPlainYearMonthPrototypeEquals {
        function: FunctionOrdinal(336) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_EQUALS_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.equals",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "equals",
    }
    TemporalPlainYearMonthPrototypeToString {
        function: FunctionOrdinal(337) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toString",
    }
    TemporalPlainYearMonthPrototypeToJson {
        function: FunctionOrdinal(338) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_TO_JSON_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.toJSON",
        flags: [],
        installer: None,
        native: "toJSON",
    }
    TemporalPlainYearMonthPrototypeToLocaleString {
        function: FunctionOrdinal(339) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.toLocaleString",
        flags: [INTL_HOST, SYSTEM_TIME_ZONE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toLocaleString",
    }
    TemporalPlainYearMonthPrototypeValueOf {
        function: FunctionOrdinal(340) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_VALUE_OF_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    TemporalPlainYearMonthPrototypeToPlainDate {
        function: FunctionOrdinal(341) => BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_TO_PLAIN_DATE_FUNCTION_ID,
        debug: "Temporal.PlainYearMonth.prototype.toPlainDate",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toPlainDate",
    }
    TemporalPlainMonthDayConstructor {
        function: FunctionOrdinal(342) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: TemporalPlainMonthDay,
        native: "PlainMonthDay",
    }
    TemporalPlainMonthDayFrom {
        function: FunctionOrdinal(343) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_FROM_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay.from",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "from",
    }
    TemporalPlainMonthDayPrototypeCalendarIdGetter {
        function: FunctionOrdinal(344) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_CALENDAR_ID_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay.prototype.calendarId",
        flags: [],
        installer: None,
        native: "get calendarId",
    }
    TemporalPlainMonthDayPrototypeMonthCodeGetter {
        function: FunctionOrdinal(345) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_MONTH_CODE_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay.prototype.monthCode",
        flags: [],
        installer: None,
        native: "get monthCode",
    }
    TemporalPlainMonthDayPrototypeDayGetter {
        function: FunctionOrdinal(346) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_DAY_GETTER_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay.prototype.day",
        flags: [],
        installer: None,
        native: "get day",
    }
    TemporalPlainMonthDayPrototypeWith {
        function: FunctionOrdinal(347) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_WITH_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay.prototype.with",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "with",
    }
    TemporalPlainMonthDayPrototypeEquals {
        function: FunctionOrdinal(348) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_EQUALS_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay.prototype.equals",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "equals",
    }
    TemporalPlainMonthDayPrototypeToString {
        function: FunctionOrdinal(349) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toString",
    }
    TemporalPlainMonthDayPrototypeToJson {
        function: FunctionOrdinal(350) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_TO_JSON_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay.prototype.toJSON",
        flags: [],
        installer: None,
        native: "toJSON",
    }
    TemporalPlainMonthDayPrototypeToLocaleString {
        function: FunctionOrdinal(351) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay.prototype.toLocaleString",
        flags: [INTL_HOST, SYSTEM_TIME_ZONE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toLocaleString",
    }
    TemporalPlainMonthDayPrototypeValueOf {
        function: FunctionOrdinal(352) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_VALUE_OF_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    TemporalPlainMonthDayPrototypeToPlainDate {
        function: FunctionOrdinal(353) => BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_TO_PLAIN_DATE_FUNCTION_ID,
        debug: "Temporal.PlainMonthDay.prototype.toPlainDate",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toPlainDate",
    }
    TemporalPlainDateTimeConstructor {
        function: FunctionOrdinal(374) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_FUNCTION_ID,
        debug: "Temporal.PlainDateTime",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: TemporalPlainDateTime,
        native: "PlainDateTime",
    }
    TemporalPlainDateTimeFrom {
        function: FunctionOrdinal(375) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_FROM_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.from",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "from",
    }
    TemporalPlainDateTimeCompare {
        function: FunctionOrdinal(376) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_COMPARE_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.compare",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "compare",
    }
    TemporalPlainDateTimePrototypeCalendarIdGetter {
        function: FunctionOrdinal(377) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_CALENDAR_ID_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.calendarId",
        flags: [],
        installer: None,
        native: "get calendarId",
    }
    TemporalPlainDateTimePrototypeEraGetter {
        function: FunctionOrdinal(378) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_ERA_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.era",
        flags: [],
        installer: None,
        native: "get era",
    }
    TemporalPlainDateTimePrototypeEraYearGetter {
        function: FunctionOrdinal(379) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_ERA_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.eraYear",
        flags: [],
        installer: None,
        native: "get eraYear",
    }
    TemporalPlainDateTimePrototypeYearGetter {
        function: FunctionOrdinal(380) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.year",
        flags: [],
        installer: None,
        native: "get year",
    }
    TemporalPlainDateTimePrototypeMonthGetter {
        function: FunctionOrdinal(381) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MONTH_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.month",
        flags: [],
        installer: None,
        native: "get month",
    }
    TemporalPlainDateTimePrototypeMonthCodeGetter {
        function: FunctionOrdinal(382) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MONTH_CODE_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.monthCode",
        flags: [],
        installer: None,
        native: "get monthCode",
    }
    TemporalPlainDateTimePrototypeDayGetter {
        function: FunctionOrdinal(383) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAY_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.day",
        flags: [],
        installer: None,
        native: "get day",
    }
    TemporalPlainDateTimePrototypeHourGetter {
        function: FunctionOrdinal(384) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_HOUR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.hour",
        flags: [],
        installer: None,
        native: "get hour",
    }
    TemporalPlainDateTimePrototypeMinuteGetter {
        function: FunctionOrdinal(385) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MINUTE_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.minute",
        flags: [],
        installer: None,
        native: "get minute",
    }
    TemporalPlainDateTimePrototypeSecondGetter {
        function: FunctionOrdinal(386) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_SECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.second",
        flags: [],
        installer: None,
        native: "get second",
    }
    TemporalPlainDateTimePrototypeMillisecondGetter {
        function: FunctionOrdinal(387) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MILLISECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.millisecond",
        flags: [],
        installer: None,
        native: "get millisecond",
    }
    TemporalPlainDateTimePrototypeMicrosecondGetter {
        function: FunctionOrdinal(388) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MICROSECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.microsecond",
        flags: [],
        installer: None,
        native: "get microsecond",
    }
    TemporalPlainDateTimePrototypeNanosecondGetter {
        function: FunctionOrdinal(389) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_NANOSECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.nanosecond",
        flags: [],
        installer: None,
        native: "get nanosecond",
    }
    TemporalPlainDateTimePrototypeDayOfWeekGetter {
        function: FunctionOrdinal(390) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAY_OF_WEEK_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.dayOfWeek",
        flags: [],
        installer: None,
        native: "get dayOfWeek",
    }
    TemporalPlainDateTimePrototypeDayOfYearGetter {
        function: FunctionOrdinal(391) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAY_OF_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.dayOfYear",
        flags: [],
        installer: None,
        native: "get dayOfYear",
    }
    TemporalPlainDateTimePrototypeWeekOfYearGetter {
        function: FunctionOrdinal(392) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_WEEK_OF_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.weekOfYear",
        flags: [],
        installer: None,
        native: "get weekOfYear",
    }
    TemporalPlainDateTimePrototypeYearOfWeekGetter {
        function: FunctionOrdinal(393) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_YEAR_OF_WEEK_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.yearOfWeek",
        flags: [],
        installer: None,
        native: "get yearOfWeek",
    }
    TemporalPlainDateTimePrototypeDaysInWeekGetter {
        function: FunctionOrdinal(394) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAYS_IN_WEEK_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.daysInWeek",
        flags: [],
        installer: None,
        native: "get daysInWeek",
    }
    TemporalPlainDateTimePrototypeDaysInMonthGetter {
        function: FunctionOrdinal(395) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAYS_IN_MONTH_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.daysInMonth",
        flags: [],
        installer: None,
        native: "get daysInMonth",
    }
    TemporalPlainDateTimePrototypeDaysInYearGetter {
        function: FunctionOrdinal(396) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAYS_IN_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.daysInYear",
        flags: [],
        installer: None,
        native: "get daysInYear",
    }
    TemporalPlainDateTimePrototypeMonthsInYearGetter {
        function: FunctionOrdinal(397) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MONTHS_IN_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.monthsInYear",
        flags: [],
        installer: None,
        native: "get monthsInYear",
    }
    TemporalPlainDateTimePrototypeInLeapYearGetter {
        function: FunctionOrdinal(398) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_IN_LEAP_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.PlainDateTime.prototype.inLeapYear",
        flags: [],
        installer: None,
        native: "get inLeapYear",
    }
    TemporalPlainDateTimePrototypeWith {
        function: FunctionOrdinal(399) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_WITH_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.with",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "with",
    }
    TemporalPlainDateTimePrototypeWithPlainTime {
        function: FunctionOrdinal(400) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_WITH_PLAIN_TIME_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.withPlainTime",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "withPlainTime",
    }
    TemporalPlainDateTimePrototypeWithCalendar {
        function: FunctionOrdinal(401) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_WITH_CALENDAR_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.withCalendar",
        flags: [],
        installer: None,
        native: "withCalendar",
    }
    TemporalPlainDateTimePrototypeAdd {
        function: FunctionOrdinal(402) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_ADD_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.add",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "add",
    }
    TemporalPlainDateTimePrototypeSubtract {
        function: FunctionOrdinal(403) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_SUBTRACT_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.subtract",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "subtract",
    }
    TemporalPlainDateTimePrototypeUntil {
        function: FunctionOrdinal(404) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_UNTIL_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.until",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "until",
    }
    TemporalPlainDateTimePrototypeSince {
        function: FunctionOrdinal(405) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_SINCE_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.since",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "since",
    }
    TemporalPlainDateTimePrototypeRound {
        function: FunctionOrdinal(406) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_ROUND_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.round",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "round",
    }
    TemporalPlainDateTimePrototypeEquals {
        function: FunctionOrdinal(407) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_EQUALS_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.equals",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "equals",
    }
    TemporalPlainDateTimePrototypeToString {
        function: FunctionOrdinal(408) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toString",
    }
    TemporalPlainDateTimePrototypeToJson {
        function: FunctionOrdinal(409) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_JSON_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.toJSON",
        flags: [],
        installer: None,
        native: "toJSON",
    }
    TemporalPlainDateTimePrototypeToLocaleString {
        function: FunctionOrdinal(410) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.toLocaleString",
        flags: [INTL_HOST, SYSTEM_TIME_ZONE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toLocaleString",
    }
    TemporalPlainDateTimePrototypeValueOf {
        function: FunctionOrdinal(411) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_VALUE_OF_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    TemporalPlainDateTimePrototypeToPlainDate {
        function: FunctionOrdinal(412) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_PLAIN_DATE_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.toPlainDate",
        flags: [],
        installer: None,
        native: "toPlainDate",
    }
    TemporalPlainDateTimePrototypeToPlainTime {
        function: FunctionOrdinal(413) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_PLAIN_TIME_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.toPlainTime",
        flags: [],
        installer: None,
        native: "toPlainTime",
    }
    TemporalPlainDateTimePrototypeToZonedDateTime {
        function: FunctionOrdinal(414) => BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_ZONED_DATE_TIME_FUNCTION_ID,
        debug: "Temporal.PlainDateTime.prototype.toZonedDateTime",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toZonedDateTime",
    }
    TemporalDurationConstructor {
        function: FunctionOrdinal(415) => BUILTIN_TEMPORAL_DURATION_FUNCTION_ID,
        debug: "Temporal.Duration",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: TemporalDuration,
        native: "Duration",
    }
    TemporalDurationFrom {
        function: FunctionOrdinal(416) => BUILTIN_TEMPORAL_DURATION_FROM_FUNCTION_ID,
        debug: "Temporal.Duration.from",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "from",
    }
    TemporalDurationCompare {
        function: FunctionOrdinal(417) => BUILTIN_TEMPORAL_DURATION_COMPARE_FUNCTION_ID,
        debug: "Temporal.Duration.compare",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "compare",
    }
    TemporalDurationPrototypeYearsGetter {
        function: FunctionOrdinal(418) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_YEARS_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.years",
        flags: [],
        installer: None,
        native: "get years",
    }
    TemporalDurationPrototypeMonthsGetter {
        function: FunctionOrdinal(419) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_MONTHS_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.months",
        flags: [],
        installer: None,
        native: "get months",
    }
    TemporalDurationPrototypeWeeksGetter {
        function: FunctionOrdinal(420) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_WEEKS_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.weeks",
        flags: [],
        installer: None,
        native: "get weeks",
    }
    TemporalDurationPrototypeDaysGetter {
        function: FunctionOrdinal(421) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_DAYS_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.days",
        flags: [],
        installer: None,
        native: "get days",
    }
    TemporalDurationPrototypeHoursGetter {
        function: FunctionOrdinal(422) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_HOURS_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.hours",
        flags: [],
        installer: None,
        native: "get hours",
    }
    TemporalDurationPrototypeMinutesGetter {
        function: FunctionOrdinal(423) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_MINUTES_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.minutes",
        flags: [],
        installer: None,
        native: "get minutes",
    }
    TemporalDurationPrototypeSecondsGetter {
        function: FunctionOrdinal(424) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_SECONDS_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.seconds",
        flags: [],
        installer: None,
        native: "get seconds",
    }
    TemporalDurationPrototypeMillisecondsGetter {
        function: FunctionOrdinal(425) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_MILLISECONDS_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.milliseconds",
        flags: [],
        installer: None,
        native: "get milliseconds",
    }
    TemporalDurationPrototypeMicrosecondsGetter {
        function: FunctionOrdinal(426) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_MICROSECONDS_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.microseconds",
        flags: [],
        installer: None,
        native: "get microseconds",
    }
    TemporalDurationPrototypeNanosecondsGetter {
        function: FunctionOrdinal(427) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_NANOSECONDS_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.nanoseconds",
        flags: [],
        installer: None,
        native: "get nanoseconds",
    }
    TemporalDurationPrototypeSignGetter {
        function: FunctionOrdinal(428) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_SIGN_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.sign",
        flags: [],
        installer: None,
        native: "get sign",
    }
    TemporalDurationPrototypeBlankGetter {
        function: FunctionOrdinal(429) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_BLANK_GETTER_FUNCTION_ID,
        debug: "get Temporal.Duration.prototype.blank",
        flags: [],
        installer: None,
        native: "get blank",
    }
    TemporalDurationPrototypeWith {
        function: FunctionOrdinal(430) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_WITH_FUNCTION_ID,
        debug: "Temporal.Duration.prototype.with",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "with",
    }
    TemporalDurationPrototypeNegated {
        function: FunctionOrdinal(431) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_NEGATED_FUNCTION_ID,
        debug: "Temporal.Duration.prototype.negated",
        flags: [],
        installer: None,
        native: "negated",
    }
    TemporalDurationPrototypeAbs {
        function: FunctionOrdinal(432) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_ABS_FUNCTION_ID,
        debug: "Temporal.Duration.prototype.abs",
        flags: [],
        installer: None,
        native: "abs",
    }
    TemporalDurationPrototypeAdd {
        function: FunctionOrdinal(433) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_ADD_FUNCTION_ID,
        debug: "Temporal.Duration.prototype.add",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "add",
    }
    TemporalDurationPrototypeSubtract {
        function: FunctionOrdinal(434) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_SUBTRACT_FUNCTION_ID,
        debug: "Temporal.Duration.prototype.subtract",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "subtract",
    }
    TemporalDurationPrototypeRound {
        function: FunctionOrdinal(435) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_ROUND_FUNCTION_ID,
        debug: "Temporal.Duration.prototype.round",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "round",
    }
    TemporalDurationPrototypeTotal {
        function: FunctionOrdinal(436) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_TOTAL_FUNCTION_ID,
        debug: "Temporal.Duration.prototype.total",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "total",
    }
    TemporalDurationPrototypeToString {
        function: FunctionOrdinal(437) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Temporal.Duration.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toString",
    }
    TemporalDurationPrototypeToJson {
        function: FunctionOrdinal(438) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_TO_JSON_FUNCTION_ID,
        debug: "Temporal.Duration.prototype.toJSON",
        flags: [],
        installer: None,
        native: "toJSON",
    }
    TemporalDurationPrototypeToLocaleString {
        function: FunctionOrdinal(439) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID,
        debug: "Temporal.Duration.prototype.toLocaleString",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toLocaleString",
    }
    TemporalDurationPrototypeValueOf {
        function: FunctionOrdinal(440) => BUILTIN_TEMPORAL_DURATION_PROTOTYPE_VALUE_OF_FUNCTION_ID,
        debug: "Temporal.Duration.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    TemporalNowInstant {
        function: FunctionOrdinal(441) => BUILTIN_TEMPORAL_NOW_INSTANT_FUNCTION_ID,
        debug: "Temporal.Now.instant",
        flags: [WALL_CLOCK],
        installer: None,
        native: "instant",
    }
    TemporalNowTimeZoneId {
        function: FunctionOrdinal(442) => BUILTIN_TEMPORAL_NOW_TIME_ZONE_ID_FUNCTION_ID,
        debug: "Temporal.Now.timeZoneId",
        flags: [SYSTEM_TIME_ZONE],
        installer: None,
        native: "timeZoneId",
    }
    TemporalNowZonedDateTimeIso {
        function: FunctionOrdinal(443) => BUILTIN_TEMPORAL_NOW_ZONED_DATE_TIME_ISO_FUNCTION_ID,
        debug: "Temporal.Now.zonedDateTimeISO",
        flags: [WALL_CLOCK, INTL_HOST, SYSTEM_TIME_ZONE],
        installer: None,
        native: "zonedDateTimeISO",
    }
    TemporalNowPlainDateTimeIso {
        function: FunctionOrdinal(845) => "$builtin.Temporal.Now.plainDateTimeISO",
        debug: "Temporal.Now.plainDateTimeISO",
        flags: [WALL_CLOCK, INTL_HOST, SYSTEM_TIME_ZONE],
        installer: None,
        native: "plainDateTimeISO",
    }
    TemporalNowPlainDateIso {
        function: FunctionOrdinal(846) => "$builtin.Temporal.Now.plainDateISO",
        debug: "Temporal.Now.plainDateISO",
        flags: [WALL_CLOCK, INTL_HOST, SYSTEM_TIME_ZONE],
        installer: None,
        native: "plainDateISO",
    }
    TemporalNowPlainTimeIso {
        function: FunctionOrdinal(847) => "$builtin.Temporal.Now.plainTimeISO",
        debug: "Temporal.Now.plainTimeISO",
        flags: [WALL_CLOCK, INTL_HOST, SYSTEM_TIME_ZONE],
        installer: None,
        native: "plainTimeISO",
    }
    TemporalInstantConstructor {
        function: FunctionOrdinal(444) => BUILTIN_TEMPORAL_INSTANT_FUNCTION_ID,
        debug: "Temporal.Instant",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: TemporalInstant,
        native: TEMPORAL_INSTANT_NAME,
    }
    TemporalInstantFrom {
        function: FunctionOrdinal(445) => BUILTIN_TEMPORAL_INSTANT_FROM_FUNCTION_ID,
        debug: "Temporal.Instant.from",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "from",
    }
    TemporalInstantCompare {
        function: FunctionOrdinal(446) => BUILTIN_TEMPORAL_INSTANT_COMPARE_FUNCTION_ID,
        debug: "Temporal.Instant.compare",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "compare",
    }
    TemporalInstantFromEpochMilliseconds {
        function: FunctionOrdinal(447) => BUILTIN_TEMPORAL_INSTANT_FROM_EPOCH_MILLISECONDS_FUNCTION_ID,
        debug: "Temporal.Instant.fromEpochMilliseconds",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "fromEpochMilliseconds",
    }
    TemporalInstantFromEpochNanoseconds {
        function: FunctionOrdinal(448) => BUILTIN_TEMPORAL_INSTANT_FROM_EPOCH_NANOSECONDS_FUNCTION_ID,
        debug: "Temporal.Instant.fromEpochNanoseconds",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "fromEpochNanoseconds",
    }
    TemporalInstantPrototypeEpochMillisecondsGetter {
        function: FunctionOrdinal(449) => BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_EPOCH_MILLISECONDS_GETTER_FUNCTION_ID,
        debug: "get Temporal.Instant.prototype.epochMilliseconds",
        flags: [],
        installer: None,
        native: "get epochMilliseconds",
    }
    TemporalInstantPrototypeEpochNanosecondsGetter {
        function: FunctionOrdinal(450) => BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_EPOCH_NANOSECONDS_GETTER_FUNCTION_ID,
        debug: "get Temporal.Instant.prototype.epochNanoseconds",
        flags: [],
        installer: None,
        native: "get epochNanoseconds",
    }
    TemporalInstantPrototypeAdd {
        function: FunctionOrdinal(826) => "$builtin.Temporal.Instant.prototype.add",
        debug: "Temporal.Instant.prototype.add",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "add",
    }
    TemporalInstantPrototypeSubtract {
        function: FunctionOrdinal(827) => "$builtin.Temporal.Instant.prototype.subtract",
        debug: "Temporal.Instant.prototype.subtract",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "subtract",
    }
    TemporalInstantPrototypeRound {
        function: FunctionOrdinal(828) => "$builtin.Temporal.Instant.prototype.round",
        debug: "Temporal.Instant.prototype.round",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "round",
    }
    TemporalInstantPrototypeUntil {
        function: FunctionOrdinal(829) => "$builtin.Temporal.Instant.prototype.until",
        debug: "Temporal.Instant.prototype.until",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "until",
    }
    TemporalInstantPrototypeSince {
        function: FunctionOrdinal(830) => "$builtin.Temporal.Instant.prototype.since",
        debug: "Temporal.Instant.prototype.since",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "since",
    }
    TemporalInstantPrototypeEquals {
        function: FunctionOrdinal(451) => BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_EQUALS_FUNCTION_ID,
        debug: "Temporal.Instant.prototype.equals",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "equals",
    }
    TemporalInstantPrototypeToString {
        function: FunctionOrdinal(452) => BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Temporal.Instant.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE, INTL_HOST],
        installer: None,
        native: "toString",
    }
    TemporalInstantPrototypeToLocaleString {
        function: FunctionOrdinal(839) => "$builtin.Temporal.Instant.prototype.toLocaleString",
        debug: "Temporal.Instant.prototype.toLocaleString",
        flags: [SYNCHRONOUS_USER_CODE, INTL_HOST, SYSTEM_TIME_ZONE],
        installer: None,
        native: "toLocaleString",
    }
    TemporalInstantPrototypeToJson {
        function: FunctionOrdinal(453) => BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_TO_JSON_FUNCTION_ID,
        debug: "Temporal.Instant.prototype.toJSON",
        flags: [INTL_HOST],
        installer: None,
        native: "toJSON",
    }
    TemporalInstantPrototypeValueOf {
        function: FunctionOrdinal(454) => BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_VALUE_OF_FUNCTION_ID,
        debug: "Temporal.Instant.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    TemporalInstantPrototypeToZonedDateTimeIso {
        function: FunctionOrdinal(848) => "$builtin.Temporal.Instant.prototype.toZonedDateTimeISO",
        debug: "Temporal.Instant.prototype.toZonedDateTimeISO",
        flags: [INTL_HOST],
        installer: None,
        native: "toZonedDateTimeISO",
    }
    TemporalZonedDateTimeConstructor {
        function: FunctionOrdinal(455) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime",
        flags: [CONSTRUCTABLE, INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: TemporalZonedDateTime,
        native: "ZonedDateTime",
    }
    TemporalZonedDateTimeFrom {
        function: FunctionOrdinal(456) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_FROM_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.from",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "from",
    }
    TemporalZonedDateTimePrototypeEpochMillisecondsGetter {
        function: FunctionOrdinal(457) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_EPOCH_MILLISECONDS_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.epochMilliseconds",
        flags: [],
        installer: None,
        native: "get epochMilliseconds",
    }
    TemporalZonedDateTimePrototypeEpochNanosecondsGetter {
        function: FunctionOrdinal(458) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_EPOCH_NANOSECONDS_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.epochNanoseconds",
        flags: [],
        installer: None,
        native: "get epochNanoseconds",
    }
    TemporalZonedDateTimePrototypeOffsetGetter {
        function: FunctionOrdinal(459) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_OFFSET_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.offset",
        flags: [INTL_HOST],
        installer: None,
        native: "get offset",
    }
    TemporalZonedDateTimePrototypeOffsetNanosecondsGetter {
        function: FunctionOrdinal(460) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_OFFSET_NANOSECONDS_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.offsetNanoseconds",
        flags: [INTL_HOST],
        installer: None,
        native: "get offsetNanoseconds",
    }
    TemporalZonedDateTimePrototypeTimeZoneIdGetter {
        function: FunctionOrdinal(461) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_TIME_ZONE_ID_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.timeZoneId",
        flags: [],
        installer: None,
        native: "get timeZoneId",
    }
    TemporalZonedDateTimePrototypeCalendarIdGetter {
        function: FunctionOrdinal(462) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_CALENDAR_ID_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.calendarId",
        flags: [],
        installer: None,
        native: "get calendarId",
    }
    TemporalZonedDateTimePrototypeEraGetter {
        function: FunctionOrdinal(463) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_ERA_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.era",
        flags: [INTL_HOST],
        installer: None,
        native: "get era",
    }
    TemporalZonedDateTimePrototypeEraYearGetter {
        function: FunctionOrdinal(464) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_ERA_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.eraYear",
        flags: [INTL_HOST],
        installer: None,
        native: "get eraYear",
    }
    TemporalZonedDateTimePrototypeYearGetter {
        function: FunctionOrdinal(465) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.year",
        flags: [INTL_HOST],
        installer: None,
        native: "get year",
    }
    TemporalZonedDateTimePrototypeMonthGetter {
        function: FunctionOrdinal(466) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MONTH_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.month",
        flags: [INTL_HOST],
        installer: None,
        native: "get month",
    }
    TemporalZonedDateTimePrototypeMonthCodeGetter {
        function: FunctionOrdinal(467) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MONTH_CODE_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.monthCode",
        flags: [INTL_HOST],
        installer: None,
        native: "get monthCode",
    }
    TemporalZonedDateTimePrototypeDayGetter {
        function: FunctionOrdinal(468) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAY_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.day",
        flags: [INTL_HOST],
        installer: None,
        native: "get day",
    }
    TemporalZonedDateTimePrototypeHourGetter {
        function: FunctionOrdinal(469) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_HOUR_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.hour",
        flags: [INTL_HOST],
        installer: None,
        native: "get hour",
    }
    TemporalZonedDateTimePrototypeMinuteGetter {
        function: FunctionOrdinal(470) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MINUTE_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.minute",
        flags: [INTL_HOST],
        installer: None,
        native: "get minute",
    }
    TemporalZonedDateTimePrototypeSecondGetter {
        function: FunctionOrdinal(471) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_SECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.second",
        flags: [INTL_HOST],
        installer: None,
        native: "get second",
    }
    TemporalZonedDateTimePrototypeMillisecondGetter {
        function: FunctionOrdinal(472) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MILLISECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.millisecond",
        flags: [INTL_HOST],
        installer: None,
        native: "get millisecond",
    }
    TemporalZonedDateTimePrototypeMicrosecondGetter {
        function: FunctionOrdinal(473) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MICROSECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.microsecond",
        flags: [INTL_HOST],
        installer: None,
        native: "get microsecond",
    }
    TemporalZonedDateTimePrototypeNanosecondGetter {
        function: FunctionOrdinal(474) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_NANOSECOND_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.nanosecond",
        flags: [INTL_HOST],
        installer: None,
        native: "get nanosecond",
    }
    TemporalZonedDateTimePrototypeEquals {
        function: FunctionOrdinal(475) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_EQUALS_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.equals",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "equals",
    }
    TemporalZonedDateTimePrototypeToInstant {
        function: FunctionOrdinal(476) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_TO_INSTANT_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.toInstant",
        flags: [],
        installer: None,
        native: "toInstant",
    }
    TemporalZonedDateTimePrototypeToPlainDateTime {
        function: FunctionOrdinal(477) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_TO_PLAIN_DATE_TIME_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.toPlainDateTime",
        flags: [INTL_HOST],
        installer: None,
        native: "toPlainDateTime",
    }
    TemporalZonedDateTimePrototypeWithTimeZone {
        function: FunctionOrdinal(478) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_WITH_TIME_ZONE_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.withTimeZone",
        flags: [INTL_HOST],
        installer: None,
        native: "withTimeZone",
    }
    TemporalZonedDateTimePrototypeWithCalendar {
        function: FunctionOrdinal(479) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_WITH_CALENDAR_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.withCalendar",
        flags: [],
        installer: None,
        native: "withCalendar",
    }
    TemporalZonedDateTimePrototypeAdd {
        function: FunctionOrdinal(480) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_ADD_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.add",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "add",
    }
    TemporalZonedDateTimePrototypeSubtract {
        function: FunctionOrdinal(481) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_SUBTRACT_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.subtract",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "subtract",
    }
    TemporalZonedDateTimePrototypeUntil {
        function: FunctionOrdinal(482) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_UNTIL_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.until",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "until",
    }
    TemporalZonedDateTimePrototypeSince {
        function: FunctionOrdinal(483) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_SINCE_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.since",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "since",
    }
    IntlGetCanonicalLocales {
        function: FunctionOrdinal(484) => BUILTIN_INTL_GET_CANONICAL_LOCALES_FUNCTION_ID,
        debug: "Intl.getCanonicalLocales",
        flags: [INTL_HOST],
        installer: None,
        native: "getCanonicalLocales",
    }
    IntlLocaleConstructor {
        function: FunctionOrdinal(485) => BUILTIN_INTL_LOCALE_FUNCTION_ID,
        debug: "Intl.Locale",
        flags: [CONSTRUCTABLE, INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: IntlLocale,
        native: INTL_LOCALE_NAME,
    }
    IntlLocalePrototypeLanguageGetter {
        function: FunctionOrdinal(486) => BUILTIN_INTL_LOCALE_PROTOTYPE_LANGUAGE_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.language",
        flags: [],
        installer: None,
        native: "get language",
    }
    IntlLocalePrototypeScriptGetter {
        function: FunctionOrdinal(487) => BUILTIN_INTL_LOCALE_PROTOTYPE_SCRIPT_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.script",
        flags: [],
        installer: None,
        native: "get script",
    }
    IntlLocalePrototypeRegionGetter {
        function: FunctionOrdinal(488) => BUILTIN_INTL_LOCALE_PROTOTYPE_REGION_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.region",
        flags: [],
        installer: None,
        native: "get region",
    }
    IntlLocalePrototypeBaseNameGetter {
        function: FunctionOrdinal(489) => BUILTIN_INTL_LOCALE_PROTOTYPE_BASE_NAME_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.baseName",
        flags: [],
        installer: None,
        native: "get baseName",
    }
    IntlLocalePrototypeCalendarGetter {
        function: FunctionOrdinal(813) => BUILTIN_INTL_LOCALE_PROTOTYPE_CALENDAR_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.calendar",
        flags: [],
        installer: None,
        native: "get calendar",
    }
    IntlLocalePrototypeCollationGetter {
        function: FunctionOrdinal(814) => BUILTIN_INTL_LOCALE_PROTOTYPE_COLLATION_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.collation",
        flags: [],
        installer: None,
        native: "get collation",
    }
    IntlLocalePrototypeFirstDayOfWeekGetter {
        function: FunctionOrdinal(815) => BUILTIN_INTL_LOCALE_PROTOTYPE_FIRST_DAY_OF_WEEK_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.firstDayOfWeek",
        flags: [],
        installer: None,
        native: "get firstDayOfWeek",
    }
    IntlLocalePrototypeHourCycleGetter {
        function: FunctionOrdinal(816) => BUILTIN_INTL_LOCALE_PROTOTYPE_HOUR_CYCLE_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.hourCycle",
        flags: [],
        installer: None,
        native: "get hourCycle",
    }
    IntlLocalePrototypeCaseFirstGetter {
        function: FunctionOrdinal(817) => BUILTIN_INTL_LOCALE_PROTOTYPE_CASE_FIRST_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.caseFirst",
        flags: [],
        installer: None,
        native: "get caseFirst",
    }
    IntlLocalePrototypeNumericGetter {
        function: FunctionOrdinal(818) => BUILTIN_INTL_LOCALE_PROTOTYPE_NUMERIC_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.numeric",
        flags: [],
        installer: None,
        native: "get numeric",
    }
    IntlLocalePrototypeNumberingSystemGetter {
        function: FunctionOrdinal(819) => BUILTIN_INTL_LOCALE_PROTOTYPE_NUMBERING_SYSTEM_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.numberingSystem",
        flags: [],
        installer: None,
        native: "get numberingSystem",
    }
    IntlLocalePrototypeVariantsGetter {
        function: FunctionOrdinal(820) => BUILTIN_INTL_LOCALE_PROTOTYPE_VARIANTS_GETTER_FUNCTION_ID,
        debug: "get Intl.Locale.prototype.variants",
        flags: [],
        installer: None,
        native: "get variants",
    }
    IntlLocalePrototypeMaximize {
        function: FunctionOrdinal(822) => "$builtin.Intl.Locale.prototype.maximize",
        debug: "Intl.Locale.prototype.maximize",
        flags: [INTL_HOST],
        installer: None,
        native: "maximize",
    }
    IntlLocalePrototypeMinimize {
        function: FunctionOrdinal(823) => "$builtin.Intl.Locale.prototype.minimize",
        debug: "Intl.Locale.prototype.minimize",
        flags: [INTL_HOST],
        installer: None,
        native: "minimize",
    }
    IntlLocalePrototypeGetCalendars {
        function: FunctionOrdinal(890) => "$builtin.Intl.Locale.prototype.getCalendars",
        debug: "Intl.Locale.prototype.getCalendars",
        flags: [INTL_HOST],
        installer: None,
        native: "getCalendars",
    }
    IntlLocalePrototypeGetCollations {
        function: FunctionOrdinal(891) => "$builtin.Intl.Locale.prototype.getCollations",
        debug: "Intl.Locale.prototype.getCollations",
        flags: [INTL_HOST],
        installer: None,
        native: "getCollations",
    }
    IntlLocalePrototypeGetTimeZones {
        function: FunctionOrdinal(892) => "$builtin.Intl.Locale.prototype.getTimeZones",
        debug: "Intl.Locale.prototype.getTimeZones",
        flags: [INTL_HOST],
        installer: None,
        native: "getTimeZones",
    }
    IntlLocalePrototypeGetNumberingSystems {
        function: FunctionOrdinal(889) => "$builtin.Intl.Locale.prototype.getNumberingSystems",
        debug: "Intl.Locale.prototype.getNumberingSystems",
        flags: [INTL_HOST],
        installer: None,
        native: "getNumberingSystems",
    }
    IntlLocalePrototypeGetHourCycles {
        function: FunctionOrdinal(888) => "$builtin.Intl.Locale.prototype.getHourCycles",
        debug: "Intl.Locale.prototype.getHourCycles",
        flags: [INTL_HOST],
        installer: None,
        native: "getHourCycles",
    }
    IntlLocalePrototypeGetTextInfo {
        function: FunctionOrdinal(887) => "$builtin.Intl.Locale.prototype.getTextInfo",
        debug: "Intl.Locale.prototype.getTextInfo",
        flags: [INTL_HOST],
        installer: None,
        native: "getTextInfo",
    }
    IntlLocalePrototypeGetWeekInfo {
        function: FunctionOrdinal(886) => "$builtin.Intl.Locale.prototype.getWeekInfo",
        debug: "Intl.Locale.prototype.getWeekInfo",
        flags: [INTL_HOST],
        installer: None,
        native: "getWeekInfo",
    }
    IntlLocalePrototypeToString {
        function: FunctionOrdinal(490) => BUILTIN_INTL_LOCALE_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Intl.Locale.prototype.toString",
        flags: [],
        installer: None,
        native: "toString",
    }
    IntlDateTimeFormatConstructor {
        function: FunctionOrdinal(491) => BUILTIN_INTL_DATE_TIME_FORMAT_FUNCTION_ID,
        debug: "Intl.DateTimeFormat",
        flags: [CONSTRUCTABLE, INTL_HOST, SYSTEM_TIME_ZONE, SYNCHRONOUS_USER_CODE],
        installer: IntlDateTimeFormat,
        native: INTL_DATE_TIME_FORMAT_NAME,
    }
    IntlDateTimeFormatSupportedLocalesOf {
        function: FunctionOrdinal(492) => BUILTIN_INTL_DATE_TIME_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID,
        debug: "Intl.DateTimeFormat.supportedLocalesOf",
        flags: [INTL_HOST],
        installer: None,
        native: "supportedLocalesOf",
    }
    IntlDateTimeFormatPrototypeResolvedOptions {
        function: FunctionOrdinal(493) => BUILTIN_INTL_DATE_TIME_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
        debug: "Intl.DateTimeFormat.prototype.resolvedOptions",
        flags: [],
        installer: None,
        native: "resolvedOptions",
    }
    IntlDateTimeFormatPrototypeFormatGetter {
        function: FunctionOrdinal(494) => BUILTIN_INTL_DATE_TIME_FORMAT_PROTOTYPE_FORMAT_GETTER_FUNCTION_ID,
        debug: "get Intl.DateTimeFormat.prototype.format",
        flags: [],
        installer: None,
        native: "get format",
    }
    IntlDateTimeFormatPrototypeFormatToParts {
        function: FunctionOrdinal(495) => BUILTIN_INTL_DATE_TIME_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID,
        debug: "Intl.DateTimeFormat.prototype.formatToParts",
        flags: [WALL_CLOCK, INTL_HOST],
        installer: None,
        native: "formatToParts",
    }
    IntlDateTimeFormatPrototypeFormatRange {
        function: FunctionOrdinal(496) => BUILTIN_INTL_DATE_TIME_FORMAT_PROTOTYPE_FORMAT_RANGE_FUNCTION_ID,
        debug: "Intl.DateTimeFormat.prototype.formatRange",
        flags: [INTL_HOST],
        installer: None,
        native: "formatRange",
    }
    IntlDateTimeFormatPrototypeFormatRangeToParts {
        function: FunctionOrdinal(497) => BUILTIN_INTL_DATE_TIME_FORMAT_PROTOTYPE_FORMAT_RANGE_TO_PARTS_FUNCTION_ID,
        debug: "Intl.DateTimeFormat.prototype.formatRangeToParts",
        flags: [INTL_HOST],
        installer: None,
        native: "formatRangeToParts",
    }
    IntlDateTimeFormatBoundFormat {
        function: FunctionOrdinal(498) => BUILTIN_INTL_DATE_TIME_FORMAT_BOUND_FORMAT_FUNCTION_ID,
        debug: "Intl.DateTimeFormat Format Function",
        flags: [WALL_CLOCK, INTL_HOST],
        installer: None,
        native: "",
    }
    IntlNumberFormatConstructor {
        function: FunctionOrdinal(831) => BUILTIN_INTL_NUMBER_FORMAT_FUNCTION_ID,
        debug: "Intl.NumberFormat",
        flags: [CONSTRUCTABLE, INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: IntlNumberFormat,
        native: INTL_NUMBER_FORMAT_NAME,
    }
    IntlNumberFormatSupportedLocalesOf {
        function: FunctionOrdinal(832) => BUILTIN_INTL_NUMBER_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID,
        debug: "Intl.NumberFormat.supportedLocalesOf",
        flags: [INTL_HOST],
        installer: None,
        native: "supportedLocalesOf",
    }
    IntlNumberFormatPrototypeResolvedOptions {
        function: FunctionOrdinal(833) => BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
        debug: "Intl.NumberFormat.prototype.resolvedOptions",
        flags: [],
        installer: None,
        native: "resolvedOptions",
    }
    IntlNumberFormatPrototypeFormatGetter {
        function: FunctionOrdinal(834) => BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_GETTER_FUNCTION_ID,
        debug: "get Intl.NumberFormat.prototype.format",
        flags: [],
        installer: None,
        native: "get format",
    }
    IntlNumberFormatPrototypeFormatToParts {
        function: FunctionOrdinal(835) => BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID,
        debug: "Intl.NumberFormat.prototype.formatToParts",
        flags: [INTL_HOST],
        installer: None,
        native: "formatToParts",
    }
    IntlNumberFormatPrototypeFormatRange {
        function: FunctionOrdinal(836) => BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_RANGE_FUNCTION_ID,
        debug: "Intl.NumberFormat.prototype.formatRange",
        flags: [INTL_HOST],
        installer: None,
        native: "formatRange",
    }
    IntlNumberFormatPrototypeFormatRangeToParts {
        function: FunctionOrdinal(837) => BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_RANGE_TO_PARTS_FUNCTION_ID,
        debug: "Intl.NumberFormat.prototype.formatRangeToParts",
        flags: [INTL_HOST],
        installer: None,
        native: "formatRangeToParts",
    }
    IntlNumberFormatBoundFormat {
        function: FunctionOrdinal(838) => BUILTIN_INTL_NUMBER_FORMAT_BOUND_FORMAT_FUNCTION_ID,
        debug: "Intl.NumberFormat Format Function",
        flags: [INTL_HOST],
        installer: None,
        native: "",
    }
    IntlPluralRulesConstructor {
        function: FunctionOrdinal(850) => BUILTIN_INTL_PLURAL_RULES_FUNCTION_ID,
        debug: "Intl.PluralRules",
        flags: [CONSTRUCTABLE, INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: IntlPluralRules,
        native: INTL_PLURAL_RULES_NAME,
    }
    IntlPluralRulesSupportedLocalesOf {
        function: FunctionOrdinal(851) => BUILTIN_INTL_PLURAL_RULES_SUPPORTED_LOCALES_OF_FUNCTION_ID,
        debug: "Intl.PluralRules.supportedLocalesOf",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "supportedLocalesOf",
    }
    IntlPluralRulesPrototypeResolvedOptions {
        function: FunctionOrdinal(852) => BUILTIN_INTL_PLURAL_RULES_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
        debug: "Intl.PluralRules.prototype.resolvedOptions",
        flags: [],
        installer: None,
        native: "resolvedOptions",
    }
    IntlPluralRulesPrototypeSelect {
        function: FunctionOrdinal(853) => BUILTIN_INTL_PLURAL_RULES_PROTOTYPE_SELECT_FUNCTION_ID,
        debug: "Intl.PluralRules.prototype.select",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "select",
    }
    IntlPluralRulesPrototypeSelectRange {
        function: FunctionOrdinal(854) => BUILTIN_INTL_PLURAL_RULES_PROTOTYPE_SELECT_RANGE_FUNCTION_ID,
        debug: "Intl.PluralRules.prototype.selectRange",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "selectRange",
    }
    IntlListFormatConstructor {
        function: FunctionOrdinal(855) => BUILTIN_INTL_LIST_FORMAT_FUNCTION_ID,
        debug: "Intl.ListFormat",
        flags: [CONSTRUCTABLE, INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: IntlListFormat,
        native: INTL_LIST_FORMAT_NAME,
    }
    IntlListFormatSupportedLocalesOf {
        function: FunctionOrdinal(856) => BUILTIN_INTL_LIST_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID,
        debug: "Intl.ListFormat.supportedLocalesOf",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "supportedLocalesOf",
    }
    IntlListFormatPrototypeResolvedOptions {
        function: FunctionOrdinal(857) => BUILTIN_INTL_LIST_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
        debug: "Intl.ListFormat.prototype.resolvedOptions",
        flags: [],
        installer: None,
        native: "resolvedOptions",
    }
    IntlListFormatPrototypeFormat {
        function: FunctionOrdinal(858) => BUILTIN_INTL_LIST_FORMAT_PROTOTYPE_FORMAT_FUNCTION_ID,
        debug: "Intl.ListFormat.prototype.format",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "format",
    }
    IntlListFormatPrototypeFormatToParts {
        function: FunctionOrdinal(859) => BUILTIN_INTL_LIST_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID,
        debug: "Intl.ListFormat.prototype.formatToParts",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "formatToParts",
    }
    IntlCollatorConstructor {
        function: FunctionOrdinal(860) => BUILTIN_INTL_COLLATOR_FUNCTION_ID,
        debug: "Intl.Collator",
        flags: [CONSTRUCTABLE, INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: IntlCollator,
        native: INTL_COLLATOR_NAME,
    }
    IntlCollatorSupportedLocalesOf {
        function: FunctionOrdinal(861) => BUILTIN_INTL_COLLATOR_SUPPORTED_LOCALES_OF_FUNCTION_ID,
        debug: "Intl.Collator.supportedLocalesOf",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "supportedLocalesOf",
    }
    IntlCollatorPrototypeResolvedOptions {
        function: FunctionOrdinal(862) => BUILTIN_INTL_COLLATOR_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
        debug: "Intl.Collator.prototype.resolvedOptions",
        flags: [],
        installer: None,
        native: "resolvedOptions",
    }
    IntlCollatorPrototypeCompareGetter {
        function: FunctionOrdinal(863) => BUILTIN_INTL_COLLATOR_PROTOTYPE_COMPARE_GETTER_FUNCTION_ID,
        debug: "get Intl.Collator.prototype.compare",
        flags: [],
        installer: None,
        native: "get compare",
    }
    IntlCollatorBoundCompare {
        function: FunctionOrdinal(864) => BUILTIN_INTL_COLLATOR_BOUND_COMPARE_FUNCTION_ID,
        debug: "Intl.Collator Compare Function",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    IntlDisplayNamesConstructor {
        function: FunctionOrdinal(865) => BUILTIN_INTL_DISPLAY_NAMES_FUNCTION_ID,
        debug: "Intl.DisplayNames",
        flags: [CONSTRUCTABLE, INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: IntlDisplayNames,
        native: INTL_DISPLAY_NAMES_NAME,
    }
    IntlDisplayNamesSupportedLocalesOf {
        function: FunctionOrdinal(866) => BUILTIN_INTL_DISPLAY_NAMES_SUPPORTED_LOCALES_OF_FUNCTION_ID,
        debug: "Intl.DisplayNames.supportedLocalesOf",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "supportedLocalesOf",
    }
    IntlDisplayNamesPrototypeResolvedOptions {
        function: FunctionOrdinal(867) => BUILTIN_INTL_DISPLAY_NAMES_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
        debug: "Intl.DisplayNames.prototype.resolvedOptions",
        flags: [],
        installer: None,
        native: "resolvedOptions",
    }
    IntlDisplayNamesPrototypeOf {
        function: FunctionOrdinal(868) => BUILTIN_INTL_DISPLAY_NAMES_PROTOTYPE_OF_FUNCTION_ID,
        debug: "Intl.DisplayNames.prototype.of",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "of",
    }
    IntlRelativeTimeFormatConstructor {
        function: FunctionOrdinal(869) => BUILTIN_INTL_RELATIVE_TIME_FORMAT_FUNCTION_ID,
        debug: "Intl.RelativeTimeFormat",
        flags: [CONSTRUCTABLE, INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: IntlRelativeTimeFormat,
        native: INTL_RELATIVE_TIME_FORMAT_NAME,
    }
    IntlRelativeTimeFormatSupportedLocalesOf {
        function: FunctionOrdinal(870) => BUILTIN_INTL_RELATIVE_TIME_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID,
        debug: "Intl.RelativeTimeFormat.supportedLocalesOf",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "supportedLocalesOf",
    }
    IntlRelativeTimeFormatPrototypeResolvedOptions {
        function: FunctionOrdinal(871) => BUILTIN_INTL_RELATIVE_TIME_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
        debug: "Intl.RelativeTimeFormat.prototype.resolvedOptions",
        flags: [],
        installer: None,
        native: "resolvedOptions",
    }
    IntlRelativeTimeFormatPrototypeFormat {
        function: FunctionOrdinal(872) => BUILTIN_INTL_RELATIVE_TIME_FORMAT_PROTOTYPE_FORMAT_FUNCTION_ID,
        debug: "Intl.RelativeTimeFormat.prototype.format",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "format",
    }
    IntlRelativeTimeFormatPrototypeFormatToParts {
        function: FunctionOrdinal(873) => BUILTIN_INTL_RELATIVE_TIME_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID,
        debug: "Intl.RelativeTimeFormat.prototype.formatToParts",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "formatToParts",
    }
    IntlSegmenterConstructor {
        function: FunctionOrdinal(874) => BUILTIN_INTL_SEGMENTER_FUNCTION_ID,
        debug: "Intl.Segmenter",
        flags: [CONSTRUCTABLE, INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: IntlSegmenter,
        native: "Segmenter",
    }
    IntlSegmenterSupportedLocalesOf {
        function: FunctionOrdinal(875) => BUILTIN_INTL_SEGMENTER_SUPPORTED_LOCALES_OF_FUNCTION_ID,
        debug: "Intl.Segmenter.supportedLocalesOf",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "supportedLocalesOf",
    }
    IntlSegmenterPrototypeSegment {
        function: FunctionOrdinal(876) => BUILTIN_INTL_SEGMENTER_PROTOTYPE_SEGMENT_FUNCTION_ID,
        debug: "Intl.Segmenter.prototype.segment",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "segment",
    }
    IntlSegmenterPrototypeResolvedOptions {
        function: FunctionOrdinal(877) => BUILTIN_INTL_SEGMENTER_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
        debug: "Intl.Segmenter.prototype.resolvedOptions",
        flags: [],
        installer: None,
        native: "resolvedOptions",
    }
    IntlSegmentsPrototypeContaining {
        function: FunctionOrdinal(878) => BUILTIN_INTL_SEGMENTS_PROTOTYPE_CONTAINING_FUNCTION_ID,
        debug: "%IntlSegmentsPrototype%.containing",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "containing",
    }
    IntlSegmentsPrototypeIterator {
        function: FunctionOrdinal(879) => BUILTIN_INTL_SEGMENTS_PROTOTYPE_ITERATOR_FUNCTION_ID,
        debug: "%IntlSegmentsPrototype%[Symbol.iterator]",
        flags: [],
        installer: None,
        native: "[Symbol.iterator]",
    }
    IntlSegmentIteratorPrototypeNext {
        function: FunctionOrdinal(880) => BUILTIN_INTL_SEGMENT_ITERATOR_PROTOTYPE_NEXT_FUNCTION_ID,
        debug: "%IntlSegmentIteratorPrototype%.next",
        flags: [],
        installer: None,
        native: "next",
    }
    IntlDurationFormatConstructor {
        function: FunctionOrdinal(881) => BUILTIN_INTL_DURATION_FORMAT_FUNCTION_ID,
        debug: "Intl.DurationFormat",
        flags: [CONSTRUCTABLE, INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: IntlDurationFormat,
        native: INTL_DURATION_FORMAT_NAME,
    }
    IntlDurationFormatSupportedLocalesOf {
        function: FunctionOrdinal(882) => BUILTIN_INTL_DURATION_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID,
        debug: "Intl.DurationFormat.supportedLocalesOf",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "supportedLocalesOf",
    }
    IntlDurationFormatPrototypeResolvedOptions {
        function: FunctionOrdinal(883) => BUILTIN_INTL_DURATION_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID,
        debug: "Intl.DurationFormat.prototype.resolvedOptions",
        flags: [],
        installer: None,
        native: "resolvedOptions",
    }
    IntlDurationFormatPrototypeFormat {
        function: FunctionOrdinal(884) => BUILTIN_INTL_DURATION_FORMAT_PROTOTYPE_FORMAT_FUNCTION_ID,
        debug: "Intl.DurationFormat.prototype.format",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "format",
    }
    IntlDurationFormatPrototypeFormatToParts {
        function: FunctionOrdinal(885) => BUILTIN_INTL_DURATION_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID,
        debug: "Intl.DurationFormat.prototype.formatToParts",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "formatToParts",
    }
    RegExpConstructor {
        function: FunctionOrdinal(499) => BUILTIN_REGEXP_FUNCTION_ID,
        global: GlobalOrdinal(10),
        global_name: REGEXP_NAME,
        debug: REGEXP_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: RegExp,
        native: REGEXP_NAME,
    }
    RegExpSpeciesGetter {
        function: FunctionOrdinal(500) => BUILTIN_REGEXP_SPECIES_GETTER_FUNCTION_ID,
        debug: "get RegExp [Symbol.species]",
        flags: [],
        installer: None,
        native: "get [Symbol.species]",
    }
    RegExpPrototypeFlagsGetter {
        function: FunctionOrdinal(501) => BUILTIN_REGEXP_PROTOTYPE_FLAGS_GETTER_FUNCTION_ID,
        debug: "get RegExp.prototype.flags",
        flags: [],
        installer: None,
        native: "get flags",
    }
    RegExpPrototypeSourceGetter {
        function: FunctionOrdinal(502) => BUILTIN_REGEXP_PROTOTYPE_SOURCE_GETTER_FUNCTION_ID,
        debug: "get RegExp.prototype.source",
        flags: [],
        installer: None,
        native: "get source",
    }
    RegExpPrototypeHasIndicesGetter {
        function: FunctionOrdinal(503) => BUILTIN_REGEXP_PROTOTYPE_HAS_INDICES_GETTER_FUNCTION_ID,
        debug: "get RegExp.prototype.hasIndices",
        flags: [],
        installer: None,
        native: "get hasIndices",
    }
    RegExpPrototypeGlobalGetter {
        function: FunctionOrdinal(504) => BUILTIN_REGEXP_PROTOTYPE_GLOBAL_GETTER_FUNCTION_ID,
        debug: "get RegExp.prototype.global",
        flags: [],
        installer: None,
        native: "get global",
    }
    RegExpPrototypeIgnoreCaseGetter {
        function: FunctionOrdinal(505) => BUILTIN_REGEXP_PROTOTYPE_IGNORE_CASE_GETTER_FUNCTION_ID,
        debug: "get RegExp.prototype.ignoreCase",
        flags: [],
        installer: None,
        native: "get ignoreCase",
    }
    RegExpPrototypeMultilineGetter {
        function: FunctionOrdinal(506) => BUILTIN_REGEXP_PROTOTYPE_MULTILINE_GETTER_FUNCTION_ID,
        debug: "get RegExp.prototype.multiline",
        flags: [],
        installer: None,
        native: "get multiline",
    }
    RegExpPrototypeDotAllGetter {
        function: FunctionOrdinal(507) => BUILTIN_REGEXP_PROTOTYPE_DOT_ALL_GETTER_FUNCTION_ID,
        debug: "get RegExp.prototype.dotAll",
        flags: [],
        installer: None,
        native: "get dotAll",
    }
    RegExpPrototypeUnicodeGetter {
        function: FunctionOrdinal(508) => BUILTIN_REGEXP_PROTOTYPE_UNICODE_GETTER_FUNCTION_ID,
        debug: "get RegExp.prototype.unicode",
        flags: [],
        installer: None,
        native: "get unicode",
    }
    RegExpPrototypeUnicodeSetsGetter {
        function: FunctionOrdinal(509) => BUILTIN_REGEXP_PROTOTYPE_UNICODE_SETS_GETTER_FUNCTION_ID,
        debug: "get RegExp.prototype.unicodeSets",
        flags: [],
        installer: None,
        native: "get unicodeSets",
    }
    RegExpPrototypeStickyGetter {
        function: FunctionOrdinal(510) => BUILTIN_REGEXP_PROTOTYPE_STICKY_GETTER_FUNCTION_ID,
        debug: "get RegExp.prototype.sticky",
        flags: [],
        installer: None,
        native: "get sticky",
    }
    RegExpLegacyStaticGetter {
        function: FunctionOrdinal(511) => BUILTIN_REGEXP_LEGACY_STATIC_GETTER_FUNCTION_ID,
        debug: "get RegExp legacy static",
        flags: [],
        installer: None,
        native: "get input",
    }
    RegExpLegacyStaticSetter {
        function: FunctionOrdinal(512) => BUILTIN_REGEXP_LEGACY_STATIC_SETTER_FUNCTION_ID,
        debug: "set RegExp legacy static",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "set input",
    }
    RegExpPrototypeCompile {
        function: FunctionOrdinal(513) => BUILTIN_REGEXP_PROTOTYPE_COMPILE_FUNCTION_ID,
        debug: "RegExp.prototype.compile",
        flags: [],
        installer: None,
        native: "compile",
    }
    RegExpPrototypeExec {
        function: FunctionOrdinal(514) => BUILTIN_REGEXP_PROTOTYPE_EXEC_FUNCTION_ID,
        debug: "RegExp.prototype.exec",
        flags: [],
        installer: None,
        native: "exec",
    }
    RegExpPrototypeTest {
        function: FunctionOrdinal(515) => BUILTIN_REGEXP_PROTOTYPE_TEST_FUNCTION_ID,
        debug: "RegExp.prototype.test",
        flags: [],
        installer: None,
        native: "test",
    }
    RegExpPrototypeToString {
        function: FunctionOrdinal(516) => BUILTIN_REGEXP_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "RegExp.prototype.toString",
        flags: [],
        installer: None,
        native: "toString",
    }
    RegExpPrototypeSymbolMatch {
        function: FunctionOrdinal(517) => BUILTIN_REGEXP_PROTOTYPE_SYMBOL_MATCH_FUNCTION_ID,
        debug: "RegExp.prototype[Symbol.match]",
        flags: [],
        installer: None,
        native: "[Symbol.match]",
    }
    RegExpPrototypeSymbolMatchAll {
        function: FunctionOrdinal(518) => BUILTIN_REGEXP_PROTOTYPE_SYMBOL_MATCH_ALL_FUNCTION_ID,
        debug: "RegExp.prototype[Symbol.matchAll]",
        flags: [],
        installer: None,
        native: "[Symbol.matchAll]",
    }
    RegExpPrototypeSymbolReplace {
        function: FunctionOrdinal(519) => BUILTIN_REGEXP_PROTOTYPE_SYMBOL_REPLACE_FUNCTION_ID,
        debug: "RegExp.prototype[Symbol.replace]",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "[Symbol.replace]",
    }
    RegExpPrototypeSymbolSearch {
        function: FunctionOrdinal(520) => BUILTIN_REGEXP_PROTOTYPE_SYMBOL_SEARCH_FUNCTION_ID,
        debug: "RegExp.prototype[Symbol.search]",
        flags: [],
        installer: None,
        native: "[Symbol.search]",
    }
    RegExpPrototypeSymbolSplit {
        function: FunctionOrdinal(521) => BUILTIN_REGEXP_PROTOTYPE_SYMBOL_SPLIT_FUNCTION_ID,
        debug: "RegExp.prototype[Symbol.split]",
        flags: [],
        installer: None,
        native: "[Symbol.split]",
    }
    RegExpEscape {
        function: FunctionOrdinal(522) => BUILTIN_REGEXP_ESCAPE_FUNCTION_ID,
        debug: "RegExp.escape",
        flags: [],
        installer: None,
        native: "escape",
    }
    JsonParse {
        function: FunctionOrdinal(523) => BUILTIN_JSON_PARSE_FUNCTION_ID,
        debug: "JSON.parse",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "parse",
    }
    JsonStringify {
        function: FunctionOrdinal(524) => BUILTIN_JSON_STRINGIFY_FUNCTION_ID,
        debug: "JSON.stringify",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "stringify",
    }
    JsonRawJson {
        function: FunctionOrdinal(525) => BUILTIN_JSON_RAW_JSON_FUNCTION_ID,
        debug: "JSON.rawJSON",
        flags: [],
        installer: None,
        native: "rawJSON",
    }
    JsonIsRawJson {
        function: FunctionOrdinal(526) => BUILTIN_JSON_IS_RAW_JSON_FUNCTION_ID,
        debug: "JSON.isRawJSON",
        flags: [],
        installer: None,
        native: "isRawJSON",
    }
    AtomicsAdd {
        function: FunctionOrdinal(527) => BUILTIN_ATOMICS_ADD_FUNCTION_ID,
        debug: "Atomics.add",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "add",
    }
    AtomicsAnd {
        function: FunctionOrdinal(528) => BUILTIN_ATOMICS_AND_FUNCTION_ID,
        debug: "Atomics.and",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "and",
    }
    AtomicsCompareExchange {
        function: FunctionOrdinal(529) => BUILTIN_ATOMICS_COMPARE_EXCHANGE_FUNCTION_ID,
        debug: "Atomics.compareExchange",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "compareExchange",
    }
    AtomicsExchange {
        function: FunctionOrdinal(530) => BUILTIN_ATOMICS_EXCHANGE_FUNCTION_ID,
        debug: "Atomics.exchange",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "exchange",
    }
    AtomicsLoad {
        function: FunctionOrdinal(531) => BUILTIN_ATOMICS_LOAD_FUNCTION_ID,
        debug: "Atomics.load",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "load",
    }
    AtomicsNotify {
        function: FunctionOrdinal(532) => BUILTIN_ATOMICS_NOTIFY_FUNCTION_ID,
        debug: "Atomics.notify",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "notify",
    }
    AtomicsOr {
        function: FunctionOrdinal(533) => BUILTIN_ATOMICS_OR_FUNCTION_ID,
        debug: "Atomics.or",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "or",
    }
    AtomicsPause {
        function: FunctionOrdinal(534) => BUILTIN_ATOMICS_PAUSE_FUNCTION_ID,
        debug: "Atomics.pause",
        flags: [],
        installer: None,
        native: "pause",
    }
    AtomicsStore {
        function: FunctionOrdinal(535) => BUILTIN_ATOMICS_STORE_FUNCTION_ID,
        debug: "Atomics.store",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "store",
    }
    AtomicsSub {
        function: FunctionOrdinal(536) => BUILTIN_ATOMICS_SUB_FUNCTION_ID,
        debug: "Atomics.sub",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "sub",
    }
    AtomicsWait {
        function: FunctionOrdinal(537) => BUILTIN_ATOMICS_WAIT_FUNCTION_ID,
        debug: "Atomics.wait",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "wait",
    }
    AtomicsWaitAsync {
        function: FunctionOrdinal(538) => BUILTIN_ATOMICS_WAIT_ASYNC_FUNCTION_ID,
        debug: "Atomics.waitAsync",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "waitAsync",
    }
    AtomicsXor {
        function: FunctionOrdinal(539) => BUILTIN_ATOMICS_XOR_FUNCTION_ID,
        debug: "Atomics.xor",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "xor",
    }
    AtomicsIsLockFree {
        function: FunctionOrdinal(540) => BUILTIN_ATOMICS_IS_LOCK_FREE_FUNCTION_ID,
        debug: "Atomics.isLockFree",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "isLockFree",
    }
    Float64ArrayConstructor {
        function: FunctionOrdinal(541) => BUILTIN_FLOAT64_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(11),
        global_name: FLOAT64_ARRAY_NAME,
        debug: FLOAT64_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: FLOAT64_ARRAY_NAME,
    }
    Float32ArrayConstructor {
        function: FunctionOrdinal(542) => BUILTIN_FLOAT32_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(12),
        global_name: FLOAT32_ARRAY_NAME,
        debug: FLOAT32_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: FLOAT32_ARRAY_NAME,
    }
    Int32ArrayConstructor {
        function: FunctionOrdinal(543) => BUILTIN_INT32_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(13),
        global_name: INT32_ARRAY_NAME,
        debug: INT32_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: INT32_ARRAY_NAME,
    }
    Int16ArrayConstructor {
        function: FunctionOrdinal(544) => BUILTIN_INT16_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(14),
        global_name: INT16_ARRAY_NAME,
        debug: INT16_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: INT16_ARRAY_NAME,
    }
    Int8ArrayConstructor {
        function: FunctionOrdinal(545) => BUILTIN_INT8_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(15),
        global_name: INT8_ARRAY_NAME,
        debug: INT8_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: INT8_ARRAY_NAME,
    }
    Uint32ArrayConstructor {
        function: FunctionOrdinal(546) => BUILTIN_UINT32_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(16),
        global_name: UINT32_ARRAY_NAME,
        debug: UINT32_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: UINT32_ARRAY_NAME,
    }
    Uint16ArrayConstructor {
        function: FunctionOrdinal(547) => BUILTIN_UINT16_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(17),
        global_name: UINT16_ARRAY_NAME,
        debug: UINT16_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: UINT16_ARRAY_NAME,
    }
    Uint8ArrayConstructor {
        function: FunctionOrdinal(548) => BUILTIN_UINT8_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(18),
        global_name: UINT8_ARRAY_NAME,
        debug: UINT8_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: Uint8Array,
        native: UINT8_ARRAY_NAME,
    }
    Uint8ClampedArrayConstructor {
        function: FunctionOrdinal(549) => BUILTIN_UINT8_CLAMPED_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(19),
        global_name: UINT8_CLAMPED_ARRAY_NAME,
        debug: UINT8_CLAMPED_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: UINT8_CLAMPED_ARRAY_NAME,
    }
    BigInt64ArrayConstructor {
        function: FunctionOrdinal(550) => BUILTIN_BIGINT64_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(20),
        global_name: BIGINT64_ARRAY_NAME,
        debug: BIGINT64_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: BIGINT64_ARRAY_NAME,
    }
    BigUint64ArrayConstructor {
        function: FunctionOrdinal(551) => BUILTIN_BIGUINT64_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(21),
        global_name: BIGUINT64_ARRAY_NAME,
        debug: BIGUINT64_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: BIGUINT64_ARRAY_NAME,
    }
    BigIntConstructor {
        function: FunctionOrdinal(552) => BUILTIN_BIGINT_FUNCTION_ID,
        global: GlobalOrdinal(22),
        global_name: BIGINT_NAME,
        debug: BIGINT_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: BigInt,
        native: BIGINT_NAME,
    }
    BigIntAsIntN {
        function: FunctionOrdinal(553) => BUILTIN_BIGINT_AS_INT_N_FUNCTION_ID,
        global_name: BIGINT_NAME,
        debug: "BigInt.asIntN",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "asIntN",
    }
    BigIntAsUintN {
        function: FunctionOrdinal(554) => BUILTIN_BIGINT_AS_UINT_N_FUNCTION_ID,
        global_name: BIGINT_NAME,
        debug: "BigInt.asUintN",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "asUintN",
    }
    BigIntPrototypeToString {
        function: FunctionOrdinal(555) => BUILTIN_BIGINT_PROTOTYPE_TO_STRING_FUNCTION_ID,
        global_name: BIGINT_NAME,
        debug: "BigInt.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toString",
    }
    BigIntPrototypeToLocaleString {
        function: FunctionOrdinal(556) => BUILTIN_BIGINT_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID,
        global_name: BIGINT_NAME,
        debug: "BigInt.prototype.toLocaleString",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toLocaleString",
    }
    BigIntPrototypeValueOf {
        function: FunctionOrdinal(557) => BUILTIN_BIGINT_PROTOTYPE_VALUE_OF_FUNCTION_ID,
        global_name: BIGINT_NAME,
        debug: "BigInt.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    NumberConstructor {
        function: FunctionOrdinal(558) => BUILTIN_NUMBER_FUNCTION_ID,
        global: GlobalOrdinal(23),
        global_name: NUMBER_NAME,
        debug: NUMBER_NAME,
        flags: [CONSTRUCTABLE, BOXED_PRIMITIVE, SYNCHRONOUS_USER_CODE],
        installer: Number,
        native: NUMBER_NAME,
    }
    NumberIsInteger {
        function: FunctionOrdinal(559) => BUILTIN_NUMBER_IS_INTEGER_FUNCTION_ID,
        debug: "Number.isInteger",
        flags: [STATIC_METHOD],
        installer: None,
        native: "isInteger",
    }
    NumberIsSafeInteger {
        function: FunctionOrdinal(560) => "$builtin.Number.isSafeInteger",
        debug: "Number.isSafeInteger",
        flags: [STATIC_METHOD],
        installer: None,
        native: "isSafeInteger",
    }
    NumberIsFinite {
        function: FunctionOrdinal(561) => "$builtin.Number.isFinite",
        debug: "Number.isFinite",
        flags: [STATIC_METHOD],
        installer: None,
        native: "isFinite",
    }
    NumberIsNaN {
        function: FunctionOrdinal(562) => "$builtin.Number.isNaN",
        debug: "Number.isNaN",
        flags: [STATIC_METHOD],
        installer: None,
        native: "isNaN",
    }
    NumberPrototypeToExponential {
        function: FunctionOrdinal(563) => "$builtin.Number.prototype.toExponential",
        debug: "Number.prototype.toExponential",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toExponential",
    }
    NumberPrototypeToFixed {
        function: FunctionOrdinal(564) => "$builtin.Number.prototype.toFixed",
        debug: "Number.prototype.toFixed",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toFixed",
    }
    NumberPrototypeToPrecision {
        function: FunctionOrdinal(565) => "$builtin.Number.prototype.toPrecision",
        debug: "Number.prototype.toPrecision",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toPrecision",
    }
    NumberPrototypeToString {
        function: FunctionOrdinal(566) => "$builtin.Number.prototype.toString",
        debug: "Number.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toString",
    }
    NumberPrototypeToLocaleString {
        function: FunctionOrdinal(567) => "$builtin.Number.prototype.toLocaleString",
        debug: "Number.prototype.toLocaleString",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toLocaleString",
    }
    NumberPrototypeValueOf {
        function: FunctionOrdinal(568) => "$builtin.Number.prototype.valueOf",
        debug: "Number.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    GlobalIsFinite {
        function: FunctionOrdinal(569) => "$builtin.isFinite",
        global: GlobalOrdinal(24),
        global_name: "isFinite",
        debug: "isFinite",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "isFinite",
    }
    GlobalIsNaN {
        function: FunctionOrdinal(570) => "$builtin.isNaN",
        global: GlobalOrdinal(25),
        global_name: "isNaN",
        debug: "isNaN",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "isNaN",
    }
    MathAbs {
        function: FunctionOrdinal(571) => "$builtin.Math.abs",
        debug: "Math.abs",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "abs",
    }
    MathAcos {
        function: FunctionOrdinal(572) => "$builtin.Math.acos",
        debug: "Math.acos",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "acos",
    }
    MathAcosh {
        function: FunctionOrdinal(573) => "$builtin.Math.acosh",
        debug: "Math.acosh",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "acosh",
    }
    MathAsin {
        function: FunctionOrdinal(574) => "$builtin.Math.asin",
        debug: "Math.asin",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "asin",
    }
    MathAsinh {
        function: FunctionOrdinal(575) => "$builtin.Math.asinh",
        debug: "Math.asinh",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "asinh",
    }
    MathAtan {
        function: FunctionOrdinal(576) => "$builtin.Math.atan",
        debug: "Math.atan",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "atan",
    }
    MathAtan2 {
        function: FunctionOrdinal(577) => "$builtin.Math.atan2",
        debug: "Math.atan2",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "atan2",
    }
    MathAtanh {
        function: FunctionOrdinal(578) => "$builtin.Math.atanh",
        debug: "Math.atanh",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "atanh",
    }
    MathCbrt {
        function: FunctionOrdinal(579) => "$builtin.Math.cbrt",
        debug: "Math.cbrt",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "cbrt",
    }
    MathCeil {
        function: FunctionOrdinal(580) => "$builtin.Math.ceil",
        debug: "Math.ceil",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "ceil",
    }
    MathClz32 {
        function: FunctionOrdinal(581) => "$builtin.Math.clz32",
        debug: "Math.clz32",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "clz32",
    }
    MathCos {
        function: FunctionOrdinal(582) => "$builtin.Math.cos",
        debug: "Math.cos",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "cos",
    }
    MathCosh {
        function: FunctionOrdinal(583) => "$builtin.Math.cosh",
        debug: "Math.cosh",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "cosh",
    }
    MathExp {
        function: FunctionOrdinal(584) => "$builtin.Math.exp",
        debug: "Math.exp",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "exp",
    }
    MathExpm1 {
        function: FunctionOrdinal(585) => "$builtin.Math.expm1",
        debug: "Math.expm1",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "expm1",
    }
    MathF16Round {
        function: FunctionOrdinal(586) => "$builtin.Math.f16round",
        debug: "Math.f16round",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "f16round",
    }
    MathFloor {
        function: FunctionOrdinal(587) => "$builtin.Math.floor",
        debug: "Math.floor",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "floor",
    }
    MathFround {
        function: FunctionOrdinal(588) => "$builtin.Math.fround",
        debug: "Math.fround",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "fround",
    }
    MathHypot {
        function: FunctionOrdinal(589) => "$builtin.Math.hypot",
        debug: "Math.hypot",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "hypot",
    }
    MathImul {
        function: FunctionOrdinal(590) => "$builtin.Math.imul",
        debug: "Math.imul",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "imul",
    }
    MathLog {
        function: FunctionOrdinal(591) => "$builtin.Math.log",
        debug: "Math.log",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "log",
    }
    MathLog10 {
        function: FunctionOrdinal(592) => "$builtin.Math.log10",
        debug: "Math.log10",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "log10",
    }
    MathLog1p {
        function: FunctionOrdinal(593) => "$builtin.Math.log1p",
        debug: "Math.log1p",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "log1p",
    }
    MathLog2 {
        function: FunctionOrdinal(594) => "$builtin.Math.log2",
        debug: "Math.log2",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "log2",
    }
    MathPow {
        function: FunctionOrdinal(595) => "$builtin.Math.pow",
        debug: "Math.pow",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "pow",
    }
    MathRandom {
        function: FunctionOrdinal(596) => "$builtin.Math.random",
        debug: "Math.random",
        flags: [STATIC_METHOD, RANDOM],
        installer: None,
        native: "random",
    }
    MathRound {
        function: FunctionOrdinal(597) => "$builtin.Math.round",
        debug: "Math.round",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "round",
    }
    MathSign {
        function: FunctionOrdinal(598) => "$builtin.Math.sign",
        debug: "Math.sign",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "sign",
    }
    MathSin {
        function: FunctionOrdinal(599) => "$builtin.Math.sin",
        debug: "Math.sin",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "sin",
    }
    MathSinh {
        function: FunctionOrdinal(600) => "$builtin.Math.sinh",
        debug: "Math.sinh",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "sinh",
    }
    MathSqrt {
        function: FunctionOrdinal(601) => "$builtin.Math.sqrt",
        debug: "Math.sqrt",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "sqrt",
    }
    MathSumPrecise {
        function: FunctionOrdinal(602) => "$builtin.Math.sumPrecise",
        debug: "Math.sumPrecise",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "sumPrecise",
    }
    MathTan {
        function: FunctionOrdinal(603) => "$builtin.Math.tan",
        debug: "Math.tan",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "tan",
    }
    MathTanh {
        function: FunctionOrdinal(604) => "$builtin.Math.tanh",
        debug: "Math.tanh",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "tanh",
    }
    MathTrunc {
        function: FunctionOrdinal(605) => "$builtin.Math.trunc",
        debug: "Math.trunc",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "trunc",
    }
    MathMin {
        function: FunctionOrdinal(606) => "$builtin.Math.min",
        debug: "Math.min",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "min",
    }
    MathMax {
        function: FunctionOrdinal(607) => "$builtin.Math.max",
        debug: "Math.max",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "max",
    }
    StringConstructor {
        function: FunctionOrdinal(608) => BUILTIN_STRING_FUNCTION_ID,
        global: GlobalOrdinal(26),
        global_name: STRING_NAME,
        debug: STRING_NAME,
        flags: [CONSTRUCTABLE, BOXED_PRIMITIVE, SYNCHRONOUS_USER_CODE],
        installer: String,
        native: STRING_NAME,
    }
    StringFromCharCode {
        function: FunctionOrdinal(609) => BUILTIN_STRING_FROM_CHAR_CODE_FUNCTION_ID,
        debug: "String.fromCharCode",
        flags: [],
        installer: None,
        native: "fromCharCode",
    }
    StringFromCodePoint {
        function: FunctionOrdinal(610) => BUILTIN_STRING_FROM_CODE_POINT_FUNCTION_ID,
        debug: "String.fromCodePoint",
        flags: [],
        installer: None,
        native: "fromCodePoint",
    }
    StringRaw {
        function: FunctionOrdinal(611) => BUILTIN_STRING_RAW_FUNCTION_ID,
        debug: "String.raw",
        flags: [],
        installer: None,
        native: "raw",
    }
    StringPrototypeToString {
        function: FunctionOrdinal(612) => BUILTIN_STRING_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "String.prototype.toString",
        flags: [],
        installer: None,
        string: "toString",
        native: "toString",
    }
    StringPrototypeValueOf {
        function: FunctionOrdinal(613) => BUILTIN_STRING_PROTOTYPE_VALUE_OF_FUNCTION_ID,
        debug: "String.prototype.valueOf",
        flags: [],
        installer: None,
        string: "valueOf",
        native: "valueOf",
    }
    StringPrototypeCharAt {
        function: FunctionOrdinal(614) => BUILTIN_STRING_PROTOTYPE_CHAR_AT_FUNCTION_ID,
        debug: "String.prototype.charAt",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "charAt",
        native: "charAt",
    }
    StringPrototypeConcat {
        function: FunctionOrdinal(615) => BUILTIN_STRING_PROTOTYPE_CONCAT_FUNCTION_ID,
        debug: "String.prototype.concat",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "concat",
        native: "concat",
    }
    StringPrototypeCharCodeAt {
        function: FunctionOrdinal(616) => BUILTIN_STRING_PROTOTYPE_CHAR_CODE_AT_FUNCTION_ID,
        debug: "String.prototype.charCodeAt",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "charCodeAt",
        native: "charCodeAt",
    }
    StringPrototypeCodePointAt {
        function: FunctionOrdinal(617) => BUILTIN_STRING_PROTOTYPE_CODE_POINT_AT_FUNCTION_ID,
        debug: "String.prototype.codePointAt",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "codePointAt",
        native: "codePointAt",
    }
    StringPrototypeAt {
        function: FunctionOrdinal(618) => BUILTIN_STRING_PROTOTYPE_AT_FUNCTION_ID,
        debug: "String.prototype.at",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "at",
        native: "at",
    }
    StringPrototypeAnchor {
        function: FunctionOrdinal(619) => BUILTIN_STRING_PROTOTYPE_ANCHOR_FUNCTION_ID,
        debug: "String.prototype.anchor",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "anchor",
        native: "anchor",
    }
    StringPrototypeBig {
        function: FunctionOrdinal(620) => BUILTIN_STRING_PROTOTYPE_BIG_FUNCTION_ID,
        debug: "String.prototype.big",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "big",
        native: "big",
    }
    StringPrototypeBlink {
        function: FunctionOrdinal(621) => BUILTIN_STRING_PROTOTYPE_BLINK_FUNCTION_ID,
        debug: "String.prototype.blink",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "blink",
        native: "blink",
    }
    StringPrototypeBold {
        function: FunctionOrdinal(622) => BUILTIN_STRING_PROTOTYPE_BOLD_FUNCTION_ID,
        debug: "String.prototype.bold",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "bold",
        native: "bold",
    }
    StringPrototypeFixed {
        function: FunctionOrdinal(623) => BUILTIN_STRING_PROTOTYPE_FIXED_FUNCTION_ID,
        debug: "String.prototype.fixed",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "fixed",
        native: "fixed",
    }
    StringPrototypeFontcolor {
        function: FunctionOrdinal(624) => BUILTIN_STRING_PROTOTYPE_FONTCOLOR_FUNCTION_ID,
        debug: "String.prototype.fontcolor",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "fontcolor",
        native: "fontcolor",
    }
    StringPrototypeFontsize {
        function: FunctionOrdinal(625) => BUILTIN_STRING_PROTOTYPE_FONTSIZE_FUNCTION_ID,
        debug: "String.prototype.fontsize",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "fontsize",
        native: "fontsize",
    }
    StringPrototypeItalics {
        function: FunctionOrdinal(626) => BUILTIN_STRING_PROTOTYPE_ITALICS_FUNCTION_ID,
        debug: "String.prototype.italics",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "italics",
        native: "italics",
    }
    StringPrototypeLink {
        function: FunctionOrdinal(627) => BUILTIN_STRING_PROTOTYPE_LINK_FUNCTION_ID,
        debug: "String.prototype.link",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "link",
        native: "link",
    }
    StringPrototypeSmall {
        function: FunctionOrdinal(628) => BUILTIN_STRING_PROTOTYPE_SMALL_FUNCTION_ID,
        debug: "String.prototype.small",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "small",
        native: "small",
    }
    StringPrototypeStrike {
        function: FunctionOrdinal(629) => BUILTIN_STRING_PROTOTYPE_STRIKE_FUNCTION_ID,
        debug: "String.prototype.strike",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "strike",
        native: "strike",
    }
    StringPrototypeSub {
        function: FunctionOrdinal(630) => BUILTIN_STRING_PROTOTYPE_SUB_FUNCTION_ID,
        debug: "String.prototype.sub",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "sub",
        native: "sub",
    }
    StringPrototypeSubstr {
        function: FunctionOrdinal(631) => BUILTIN_STRING_PROTOTYPE_SUBSTR_FUNCTION_ID,
        debug: "String.prototype.substr",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "substr",
        native: "substr",
    }
    StringPrototypeSubstring {
        function: FunctionOrdinal(632) => BUILTIN_STRING_PROTOTYPE_SUBSTRING_FUNCTION_ID,
        debug: "String.prototype.substring",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "substring",
        native: "substring",
    }
    StringPrototypeSup {
        function: FunctionOrdinal(633) => BUILTIN_STRING_PROTOTYPE_SUP_FUNCTION_ID,
        debug: "String.prototype.sup",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "sup",
        native: "sup",
    }
    StringPrototypeMatch {
        function: FunctionOrdinal(634) => BUILTIN_STRING_PROTOTYPE_MATCH_FUNCTION_ID,
        debug: "String.prototype.match",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "match",
        native: "match",
    }
    StringPrototypeMatchAll {
        function: FunctionOrdinal(635) => BUILTIN_STRING_PROTOTYPE_MATCH_ALL_FUNCTION_ID,
        debug: "String.prototype.matchAll",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "matchAll",
        native: "matchAll",
    }
    StringPrototypeReplace {
        function: FunctionOrdinal(636) => BUILTIN_STRING_PROTOTYPE_REPLACE_FUNCTION_ID,
        debug: "String.prototype.replace",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "replace",
        native: "replace",
    }
    StringPrototypeReplaceAll {
        function: FunctionOrdinal(637) => BUILTIN_STRING_PROTOTYPE_REPLACE_ALL_FUNCTION_ID,
        debug: "String.prototype.replaceAll",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "replaceAll",
        native: "replaceAll",
    }
    StringPrototypeSearch {
        function: FunctionOrdinal(638) => BUILTIN_STRING_PROTOTYPE_SEARCH_FUNCTION_ID,
        debug: "String.prototype.search",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "search",
        native: "search",
    }
    StringPrototypeIndexOf {
        function: FunctionOrdinal(639) => BUILTIN_STRING_PROTOTYPE_INDEX_OF_FUNCTION_ID,
        debug: "String.prototype.indexOf",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "indexOf",
        native: "indexOf",
    }
    StringPrototypeLastIndexOf {
        function: FunctionOrdinal(640) => BUILTIN_STRING_PROTOTYPE_LAST_INDEX_OF_FUNCTION_ID,
        debug: "String.prototype.lastIndexOf",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "lastIndexOf",
        native: "lastIndexOf",
    }
    StringPrototypeSlice {
        function: FunctionOrdinal(641) => BUILTIN_STRING_PROTOTYPE_SLICE_FUNCTION_ID,
        debug: "String.prototype.slice",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "slice",
        native: "slice",
    }
    StringPrototypeSplit {
        function: FunctionOrdinal(642) => BUILTIN_STRING_PROTOTYPE_SPLIT_FUNCTION_ID,
        debug: "String.prototype.split",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "split",
        native: "split",
    }
    StringPrototypePadStart {
        function: FunctionOrdinal(643) => BUILTIN_STRING_PROTOTYPE_PAD_START_FUNCTION_ID,
        debug: "String.prototype.padStart",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "padStart",
        native: "padStart",
    }
    StringPrototypePadEnd {
        function: FunctionOrdinal(644) => BUILTIN_STRING_PROTOTYPE_PAD_END_FUNCTION_ID,
        debug: "String.prototype.padEnd",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "padEnd",
        native: "padEnd",
    }
    StringPrototypeRepeat {
        function: FunctionOrdinal(645) => BUILTIN_STRING_PROTOTYPE_REPEAT_FUNCTION_ID,
        debug: "String.prototype.repeat",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "repeat",
        native: "repeat",
    }
    StringPrototypeEndsWith {
        function: FunctionOrdinal(646) => BUILTIN_STRING_PROTOTYPE_ENDS_WITH_FUNCTION_ID,
        debug: "String.prototype.endsWith",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "endsWith",
        native: "endsWith",
    }
    StringPrototypeIncludes {
        function: FunctionOrdinal(647) => BUILTIN_STRING_PROTOTYPE_INCLUDES_FUNCTION_ID,
        debug: "String.prototype.includes",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "includes",
        native: "includes",
    }
    StringPrototypeStartsWith {
        function: FunctionOrdinal(648) => BUILTIN_STRING_PROTOTYPE_STARTS_WITH_FUNCTION_ID,
        debug: "String.prototype.startsWith",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "startsWith",
        native: "startsWith",
    }
    StringPrototypeNormalize {
        function: FunctionOrdinal(649) => BUILTIN_STRING_PROTOTYPE_NORMALIZE_FUNCTION_ID,
        debug: "String.prototype.normalize",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "normalize",
        native: "normalize",
    }
    StringPrototypeLocaleCompare {
        function: FunctionOrdinal(650) => BUILTIN_STRING_PROTOTYPE_LOCALE_COMPARE_FUNCTION_ID,
        debug: "String.prototype.localeCompare",
        flags: [INTL_HOST, SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "localeCompare",
        native: "localeCompare",
    }
    StringPrototypeIterator {
        function: FunctionOrdinal(651) => BUILTIN_STRING_PROTOTYPE_ITERATOR_FUNCTION_ID,
        debug: "String.prototype [Symbol.iterator]",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "[Symbol.iterator]",
        native: "[Symbol.iterator]",
    }
    StringPrototypeToLocaleLowerCase {
        function: FunctionOrdinal(652) => BUILTIN_STRING_PROTOTYPE_TO_LOCALE_LOWER_CASE_FUNCTION_ID,
        debug: "String.prototype.toLocaleLowerCase",
        flags: [SYNCHRONOUS_USER_CODE, INTL_HOST],
        installer: None,
        string: "toLocaleLowerCase",
        native: "toLocaleLowerCase",
    }
    StringPrototypeToLocaleUpperCase {
        function: FunctionOrdinal(653) => BUILTIN_STRING_PROTOTYPE_TO_LOCALE_UPPER_CASE_FUNCTION_ID,
        debug: "String.prototype.toLocaleUpperCase",
        flags: [SYNCHRONOUS_USER_CODE, INTL_HOST],
        installer: None,
        string: "toLocaleUpperCase",
        native: "toLocaleUpperCase",
    }
    StringPrototypeToLowerCase {
        function: FunctionOrdinal(654) => BUILTIN_STRING_PROTOTYPE_TO_LOWER_CASE_FUNCTION_ID,
        debug: "String.prototype.toLowerCase",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "toLowerCase",
        native: "toLowerCase",
    }
    StringPrototypeToUpperCase {
        function: FunctionOrdinal(655) => BUILTIN_STRING_PROTOTYPE_TO_UPPER_CASE_FUNCTION_ID,
        debug: "String.prototype.toUpperCase",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "toUpperCase",
        native: "toUpperCase",
    }
    StringPrototypeTrim {
        function: FunctionOrdinal(656) => BUILTIN_STRING_PROTOTYPE_TRIM_FUNCTION_ID,
        debug: "String.prototype.trim",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "trim",
        native: "trim",
    }
    StringPrototypeTrimStart {
        function: FunctionOrdinal(657) => BUILTIN_STRING_PROTOTYPE_TRIM_START_FUNCTION_ID,
        debug: "String.prototype.trimStart",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "trimStart",
        native: "trimStart",
    }
    StringPrototypeTrimEnd {
        function: FunctionOrdinal(658) => BUILTIN_STRING_PROTOTYPE_TRIM_END_FUNCTION_ID,
        debug: "String.prototype.trimEnd",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "trimEnd",
        native: "trimEnd",
    }
    StringPrototypeIsWellFormed {
        function: FunctionOrdinal(659) => BUILTIN_STRING_PROTOTYPE_IS_WELL_FORMED_FUNCTION_ID,
        debug: "String.prototype.isWellFormed",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "isWellFormed",
        native: "isWellFormed",
    }
    StringPrototypeToWellFormed {
        function: FunctionOrdinal(660) => BUILTIN_STRING_PROTOTYPE_TO_WELL_FORMED_FUNCTION_ID,
        debug: "String.prototype.toWellFormed",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        string: "toWellFormed",
        native: "toWellFormed",
    }
    BooleanConstructor {
        function: FunctionOrdinal(661) => BUILTIN_BOOLEAN_FUNCTION_ID,
        global: GlobalOrdinal(27),
        global_name: BOOLEAN_NAME,
        debug: BOOLEAN_NAME,
        flags: [CONSTRUCTABLE, BOXED_PRIMITIVE, SYNCHRONOUS_USER_CODE],
        installer: Boolean,
        native: BOOLEAN_NAME,
    }
    BooleanPrototypeToString {
        function: FunctionOrdinal(662) => "$builtin.Boolean.prototype.toString",
        debug: "Boolean.prototype.toString",
        flags: [],
        installer: None,
        native: "toString",
    }
    BooleanPrototypeValueOf {
        function: FunctionOrdinal(663) => "$builtin.Boolean.prototype.valueOf",
        debug: "Boolean.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    PromiseConstructor {
        function: FunctionOrdinal(664) => BUILTIN_PROMISE_FUNCTION_ID,
        global: GlobalOrdinal(28),
        global_name: PROMISE_NAME,
        debug: PROMISE_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: Promise,
        native: PROMISE_NAME,
    }
    PromisePrototypeThen {
        function: FunctionOrdinal(665) => BUILTIN_PROMISE_PROTOTYPE_THEN_FUNCTION_ID,
        debug: "Promise.prototype.then",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "then",
    }
    PromisePrototypeCatch {
        function: FunctionOrdinal(666) => BUILTIN_PROMISE_PROTOTYPE_CATCH_FUNCTION_ID,
        debug: "Promise.prototype.catch",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "catch",
    }
    PromisePrototypeFinally {
        function: FunctionOrdinal(667) => BUILTIN_PROMISE_PROTOTYPE_FINALLY_FUNCTION_ID,
        debug: "Promise.prototype.finally",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "finally",
    }
    PromiseThenFinally {
        function: FunctionOrdinal(668) => BUILTIN_PROMISE_THEN_FINALLY_FUNCTION_ID,
        debug: "Promise Then Finally Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    PromiseCatchFinally {
        function: FunctionOrdinal(669) => BUILTIN_PROMISE_CATCH_FINALLY_FUNCTION_ID,
        debug: "Promise Catch Finally Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    PromiseValueThunk {
        function: FunctionOrdinal(670) => BUILTIN_PROMISE_VALUE_THUNK_FUNCTION_ID,
        debug: "Promise Value Thunk Function",
        flags: [],
        installer: None,
        native: "",
    }
    PromiseThrower {
        function: FunctionOrdinal(671) => BUILTIN_PROMISE_THROWER_FUNCTION_ID,
        debug: "Promise Thrower Function",
        flags: [],
        installer: None,
        native: "",
    }
    PromiseSpeciesGetter {
        function: FunctionOrdinal(672) => BUILTIN_PROMISE_SPECIES_GETTER_FUNCTION_ID,
        debug: "get Promise [Symbol.species]",
        flags: [],
        installer: None,
        native: "get [Symbol.species]",
    }
    PromiseResolve {
        function: FunctionOrdinal(673) => BUILTIN_PROMISE_STATIC_RESOLVE_FUNCTION_ID,
        debug: "Promise.resolve",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "resolve",
    }
    PromiseWithResolvers {
        function: FunctionOrdinal(674) => BUILTIN_PROMISE_STATIC_WITH_RESOLVERS_FUNCTION_ID,
        debug: "Promise.withResolvers",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "withResolvers",
    }
    PromiseTry {
        function: FunctionOrdinal(675) => BUILTIN_PROMISE_STATIC_TRY_FUNCTION_ID,
        debug: "Promise.try",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "try",
    }
    PromiseReject {
        function: FunctionOrdinal(676) => BUILTIN_PROMISE_STATIC_REJECT_FUNCTION_ID,
        debug: "Promise.reject",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "reject",
    }
    PromiseAll {
        function: FunctionOrdinal(677) => BUILTIN_PROMISE_STATIC_ALL_FUNCTION_ID,
        debug: "Promise.all",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "all",
    }
    PromiseAllSettled {
        function: FunctionOrdinal(678) => BUILTIN_PROMISE_STATIC_ALL_SETTLED_FUNCTION_ID,
        debug: "Promise.allSettled",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "allSettled",
    }
    PromiseAllKeyed {
        function: FunctionOrdinal(679) => BUILTIN_PROMISE_STATIC_ALL_KEYED_FUNCTION_ID,
        debug: "Promise.allKeyed",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "allKeyed",
    }
    PromiseAllSettledKeyed {
        function: FunctionOrdinal(680) => BUILTIN_PROMISE_STATIC_ALL_SETTLED_KEYED_FUNCTION_ID,
        debug: "Promise.allSettledKeyed",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "allSettledKeyed",
    }
    PromiseAny {
        function: FunctionOrdinal(681) => BUILTIN_PROMISE_STATIC_ANY_FUNCTION_ID,
        debug: "Promise.any",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "any",
    }
    PromiseRace {
        function: FunctionOrdinal(682) => BUILTIN_PROMISE_STATIC_RACE_FUNCTION_ID,
        debug: "Promise.race",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "race",
    }
    PromiseAllResolveElement {
        function: FunctionOrdinal(683) => BUILTIN_PROMISE_ALL_RESOLVE_ELEMENT_FUNCTION_ID,
        debug: "Promise.all Resolve Element Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    PromiseAllSettledResolveElement {
        function: FunctionOrdinal(684) => BUILTIN_PROMISE_ALL_SETTLED_RESOLVE_ELEMENT_FUNCTION_ID,
        debug: "Promise.allSettled Resolve Element Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    PromiseAllSettledRejectElement {
        function: FunctionOrdinal(685) => BUILTIN_PROMISE_ALL_SETTLED_REJECT_ELEMENT_FUNCTION_ID,
        debug: "Promise.allSettled Reject Element Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    PromiseAnyRejectElement {
        function: FunctionOrdinal(686) => BUILTIN_PROMISE_ANY_REJECT_ELEMENT_FUNCTION_ID,
        debug: "Promise.any Reject Element Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    PromiseAllKeyedResolveElement {
        function: FunctionOrdinal(687) => BUILTIN_PROMISE_ALL_KEYED_RESOLVE_ELEMENT_FUNCTION_ID,
        debug: "Promise.allKeyed Resolve Element Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    PromiseAllSettledKeyedResolveElement {
        function: FunctionOrdinal(688) => BUILTIN_PROMISE_ALL_SETTLED_KEYED_RESOLVE_ELEMENT_FUNCTION_ID,
        debug: "Promise.allSettledKeyed Resolve Element Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    PromiseAllSettledKeyedRejectElement {
        function: FunctionOrdinal(689) => BUILTIN_PROMISE_ALL_SETTLED_KEYED_REJECT_ELEMENT_FUNCTION_ID,
        debug: "Promise.allSettledKeyed Reject Element Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    PromiseCapabilityExecutor {
        function: FunctionOrdinal(690) => BUILTIN_PROMISE_CAPABILITY_EXECUTOR_FUNCTION_ID,
        debug: "Promise Capability Executor",
        flags: [],
        installer: None,
        native: "",
    }
    PromiseResolveFunction {
        function: FunctionOrdinal(691) => BUILTIN_PROMISE_RESOLVE_FUNCTION_ID,
        debug: "Promise Resolve Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    PromiseRejectFunction {
        function: FunctionOrdinal(692) => BUILTIN_PROMISE_REJECT_FUNCTION_ID,
        debug: "Promise Reject Function",
        flags: [],
        installer: None,
        native: "",
    }
    MapConstructor {
        function: FunctionOrdinal(693) => BUILTIN_MAP_FUNCTION_ID,
        global: GlobalOrdinal(29),
        global_name: MAP_NAME,
        debug: MAP_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: Map,
        native: MAP_NAME,
    }
    MapSpeciesGetter {
        function: FunctionOrdinal(694) => BUILTIN_MAP_SPECIES_GETTER_FUNCTION_ID,
        debug: "get Map [Symbol.species]",
        flags: [],
        installer: None,
        native: "get [Symbol.species]",
    }
    MapGroupBy {
        function: FunctionOrdinal(695) => BUILTIN_MAP_GROUP_BY_FUNCTION_ID,
        debug: "Map.groupBy",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "groupBy",
    }
    MapPrototypeClear {
        function: FunctionOrdinal(696) => BUILTIN_MAP_PROTOTYPE_CLEAR_FUNCTION_ID,
        debug: "Map.prototype.clear",
        flags: [],
        installer: None,
        native: "clear",
    }
    MapPrototypeDelete {
        function: FunctionOrdinal(697) => BUILTIN_MAP_PROTOTYPE_DELETE_FUNCTION_ID,
        debug: "Map.prototype.delete",
        flags: [],
        installer: None,
        native: "delete",
    }
    MapPrototypeForEach {
        function: FunctionOrdinal(698) => BUILTIN_MAP_PROTOTYPE_FOR_EACH_FUNCTION_ID,
        debug: "Map.prototype.forEach",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "forEach",
    }
    MapPrototypeKeys {
        function: FunctionOrdinal(699) => BUILTIN_MAP_PROTOTYPE_KEYS_FUNCTION_ID,
        debug: "Map.prototype.keys",
        flags: [],
        installer: None,
        native: "keys",
    }
    MapPrototypeValues {
        function: FunctionOrdinal(700) => BUILTIN_MAP_PROTOTYPE_VALUES_FUNCTION_ID,
        debug: "Map.prototype.values",
        flags: [],
        installer: None,
        native: "values",
    }
    MapPrototypeEntries {
        function: FunctionOrdinal(701) => BUILTIN_MAP_PROTOTYPE_ENTRIES_FUNCTION_ID,
        debug: "Map.prototype.entries",
        flags: [],
        installer: None,
        native: "entries",
    }
    MapIteratorNext {
        function: FunctionOrdinal(702) => BUILTIN_MAP_ITERATOR_NEXT_FUNCTION_ID,
        debug: "Map Iterator.prototype.next",
        flags: [],
        installer: None,
        native: "next",
    }
    MapPrototypeGet {
        function: FunctionOrdinal(703) => BUILTIN_MAP_PROTOTYPE_GET_FUNCTION_ID,
        debug: "Map.prototype.get",
        flags: [],
        installer: None,
        native: "get",
    }
    MapPrototypeGetOrInsert {
        function: FunctionOrdinal(704) => BUILTIN_MAP_PROTOTYPE_GET_OR_INSERT_FUNCTION_ID,
        debug: "Map.prototype.getOrInsert",
        flags: [],
        installer: None,
        native: "getOrInsert",
    }
    MapPrototypeGetOrInsertComputed {
        function: FunctionOrdinal(705) => BUILTIN_MAP_PROTOTYPE_GET_OR_INSERT_COMPUTED_FUNCTION_ID,
        debug: "Map.prototype.getOrInsertComputed",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getOrInsertComputed",
    }
    MapPrototypeHas {
        function: FunctionOrdinal(706) => BUILTIN_MAP_PROTOTYPE_HAS_FUNCTION_ID,
        debug: "Map.prototype.has",
        flags: [],
        installer: None,
        native: "has",
    }
    MapPrototypeSet {
        function: FunctionOrdinal(707) => BUILTIN_MAP_PROTOTYPE_SET_FUNCTION_ID,
        debug: "Map.prototype.set",
        flags: [],
        installer: None,
        native: "set",
    }
    MapPrototypeSizeGetter {
        function: FunctionOrdinal(708) => BUILTIN_MAP_PROTOTYPE_SIZE_GETTER_FUNCTION_ID,
        debug: "get Map.prototype.size",
        flags: [],
        installer: None,
        native: "get size",
    }
    WeakMapConstructor {
        function: FunctionOrdinal(709) => BUILTIN_WEAK_MAP_FUNCTION_ID,
        global: GlobalOrdinal(30),
        global_name: WEAK_MAP_NAME,
        debug: WEAK_MAP_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: WeakMap,
        native: WEAK_MAP_NAME,
    }
    WeakMapPrototypeDelete {
        function: FunctionOrdinal(710) => BUILTIN_WEAK_MAP_PROTOTYPE_DELETE_FUNCTION_ID,
        debug: "WeakMap.prototype.delete",
        flags: [],
        installer: None,
        native: "delete",
    }
    WeakMapPrototypeGet {
        function: FunctionOrdinal(711) => BUILTIN_WEAK_MAP_PROTOTYPE_GET_FUNCTION_ID,
        debug: "WeakMap.prototype.get",
        flags: [],
        installer: None,
        native: "get",
    }
    WeakMapPrototypeGetOrInsert {
        function: FunctionOrdinal(712) => BUILTIN_WEAK_MAP_PROTOTYPE_GET_OR_INSERT_FUNCTION_ID,
        debug: "WeakMap.prototype.getOrInsert",
        flags: [],
        installer: None,
        native: "getOrInsert",
    }
    WeakMapPrototypeGetOrInsertComputed {
        function: FunctionOrdinal(713) => BUILTIN_WEAK_MAP_PROTOTYPE_GET_OR_INSERT_COMPUTED_FUNCTION_ID,
        debug: "WeakMap.prototype.getOrInsertComputed",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getOrInsertComputed",
    }
    WeakMapPrototypeHas {
        function: FunctionOrdinal(714) => BUILTIN_WEAK_MAP_PROTOTYPE_HAS_FUNCTION_ID,
        debug: "WeakMap.prototype.has",
        flags: [],
        installer: None,
        native: "has",
    }
    WeakMapPrototypeSet {
        function: FunctionOrdinal(715) => BUILTIN_WEAK_MAP_PROTOTYPE_SET_FUNCTION_ID,
        debug: "WeakMap.prototype.set",
        flags: [],
        installer: None,
        native: "set",
    }
    WeakSetConstructor {
        function: FunctionOrdinal(716) => BUILTIN_WEAK_SET_FUNCTION_ID,
        global: GlobalOrdinal(31),
        global_name: WEAK_SET_NAME,
        debug: WEAK_SET_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: WeakSet,
        native: WEAK_SET_NAME,
    }
    WeakSetPrototypeAdd {
        function: FunctionOrdinal(717) => BUILTIN_WEAK_SET_PROTOTYPE_ADD_FUNCTION_ID,
        debug: "WeakSet.prototype.add",
        flags: [],
        installer: None,
        native: "add",
    }
    WeakSetPrototypeDelete {
        function: FunctionOrdinal(718) => BUILTIN_WEAK_SET_PROTOTYPE_DELETE_FUNCTION_ID,
        debug: "WeakSet.prototype.delete",
        flags: [],
        installer: None,
        native: "delete",
    }
    WeakSetPrototypeHas {
        function: FunctionOrdinal(719) => BUILTIN_WEAK_SET_PROTOTYPE_HAS_FUNCTION_ID,
        debug: "WeakSet.prototype.has",
        flags: [],
        installer: None,
        native: "has",
    }
    WeakRefConstructor {
        function: FunctionOrdinal(720) => BUILTIN_WEAK_REF_FUNCTION_ID,
        global: GlobalOrdinal(32),
        global_name: WEAK_REF_NAME,
        debug: WEAK_REF_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: WeakRef,
        native: WEAK_REF_NAME,
    }
    WeakRefPrototypeDeref {
        function: FunctionOrdinal(721) => BUILTIN_WEAK_REF_PROTOTYPE_DEREF_FUNCTION_ID,
        debug: "WeakRef.prototype.deref",
        flags: [],
        installer: None,
        native: "deref",
    }
    FinalizationRegistryConstructor {
        function: FunctionOrdinal(722) => BUILTIN_FINALIZATION_REGISTRY_FUNCTION_ID,
        global: GlobalOrdinal(33),
        global_name: FINALIZATION_REGISTRY_NAME,
        debug: FINALIZATION_REGISTRY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: FinalizationRegistry,
        native: FINALIZATION_REGISTRY_NAME,
    }
    FinalizationRegistryPrototypeRegister {
        function: FunctionOrdinal(723) => BUILTIN_FINALIZATION_REGISTRY_PROTOTYPE_REGISTER_FUNCTION_ID,
        debug: "FinalizationRegistry.prototype.register",
        flags: [],
        installer: None,
        native: "register",
    }
    FinalizationRegistryPrototypeUnregister {
        function: FunctionOrdinal(724) => BUILTIN_FINALIZATION_REGISTRY_PROTOTYPE_UNREGISTER_FUNCTION_ID,
        debug: "FinalizationRegistry.prototype.unregister",
        flags: [],
        installer: None,
        native: "unregister",
    }
    AsyncDisposableStackConstructor {
        function: FunctionOrdinal(725) => BUILTIN_ASYNC_DISPOSABLE_STACK_FUNCTION_ID,
        global: GlobalOrdinal(34),
        global_name: ASYNC_DISPOSABLE_STACK_NAME,
        debug: ASYNC_DISPOSABLE_STACK_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: AsyncDisposableStack,
        native: ASYNC_DISPOSABLE_STACK_NAME,
    }
    AsyncDisposableStackPrototypeUse {
        function: FunctionOrdinal(726) => BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_USE_FUNCTION_ID,
        debug: "AsyncDisposableStack.prototype.use",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "use",
    }
    AsyncDisposableStackPrototypeAdopt {
        function: FunctionOrdinal(727) => BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_ADOPT_FUNCTION_ID,
        debug: "AsyncDisposableStack.prototype.adopt",
        flags: [],
        installer: None,
        native: "adopt",
    }
    AsyncDisposableStackPrototypeDefer {
        function: FunctionOrdinal(728) => BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_DEFER_FUNCTION_ID,
        debug: "AsyncDisposableStack.prototype.defer",
        flags: [],
        installer: None,
        native: "defer",
    }
    AsyncDisposableStackPrototypeMove {
        function: FunctionOrdinal(729) => BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_MOVE_FUNCTION_ID,
        debug: "AsyncDisposableStack.prototype.move",
        flags: [],
        installer: None,
        native: "move",
    }
    AsyncDisposableStackPrototypeDisposeAsync {
        function: FunctionOrdinal(730) => BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_DISPOSE_ASYNC_FUNCTION_ID,
        debug: "AsyncDisposableStack.prototype.disposeAsync",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "disposeAsync",
    }
    AsyncDisposableStackPrototypeDisposedGetter {
        function: FunctionOrdinal(731) => BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_DISPOSED_GETTER_FUNCTION_ID,
        debug: "get AsyncDisposableStack.prototype.disposed",
        flags: [],
        installer: None,
        native: "get disposed",
    }
    AsyncDisposableStackDisposeAsyncFulfilled {
        function: FunctionOrdinal(732) => BUILTIN_ASYNC_DISPOSABLE_STACK_DISPOSE_ASYNC_FULFILLED_FUNCTION_ID,
        debug: "AsyncDisposableStack disposeAsync Fulfilled Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    AsyncDisposableStackDisposeAsyncRejected {
        function: FunctionOrdinal(733) => BUILTIN_ASYNC_DISPOSABLE_STACK_DISPOSE_ASYNC_REJECTED_FUNCTION_ID,
        debug: "AsyncDisposableStack disposeAsync Rejected Function",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    SetConstructor {
        function: FunctionOrdinal(734) => BUILTIN_SET_FUNCTION_ID,
        global: GlobalOrdinal(35),
        global_name: SET_NAME,
        debug: SET_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: Set,
        native: SET_NAME,
    }
    SetSpeciesGetter {
        function: FunctionOrdinal(735) => BUILTIN_SET_SPECIES_GETTER_FUNCTION_ID,
        debug: "get Set [Symbol.species]",
        flags: [],
        installer: None,
        native: "get [Symbol.species]",
    }
    SetPrototypeAdd {
        function: FunctionOrdinal(736) => BUILTIN_SET_PROTOTYPE_ADD_FUNCTION_ID,
        debug: "Set.prototype.add",
        flags: [],
        installer: None,
        native: "add",
    }
    SetPrototypeClear {
        function: FunctionOrdinal(737) => BUILTIN_SET_PROTOTYPE_CLEAR_FUNCTION_ID,
        debug: "Set.prototype.clear",
        flags: [],
        installer: None,
        native: "clear",
    }
    SetPrototypeDelete {
        function: FunctionOrdinal(738) => BUILTIN_SET_PROTOTYPE_DELETE_FUNCTION_ID,
        debug: "Set.prototype.delete",
        flags: [],
        installer: None,
        native: "delete",
    }
    SetPrototypeDifference {
        function: FunctionOrdinal(739) => BUILTIN_SET_PROTOTYPE_DIFFERENCE_FUNCTION_ID,
        debug: "Set.prototype.difference",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "difference",
    }
    SetPrototypeForEach {
        function: FunctionOrdinal(740) => BUILTIN_SET_PROTOTYPE_FOR_EACH_FUNCTION_ID,
        debug: "Set.prototype.forEach",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "forEach",
    }
    SetPrototypeIntersection {
        function: FunctionOrdinal(741) => BUILTIN_SET_PROTOTYPE_INTERSECTION_FUNCTION_ID,
        debug: "Set.prototype.intersection",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "intersection",
    }
    SetPrototypeIsDisjointFrom {
        function: FunctionOrdinal(742) => BUILTIN_SET_PROTOTYPE_IS_DISJOINT_FROM_FUNCTION_ID,
        debug: "Set.prototype.isDisjointFrom",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "isDisjointFrom",
    }
    SetPrototypeIsSubsetOf {
        function: FunctionOrdinal(743) => BUILTIN_SET_PROTOTYPE_IS_SUBSET_OF_FUNCTION_ID,
        debug: "Set.prototype.isSubsetOf",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "isSubsetOf",
    }
    SetPrototypeIsSupersetOf {
        function: FunctionOrdinal(744) => BUILTIN_SET_PROTOTYPE_IS_SUPERSET_OF_FUNCTION_ID,
        debug: "Set.prototype.isSupersetOf",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "isSupersetOf",
    }
    SetPrototypeSymmetricDifference {
        function: FunctionOrdinal(745) => BUILTIN_SET_PROTOTYPE_SYMMETRIC_DIFFERENCE_FUNCTION_ID,
        debug: "Set.prototype.symmetricDifference",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "symmetricDifference",
    }
    SetPrototypeUnion {
        function: FunctionOrdinal(746) => BUILTIN_SET_PROTOTYPE_UNION_FUNCTION_ID,
        debug: "Set.prototype.union",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "union",
    }
    SetPrototypeValues {
        function: FunctionOrdinal(747) => BUILTIN_SET_PROTOTYPE_VALUES_FUNCTION_ID,
        debug: "Set.prototype.values",
        flags: [],
        installer: None,
        native: "values",
    }
    SetPrototypeEntries {
        function: FunctionOrdinal(748) => BUILTIN_SET_PROTOTYPE_ENTRIES_FUNCTION_ID,
        debug: "Set.prototype.entries",
        flags: [],
        installer: None,
        native: "entries",
    }
    SetIteratorNext {
        function: FunctionOrdinal(749) => BUILTIN_SET_ITERATOR_NEXT_FUNCTION_ID,
        debug: "Set Iterator.prototype.next",
        flags: [],
        installer: None,
        native: "next",
    }
    SetPrototypeHas {
        function: FunctionOrdinal(750) => BUILTIN_SET_PROTOTYPE_HAS_FUNCTION_ID,
        debug: "Set.prototype.has",
        flags: [],
        installer: None,
        native: "has",
    }
    SetPrototypeSizeGetter {
        function: FunctionOrdinal(751) => BUILTIN_SET_PROTOTYPE_SIZE_GETTER_FUNCTION_ID,
        debug: "get Set.prototype.size",
        flags: [],
        installer: None,
        native: "get size",
    }
    SymbolConstructor {
        function: FunctionOrdinal(752) => BUILTIN_SYMBOL_FUNCTION_ID,
        global: GlobalOrdinal(36),
        global_name: SYMBOL_NAME,
        debug: SYMBOL_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: Symbol,
        native: SYMBOL_NAME,
    }
    SymbolFor {
        function: FunctionOrdinal(753) => BUILTIN_SYMBOL_FOR_FUNCTION_ID,
        debug: "Symbol.for",
        flags: [STATIC_METHOD],
        installer: None,
        native: "for",
    }
    SymbolKeyFor {
        function: FunctionOrdinal(754) => BUILTIN_SYMBOL_KEY_FOR_FUNCTION_ID,
        debug: "Symbol.keyFor",
        flags: [STATIC_METHOD],
        installer: None,
        native: "keyFor",
    }
    SymbolPrototypeDescriptionGetter {
        function: FunctionOrdinal(755) => "$builtin.Symbol.prototype.description",
        debug: "get Symbol.prototype.description",
        flags: [],
        installer: None,
        native: "get description",
    }
    SymbolPrototypeToString {
        function: FunctionOrdinal(756) => "$builtin.Symbol.prototype.toString",
        debug: "Symbol.prototype.toString",
        flags: [],
        installer: None,
        native: "toString",
    }
    SymbolPrototypeValueOf {
        function: FunctionOrdinal(757) => "$builtin.Symbol.prototype.valueOf",
        debug: "Symbol.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    SymbolPrototypeToPrimitive {
        function: FunctionOrdinal(758) => "$builtin.Symbol.prototype.toPrimitive",
        debug: "Symbol.prototype[Symbol.toPrimitive]",
        flags: [],
        installer: None,
        native: "[Symbol.toPrimitive]",
    }
    ErrorConstructor {
        function: FunctionOrdinal(759) => BUILTIN_ERROR_FUNCTION_ID,
        global: GlobalOrdinal(37),
        global_name: ERROR_NAME,
        debug: ERROR_NAME,
        flags: [CONSTRUCTABLE, ERROR_CONSTRUCTOR, SYNCHRONOUS_USER_CODE],
        installer: Error,
        native: ERROR_NAME,
    }
    ErrorIsError {
        function: FunctionOrdinal(760) => BUILTIN_ERROR_IS_ERROR_FUNCTION_ID,
        debug: "Error.isError",
        flags: [STATIC_METHOD],
        installer: None,
        native: "isError",
    }
    EvalErrorConstructor {
        function: FunctionOrdinal(761) => BUILTIN_EVAL_ERROR_FUNCTION_ID,
        global: GlobalOrdinal(38),
        global_name: EVAL_ERROR_NAME,
        debug: EVAL_ERROR_NAME,
        flags: [CONSTRUCTABLE, ERROR_CONSTRUCTOR, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: EVAL_ERROR_NAME,
    }
    AggregateErrorConstructor {
        function: FunctionOrdinal(762) => BUILTIN_AGGREGATE_ERROR_FUNCTION_ID,
        global: GlobalOrdinal(39),
        global_name: AGGREGATE_ERROR_NAME,
        debug: AGGREGATE_ERROR_NAME,
        flags: [CONSTRUCTABLE, ERROR_CONSTRUCTOR, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: AGGREGATE_ERROR_NAME,
    }
    SuppressedErrorConstructor {
        function: FunctionOrdinal(763) => BUILTIN_SUPPRESSED_ERROR_FUNCTION_ID,
        global: GlobalOrdinal(40),
        global_name: SUPPRESSED_ERROR_NAME,
        debug: SUPPRESSED_ERROR_NAME,
        flags: [CONSTRUCTABLE, ERROR_CONSTRUCTOR, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: SUPPRESSED_ERROR_NAME,
    }
    RangeErrorConstructor {
        function: FunctionOrdinal(764) => BUILTIN_RANGE_ERROR_FUNCTION_ID,
        global: GlobalOrdinal(41),
        global_name: RANGE_ERROR_NAME,
        debug: RANGE_ERROR_NAME,
        flags: [CONSTRUCTABLE, ERROR_CONSTRUCTOR, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: RANGE_ERROR_NAME,
    }
    SyntaxErrorConstructor {
        function: FunctionOrdinal(765) => BUILTIN_SYNTAX_ERROR_FUNCTION_ID,
        global: GlobalOrdinal(42),
        global_name: SYNTAX_ERROR_NAME,
        debug: SYNTAX_ERROR_NAME,
        flags: [CONSTRUCTABLE, ERROR_CONSTRUCTOR, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: SYNTAX_ERROR_NAME,
    }
    TypeErrorConstructor {
        function: FunctionOrdinal(766) => BUILTIN_TYPE_ERROR_FUNCTION_ID,
        global: GlobalOrdinal(43),
        global_name: TYPE_ERROR_NAME,
        debug: TYPE_ERROR_NAME,
        flags: [CONSTRUCTABLE, ERROR_CONSTRUCTOR, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: TYPE_ERROR_NAME,
    }
    URIErrorConstructor {
        function: FunctionOrdinal(767) => BUILTIN_URI_ERROR_FUNCTION_ID,
        global: GlobalOrdinal(44),
        global_name: URI_ERROR_NAME,
        debug: URI_ERROR_NAME,
        flags: [CONSTRUCTABLE, ERROR_CONSTRUCTOR, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: URI_ERROR_NAME,
    }
    ReferenceErrorConstructor {
        function: FunctionOrdinal(768) => BUILTIN_REFERENCE_ERROR_FUNCTION_ID,
        global: GlobalOrdinal(45),
        global_name: REFERENCE_ERROR_NAME,
        debug: REFERENCE_ERROR_NAME,
        flags: [CONSTRUCTABLE, ERROR_CONSTRUCTOR, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: REFERENCE_ERROR_NAME,
    }
    ErrorPrototypeToString {
        function: FunctionOrdinal(769) => BUILTIN_ERROR_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Error.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toString",
    }
    ThrowTypeError {
        function: FunctionOrdinal(770) => BUILTIN_THROW_TYPE_ERROR_FUNCTION_ID,
        debug: "%ThrowTypeError%",
        flags: [ALWAYS_THROWS],
        installer: None,
        native: "",
    }
    AbstractModuleSourceConstructor {
        function: FunctionOrdinal(893) => "$builtin.AbstractModuleSource",
        debug: "%AbstractModuleSource%",
        flags: [CONSTRUCTABLE, ALWAYS_THROWS],
        installer: None,
        native: "AbstractModuleSource",
    }
    AbstractModuleSourcePrototypeToStringTagGetter {
        function: FunctionOrdinal(894) => "$builtin.AbstractModuleSource.prototype.@@toStringTag",
        debug: "%AbstractModuleSource%.prototype[@@toStringTag]",
        flags: [],
        installer: None,
        native: "get [Symbol.toStringTag]",
    }
    Escape {
        function: FunctionOrdinal(771) => BUILTIN_ESCAPE_FUNCTION_ID,
        global: GlobalOrdinal(46),
        global_name: ESCAPE_NAME,
        debug: ESCAPE_NAME,
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: ESCAPE_NAME,
    }
    Unescape {
        function: FunctionOrdinal(772) => BUILTIN_UNESCAPE_FUNCTION_ID,
        global: GlobalOrdinal(47),
        global_name: UNESCAPE_NAME,
        debug: UNESCAPE_NAME,
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: UNESCAPE_NAME,
    }
    EncodeUri {
        function: FunctionOrdinal(773) => BUILTIN_ENCODE_URI_FUNCTION_ID,
        global: GlobalOrdinal(48),
        global_name: ENCODE_URI_NAME,
        debug: ENCODE_URI_NAME,
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: ENCODE_URI_NAME,
    }
    EncodeUriComponent {
        function: FunctionOrdinal(774) => BUILTIN_ENCODE_URI_COMPONENT_FUNCTION_ID,
        global: GlobalOrdinal(49),
        global_name: ENCODE_URI_COMPONENT_NAME,
        debug: ENCODE_URI_COMPONENT_NAME,
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: ENCODE_URI_COMPONENT_NAME,
    }
    DecodeUri {
        function: FunctionOrdinal(775) => BUILTIN_DECODE_URI_FUNCTION_ID,
        global: GlobalOrdinal(50),
        global_name: DECODE_URI_NAME,
        debug: DECODE_URI_NAME,
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: DECODE_URI_NAME,
    }
    DecodeUriComponent {
        function: FunctionOrdinal(776) => BUILTIN_DECODE_URI_COMPONENT_FUNCTION_ID,
        global: GlobalOrdinal(51),
        global_name: DECODE_URI_COMPONENT_NAME,
        debug: DECODE_URI_COMPONENT_NAME,
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: DECODE_URI_COMPONENT_NAME,
    }
    DisposableStackConstructor {
        function: FunctionOrdinal(777) => BUILTIN_DISPOSABLE_STACK_FUNCTION_ID,
        global: GlobalOrdinal(52),
        global_name: DISPOSABLE_STACK_NAME,
        debug: DISPOSABLE_STACK_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: DisposableStack,
        native: DISPOSABLE_STACK_NAME,
    }
    DisposableStackPrototypeUse {
        function: FunctionOrdinal(778) => BUILTIN_DISPOSABLE_STACK_PROTOTYPE_USE_FUNCTION_ID,
        debug: "DisposableStack.prototype.use",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "use",
    }
    DisposableStackPrototypeAdopt {
        function: FunctionOrdinal(779) => BUILTIN_DISPOSABLE_STACK_PROTOTYPE_ADOPT_FUNCTION_ID,
        debug: "DisposableStack.prototype.adopt",
        flags: [],
        installer: None,
        native: "adopt",
    }
    DisposableStackPrototypeDefer {
        function: FunctionOrdinal(780) => BUILTIN_DISPOSABLE_STACK_PROTOTYPE_DEFER_FUNCTION_ID,
        debug: "DisposableStack.prototype.defer",
        flags: [],
        installer: None,
        native: "defer",
    }
    DisposableStackPrototypeMove {
        function: FunctionOrdinal(781) => BUILTIN_DISPOSABLE_STACK_PROTOTYPE_MOVE_FUNCTION_ID,
        debug: "DisposableStack.prototype.move",
        flags: [],
        installer: None,
        native: "move",
    }
    DisposableStackPrototypeDispose {
        function: FunctionOrdinal(782) => BUILTIN_DISPOSABLE_STACK_PROTOTYPE_DISPOSE_FUNCTION_ID,
        debug: "DisposableStack.prototype.dispose",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "dispose",
    }
    DisposableStackPrototypeDisposedGetter {
        function: FunctionOrdinal(783) => BUILTIN_DISPOSABLE_STACK_PROTOTYPE_DISPOSED_GETTER_FUNCTION_ID,
        debug: "get DisposableStack.prototype.disposed",
        flags: [],
        installer: None,
        native: "get disposed",
    }
    FunctionPrototype {
        function: FunctionOrdinal(784) => BUILTIN_FUNCTION_PROTOTYPE_FUNCTION_ID,
        debug: "%Function.prototype%",
        flags: [],
        installer: None,
        native: "",
    }
    FunctionPrototypeSymbolHasInstance {
        function: FunctionOrdinal(785) => BUILTIN_FUNCTION_PROTOTYPE_SYMBOL_HAS_INSTANCE_FUNCTION_ID,
        debug: "Function.prototype[Symbol.hasInstance]",
        flags: [],
        installer: None,
        native: "[Symbol.hasInstance]",
    }
    TypedArrayConstructor {
        function: FunctionOrdinal(786) => BUILTIN_TYPED_ARRAY_CONSTRUCTOR_FUNCTION_ID,
        debug: "%TypedArray%",
        flags: [CONSTRUCTABLE, ALWAYS_THROWS],
        installer: None,
        native: TYPED_ARRAY_NAME,
    }
    RegExpStringIteratorNext {
        function: FunctionOrdinal(787) => BUILTIN_REGEXP_STRING_ITERATOR_NEXT_FUNCTION_ID,
        debug: "RegExp String Iterator.prototype.next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    TemporalZonedDateTimeCompare {
        function: FunctionOrdinal(788) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_COMPARE_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.compare",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "compare",
    }
    TemporalZonedDateTimePrototypeDayOfWeekGetter {
        function: FunctionOrdinal(789) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAY_OF_WEEK_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.dayOfWeek",
        flags: [INTL_HOST],
        installer: None,
        native: "get dayOfWeek",
    }
    TemporalZonedDateTimePrototypeDayOfYearGetter {
        function: FunctionOrdinal(790) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAY_OF_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.dayOfYear",
        flags: [INTL_HOST],
        installer: None,
        native: "get dayOfYear",
    }
    TemporalZonedDateTimePrototypeWeekOfYearGetter {
        function: FunctionOrdinal(791) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_WEEK_OF_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.weekOfYear",
        flags: [INTL_HOST],
        installer: None,
        native: "get weekOfYear",
    }
    TemporalZonedDateTimePrototypeYearOfWeekGetter {
        function: FunctionOrdinal(792) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_YEAR_OF_WEEK_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.yearOfWeek",
        flags: [INTL_HOST],
        installer: None,
        native: "get yearOfWeek",
    }
    TemporalZonedDateTimePrototypeDaysInWeekGetter {
        function: FunctionOrdinal(793) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAYS_IN_WEEK_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.daysInWeek",
        flags: [INTL_HOST],
        installer: None,
        native: "get daysInWeek",
    }
    TemporalZonedDateTimePrototypeDaysInMonthGetter {
        function: FunctionOrdinal(794) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAYS_IN_MONTH_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.daysInMonth",
        flags: [INTL_HOST],
        installer: None,
        native: "get daysInMonth",
    }
    TemporalZonedDateTimePrototypeDaysInYearGetter {
        function: FunctionOrdinal(795) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAYS_IN_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.daysInYear",
        flags: [INTL_HOST],
        installer: None,
        native: "get daysInYear",
    }
    TemporalZonedDateTimePrototypeMonthsInYearGetter {
        function: FunctionOrdinal(796) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MONTHS_IN_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.monthsInYear",
        flags: [INTL_HOST],
        installer: None,
        native: "get monthsInYear",
    }
    TemporalZonedDateTimePrototypeInLeapYearGetter {
        function: FunctionOrdinal(797) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_IN_LEAP_YEAR_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.inLeapYear",
        flags: [INTL_HOST],
        installer: None,
        native: "get inLeapYear",
    }
    TemporalZonedDateTimePrototypeToString {
        function: FunctionOrdinal(798) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_TO_STRING_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.toString",
        flags: [SYNCHRONOUS_USER_CODE, INTL_HOST],
        installer: None,
        native: "toString",
    }
    TemporalZonedDateTimePrototypeToJson {
        function: FunctionOrdinal(840) => "$builtin.Temporal.ZonedDateTime.prototype.toJSON",
        debug: "Temporal.ZonedDateTime.prototype.toJSON",
        flags: [INTL_HOST],
        installer: None,
        native: "toJSON",
    }
    TemporalZonedDateTimePrototypeValueOf {
        function: FunctionOrdinal(841) => "$builtin.Temporal.ZonedDateTime.prototype.valueOf",
        debug: "Temporal.ZonedDateTime.prototype.valueOf",
        flags: [],
        installer: None,
        native: "valueOf",
    }
    TemporalZonedDateTimePrototypeToLocaleString {
        function: FunctionOrdinal(842) => "$builtin.Temporal.ZonedDateTime.prototype.toLocaleString",
        debug: "Temporal.ZonedDateTime.prototype.toLocaleString",
        flags: [SYNCHRONOUS_USER_CODE, INTL_HOST],
        installer: None,
        native: "toLocaleString",
    }
    TemporalZonedDateTimePrototypeToPlainTime {
        function: FunctionOrdinal(843) => "$builtin.Temporal.ZonedDateTime.prototype.toPlainTime",
        debug: "Temporal.ZonedDateTime.prototype.toPlainTime",
        flags: [INTL_HOST],
        installer: None,
        native: "toPlainTime",
    }
    TemporalZonedDateTimePrototypeWithPlainTime {
        function: FunctionOrdinal(844) => "$builtin.Temporal.ZonedDateTime.prototype.withPlainTime",
        debug: "Temporal.ZonedDateTime.prototype.withPlainTime",
        flags: [SYNCHRONOUS_USER_CODE, INTL_HOST],
        installer: None,
        native: "withPlainTime",
    }
    TemporalZonedDateTimePrototypeRound {
        function: FunctionOrdinal(799) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_ROUND_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.round",
        flags: [SYNCHRONOUS_USER_CODE, INTL_HOST],
        installer: None,
        native: "round",
    }
    TemporalZonedDateTimePrototypeGetTimeZoneTransition {
        function: FunctionOrdinal(800) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_GET_TIME_ZONE_TRANSITION_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.getTimeZoneTransition",
        flags: [SYNCHRONOUS_USER_CODE, INTL_HOST],
        installer: None,
        native: "getTimeZoneTransition",
    }
    TemporalZonedDateTimePrototypeHoursInDayGetter {
        function: FunctionOrdinal(801) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_HOURS_IN_DAY_GETTER_FUNCTION_ID,
        debug: "get Temporal.ZonedDateTime.prototype.hoursInDay",
        flags: [INTL_HOST],
        installer: None,
        native: "get hoursInDay",
    }
    TemporalZonedDateTimePrototypeStartOfDay {
        function: FunctionOrdinal(802) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_START_OF_DAY_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.startOfDay",
        flags: [INTL_HOST],
        installer: None,
        native: "startOfDay",
    }
    TemporalPlainDatePrototypeToZonedDateTime {
        function: FunctionOrdinal(803) => BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_ZONED_DATE_TIME_FUNCTION_ID,
        debug: "Temporal.PlainDate.prototype.toZonedDateTime",
        flags: [SYNCHRONOUS_USER_CODE, INTL_HOST],
        installer: None,
        native: "toZonedDateTime",
    }
    Uint8ArrayFromBase64 {
        function: FunctionOrdinal(804) => BUILTIN_UINT8_ARRAY_FROM_BASE64_FUNCTION_ID,
        debug: "Uint8Array.fromBase64",
        flags: [STATIC_METHOD, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "fromBase64",
    }
    Uint8ArrayFromHex {
        function: FunctionOrdinal(805) => BUILTIN_UINT8_ARRAY_FROM_HEX_FUNCTION_ID,
        debug: "Uint8Array.fromHex",
        flags: [STATIC_METHOD],
        installer: None,
        native: "fromHex",
    }
    Uint8ArrayPrototypeSetFromBase64 {
        function: FunctionOrdinal(806) => BUILTIN_UINT8_ARRAY_PROTOTYPE_SET_FROM_BASE64_FUNCTION_ID,
        debug: "Uint8Array.prototype.setFromBase64",
        flags: [SYNCHRONOUS_USER_CODE, INDEXED_RECEIVER_MUTATION],
        installer: None,
        native: "setFromBase64",
    }
    Uint8ArrayPrototypeSetFromHex {
        function: FunctionOrdinal(807) => BUILTIN_UINT8_ARRAY_PROTOTYPE_SET_FROM_HEX_FUNCTION_ID,
        debug: "Uint8Array.prototype.setFromHex",
        flags: [INDEXED_RECEIVER_MUTATION],
        installer: None,
        native: "setFromHex",
    }
    Uint8ArrayPrototypeToBase64 {
        function: FunctionOrdinal(808) => BUILTIN_UINT8_ARRAY_PROTOTYPE_TO_BASE64_FUNCTION_ID,
        debug: "Uint8Array.prototype.toBase64",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "toBase64",
    }
    Uint8ArrayPrototypeToHex {
        function: FunctionOrdinal(809) => BUILTIN_UINT8_ARRAY_PROTOTYPE_TO_HEX_FUNCTION_ID,
        debug: "Uint8Array.prototype.toHex",
        flags: [],
        installer: None,
        native: "toHex",
    }
    TemporalZonedDateTimePrototypeWith {
        function: FunctionOrdinal(810) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_WITH_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.with",
        flags: [SYNCHRONOUS_USER_CODE, INTL_HOST],
        installer: None,
        native: "with",
    }
    TemporalZonedDateTimePrototypeToPlainDate {
        function: FunctionOrdinal(811) => BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_TO_PLAIN_DATE_FUNCTION_ID,
        debug: "Temporal.ZonedDateTime.prototype.toPlainDate",
        flags: [INTL_HOST],
        installer: None,
        native: "toPlainDate",
    }
    TypedArrayPrototypeFill {
        function: FunctionOrdinal(812) => BUILTIN_TYPED_ARRAY_PROTOTYPE_FILL_FUNCTION_ID,
        debug: "TypedArray.prototype.fill",
        flags: [INDEXED_RECEIVER_MUTATION, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "fill",
    }
    Float16ArrayConstructor {
        function: FunctionOrdinal(821) => BUILTIN_FLOAT16_ARRAY_FUNCTION_ID,
        global: GlobalOrdinal(53),
        global_name: FLOAT16_ARRAY_NAME,
        debug: FLOAT16_ARRAY_NAME,
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: FLOAT16_ARRAY_NAME,
    }
    IntlSupportedValuesOf {
        function: FunctionOrdinal(849) => BUILTIN_INTL_SUPPORTED_VALUES_OF_FUNCTION_ID,
        debug: "Intl.supportedValuesOf",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "supportedValuesOf",
    }
    ReadableStreamConstructor {
        function: FunctionOrdinal(923) => "$builtin.ReadableStreamConstructor",
        global: GlobalOrdinal(62),
        global_name: "ReadableStream",
        debug: "ReadableStream",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "ReadableStream",
    }
    ReadableStreamGetReader {
        function: FunctionOrdinal(924) => "$builtin.ReadableStreamGetReader",
        debug: "ReadableStream.prototype.getReader",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "getReader",
    }
    ReadableStreamCancel {
        function: FunctionOrdinal(925) => "$builtin.ReadableStreamCancel",
        debug: "ReadableStream.prototype.cancel",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "cancel",
    }
    ReadableStreamLocked {
        function: FunctionOrdinal(926) => "$builtin.ReadableStreamLocked",
        debug: "get ReadableStream.prototype.locked",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get locked",
    }
    ReadableStreamDefaultReaderConstructor {
        function: FunctionOrdinal(927) => "$builtin.ReadableStreamDefaultReaderConstructor",
        global: GlobalOrdinal(63),
        global_name: "ReadableStreamDefaultReader",
        debug: "ReadableStreamDefaultReader",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "ReadableStreamDefaultReader",
    }
    ReadableStreamDefaultReaderRead {
        function: FunctionOrdinal(928) => "$builtin.ReadableStreamDefaultReaderRead",
        debug: "ReadableStreamDefaultReader.prototype.read",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "read",
    }
    ReadableStreamDefaultReaderCancel {
        function: FunctionOrdinal(929) => "$builtin.ReadableStreamDefaultReaderCancel",
        debug: "ReadableStreamDefaultReader.prototype.cancel",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "cancel",
    }
    ReadableStreamDefaultReaderReleaseLock {
        function: FunctionOrdinal(930) => "$builtin.ReadableStreamDefaultReaderReleaseLock",
        debug: "ReadableStreamDefaultReader.prototype.releaseLock",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "releaseLock",
    }
    ReadableStreamDefaultReaderClosed {
        function: FunctionOrdinal(931) => "$builtin.ReadableStreamDefaultReaderClosed",
        debug: "get ReadableStreamDefaultReader.prototype.closed",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get closed",
    }
    ReadableByteStreamControllerConstructor {
        function: FunctionOrdinal(932) => "$builtin.ReadableByteStreamControllerConstructor",
        global: GlobalOrdinal(64),
        global_name: "ReadableByteStreamController",
        debug: "ReadableByteStreamController",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "ReadableByteStreamController",
    }
    ReadableByteStreamControllerEnqueue {
        function: FunctionOrdinal(933) => "$builtin.ReadableByteStreamControllerEnqueue",
        debug: "ReadableByteStreamController.prototype.enqueue",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "enqueue",
    }
    ReadableByteStreamControllerClose {
        function: FunctionOrdinal(934) => "$builtin.ReadableByteStreamControllerClose",
        debug: "ReadableByteStreamController.prototype.close",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "close",
    }
    ReadableByteStreamControllerError {
        function: FunctionOrdinal(935) => "$builtin.ReadableByteStreamControllerError",
        debug: "ReadableByteStreamController.prototype.error",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "error",
    }
    ReadableByteStreamControllerDesiredSize {
        function: FunctionOrdinal(936) => "$builtin.ReadableByteStreamControllerDesiredSize",
        debug: "get ReadableByteStreamController.prototype.desiredSize",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get desiredSize",
    }
    ReadableStreamStartFulfilled {
        function: FunctionOrdinal(937) => "$builtin.ReadableStreamStartFulfilled",
        debug: "ReadableStream.StartFulfilled",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    ReadableStreamStartRejected {
        function: FunctionOrdinal(938) => "$builtin.ReadableStreamStartRejected",
        debug: "ReadableStream.StartRejected",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    ReadableStreamPullFulfilled {
        function: FunctionOrdinal(939) => "$builtin.ReadableStreamPullFulfilled",
        debug: "ReadableStream.PullFulfilled",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    ReadableStreamPullRejected {
        function: FunctionOrdinal(940) => "$builtin.ReadableStreamPullRejected",
        debug: "ReadableStream.PullRejected",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    ReadableStreamCancelFulfilled {
        function: FunctionOrdinal(941) => "$builtin.ReadableStreamCancelFulfilled",
        debug: "ReadableStream.CancelFulfilled",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }
    ReadableStreamPullIfNeeded {
        function: FunctionOrdinal(942) => "$builtin.ReadableStreamPullIfNeeded",
        debug: "ReadableStream.PullIfNeeded",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }

    ShadowRealmConstructor {
        function: FunctionOrdinal(895) => "$builtin.ShadowRealm",
        global: GlobalOrdinal(54),
        global_name: "ShadowRealm",
        debug: "ShadowRealm",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: ShadowRealm,
        native: "ShadowRealm",
    }

    ShadowRealmPrototypeEvaluate {
        function: FunctionOrdinal(896) => "$builtin.ShadowRealm.prototype.evaluate",
        debug: "ShadowRealm.prototype.evaluate",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "evaluate",
    }

    ShadowRealmPrototypeImportValue {
        function: FunctionOrdinal(897) => "$builtin.ShadowRealm.prototype.importValue",
        debug: "ShadowRealm.prototype.importValue",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "importValue",
    }

    ShadowRealmWrappedFunctionCall {
        function: FunctionOrdinal(898) => "$builtin.ShadowRealm.WrappedFunction",
        debug: "ShadowRealm.WrappedFunction",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }

    ShadowRealmImportFulfilled {
        function: FunctionOrdinal(899) => "$builtin.ShadowRealm.ImportFulfilled",
        debug: "ShadowRealm.ImportFulfilled",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "",
    }

    ShadowRealmImportRejected {
        function: FunctionOrdinal(900) => "$builtin.ShadowRealm.ImportRejected",
        debug: "ShadowRealm.ImportRejected",
        flags: [],
        installer: None,
        native: "",
    }

    ErrorPrototypeStackGetter {
        function: FunctionOrdinal(901) => "$builtin.Error.prototype.stack.get",
        debug: "get Error.prototype.stack",
        flags: [],
        installer: None,
        native: "get stack",
    }
    ErrorPrototypeStackSetter {
        function: FunctionOrdinal(902) => "$builtin.Error.prototype.stack.set",
        debug: "set Error.prototype.stack",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "set stack",
    }
    IteratorPrototypeChunks {
        function: FunctionOrdinal(903) => "$builtin.IteratorPrototypeChunks",
        debug: "Iterator.prototype.chunks",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "chunks",
    }
    IteratorPrototypeWindows {
        function: FunctionOrdinal(904) => "$builtin.IteratorPrototypeWindows",
        debug: "Iterator.prototype.windows",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "windows",
    }
    IteratorBufferedNext {
        function: FunctionOrdinal(905) => "$builtin.IteratorBufferedNext",
        debug: "Iterator buffered helper next",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "next",
    }
    IteratorBufferedReturn {
        function: FunctionOrdinal(906) => "$builtin.IteratorBufferedReturn",
        debug: "Iterator buffered helper return",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "return",
    }
    ArrayBufferPrototypeImmutableGetter {
        function: FunctionOrdinal(907) => BUILTIN_ARRAY_BUFFER_PROTOTYPE_IMMUTABLE_GETTER_FUNCTION_ID,
        debug: "get ArrayBuffer.prototype.immutable",
        flags: [],
        installer: None,
        native: "get immutable",
    }
    IteratorPrototypeIncludes {
        function: FunctionOrdinal(908) => "$builtin.Iterator.prototype.includes",
        debug: "Iterator.prototype.includes",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "includes",
    }
    IteratorPrototypeJoin {
        function: FunctionOrdinal(909) => "$builtin.Iterator.prototype.join",
        debug: "Iterator.prototype.join",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "join",
    }
    AsyncIteratorIdentity {
        function: FunctionOrdinal(910) => "$builtin.AsyncIteratorPrototype.identity",
        debug: "Async Iterator.prototype [Symbol.asyncIterator]",
        flags: [],
        installer: None,
        native: "[Symbol.asyncIterator]",
    }
    AbortControllerConstructor {
        function: FunctionOrdinal(943) => "scriptw:web:AbortControllerConstructor",
        global: GlobalOrdinal(57),
        global_name: "AbortController",
        debug: "AbortControllerConstructor",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "AbortController",
    }
    AbortControllerPrototypeAbort {
        function: FunctionOrdinal(944) => "scriptw:web:AbortControllerPrototypeAbort",
        debug: "AbortControllerPrototypeAbort",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "abort",
    }
    AbortControllerPrototypeSignalGetter {
        function: FunctionOrdinal(945) => "scriptw:web:AbortControllerPrototypeSignalGetter",
        debug: "AbortControllerPrototypeSignalGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get signal",
    }
    AbortSignalConstructor {
        function: FunctionOrdinal(946) => "scriptw:web:AbortSignalConstructor",
        global: GlobalOrdinal(58),
        global_name: "AbortSignal",
        debug: "AbortSignalConstructor",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "AbortSignal",
    }
    AbortSignalPrototypeThrowIfAborted {
        function: FunctionOrdinal(947) => "scriptw:web:AbortSignalPrototypeThrowIfAborted",
        debug: "AbortSignalPrototypeThrowIfAborted",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "throwIfAborted",
    }
    AbortSignalAbort {
        function: FunctionOrdinal(948) => "scriptw:web:AbortSignalAbort",
        debug: "AbortSignalAbort",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "abort",
    }
    AbortSignalPrototypeAbortedGetter {
        function: FunctionOrdinal(949) => "scriptw:web:AbortSignalPrototypeAbortedGetter",
        debug: "AbortSignalPrototypeAbortedGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get aborted",
    }
    AbortSignalPrototypeReasonGetter {
        function: FunctionOrdinal(950) => "scriptw:web:AbortSignalPrototypeReasonGetter",
        debug: "AbortSignalPrototypeReasonGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get reason",
    }
    AbortSignalPrototypeOnabortGetter {
        function: FunctionOrdinal(951) => "scriptw:web:AbortSignalPrototypeOnabortGetter",
        debug: "AbortSignalPrototypeOnabortGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get onabort",
    }
    AbortSignalPrototypeOnabortSetter {
        function: FunctionOrdinal(952) => "scriptw:web:AbortSignalPrototypeOnabortSetter",
        debug: "AbortSignalPrototypeOnabortSetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "set onabort",
    }
    EventTargetConstructor {
        function: FunctionOrdinal(953) => "scriptw:web:EventTargetConstructor",
        global: GlobalOrdinal(59),
        global_name: "EventTarget",
        debug: "EventTargetConstructor",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "EventTarget",
    }
    EventTargetPrototypeAddEventListener {
        function: FunctionOrdinal(954) => "scriptw:web:EventTargetPrototypeAddEventListener",
        debug: "EventTargetPrototypeAddEventListener",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "addEventListener",
    }
    EventTargetPrototypeRemoveEventListener {
        function: FunctionOrdinal(955) => "scriptw:web:EventTargetPrototypeRemoveEventListener",
        debug: "EventTargetPrototypeRemoveEventListener",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "removeEventListener",
    }
    EventTargetPrototypeDispatchEvent {
        function: FunctionOrdinal(956) => "scriptw:web:EventTargetPrototypeDispatchEvent",
        debug: "EventTargetPrototypeDispatchEvent",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "dispatchEvent",
    }
    EventConstructor {
        function: FunctionOrdinal(957) => "scriptw:web:EventConstructor",
        global: GlobalOrdinal(60),
        global_name: "Event",
        debug: "EventConstructor",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "Event",
    }
    EventPrototypePreventDefault {
        function: FunctionOrdinal(958) => "scriptw:web:EventPrototypePreventDefault",
        debug: "EventPrototypePreventDefault",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "preventDefault",
    }
    EventPrototypeStopPropagation {
        function: FunctionOrdinal(959) => "scriptw:web:EventPrototypeStopPropagation",
        debug: "EventPrototypeStopPropagation",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "stopPropagation",
    }
    EventPrototypeStopImmediatePropagation {
        function: FunctionOrdinal(960) => "scriptw:web:EventPrototypeStopImmediatePropagation",
        debug: "EventPrototypeStopImmediatePropagation",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "stopImmediatePropagation",
    }
    EventPrototypeTypeGetter {
        function: FunctionOrdinal(961) => "scriptw:web:EventPrototypeTypeGetter",
        debug: "EventPrototypeTypeGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get type",
    }
    EventPrototypeTargetGetter {
        function: FunctionOrdinal(962) => "scriptw:web:EventPrototypeTargetGetter",
        debug: "EventPrototypeTargetGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get target",
    }
    EventPrototypeCurrentTargetGetter {
        function: FunctionOrdinal(963) => "scriptw:web:EventPrototypeCurrentTargetGetter",
        debug: "EventPrototypeCurrentTargetGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get currentTarget",
    }
    EventPrototypeDefaultPreventedGetter {
        function: FunctionOrdinal(964) => "scriptw:web:EventPrototypeDefaultPreventedGetter",
        debug: "EventPrototypeDefaultPreventedGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get defaultPrevented",
    }
    EventPrototypeEventPhaseGetter {
        function: FunctionOrdinal(965) => "scriptw:web:EventPrototypeEventPhaseGetter",
        debug: "EventPrototypeEventPhaseGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get eventPhase",
    }
    EventPrototypeBubblesGetter {
        function: FunctionOrdinal(966) => "scriptw:web:EventPrototypeBubblesGetter",
        debug: "EventPrototypeBubblesGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get bubbles",
    }
    EventPrototypeCancelableGetter {
        function: FunctionOrdinal(967) => "scriptw:web:EventPrototypeCancelableGetter",
        debug: "EventPrototypeCancelableGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get cancelable",
    }
    EventPrototypeComposedGetter {
        function: FunctionOrdinal(968) => "scriptw:web:EventPrototypeComposedGetter",
        debug: "EventPrototypeComposedGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get composed",
    }
    EventPrototypeIsTrustedGetter {
        function: FunctionOrdinal(969) => "scriptw:web:EventPrototypeIsTrustedGetter",
        debug: "EventPrototypeIsTrustedGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get isTrusted",
    }
    DOMExceptionConstructor {
        function: FunctionOrdinal(970) => "scriptw:web:DOMExceptionConstructor",
        global: GlobalOrdinal(61),
        global_name: "DOMException",
        debug: "DOMExceptionConstructor",
        flags: [CONSTRUCTABLE, SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "DOMException",
    }
    DOMExceptionPrototypeNameGetter {
        function: FunctionOrdinal(971) => "scriptw:web:DOMExceptionPrototypeNameGetter",
        debug: "DOMExceptionPrototypeNameGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get name",
    }
    DOMExceptionPrototypeMessageGetter {
        function: FunctionOrdinal(972) => "scriptw:web:DOMExceptionPrototypeMessageGetter",
        debug: "DOMExceptionPrototypeMessageGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get message",
    }
    DOMExceptionPrototypeCodeGetter {
        function: FunctionOrdinal(973) => "scriptw:web:DOMExceptionPrototypeCodeGetter",
        debug: "DOMExceptionPrototypeCodeGetter",
        flags: [SYNCHRONOUS_USER_CODE],
        installer: None,
        native: "get code",
    }


}

// Native metadata supplies both the initial public name and its frozen source
// representation. A debug label cannot silently become a malformed native name.
// Generic String methods can observe user code through borrowed receivers,
// symbol hooks or arguments. A missing effect flag on any such row is a build
// error; the two nongeneric internal-brand checks do not coerce their receiver.
const _: () = {
    let mut index = 0;
    while index < StandardBuiltinId::DECLARATION_ORDER.len() {
        let builtin = StandardBuiltinId::DECLARATION_ORDER[index];
        if let Some(name) = builtin.native_function_name() {
            assert!(super::native_name::is_catalog_native_function_name(name));
        }
        if builtin.string_prototype_method_name().is_some()
            && !matches!(
                builtin,
                StandardBuiltinId::StringPrototypeToString
                    | StandardBuiltinId::StringPrototypeValueOf
            )
        {
            assert!(builtin.may_run_user_code_synchronously());
        }
        // These invocation/coercion entries also observe user code through
        // Call, property access, Construct or ToString.
        if matches!(
            builtin,
            StandardBuiltinId::FunctionPrototypeCall
                | StandardBuiltinId::FunctionPrototypeBind
                | StandardBuiltinId::ArrayIteratorNext
                | StandardBuiltinId::TypedArrayOf
        ) {
            assert!(builtin.may_run_user_code_synchronously());
        }
        // Every Error constructor can observe NewTarget, message, options or
        // iteration. The codec family and Error.toString also perform
        // observable coercion/property operations in their native bodies.
        if builtin.is_error_constructor()
            || matches!(
                builtin,
                StandardBuiltinId::Escape
                    | StandardBuiltinId::Unescape
                    | StandardBuiltinId::EncodeUri
                    | StandardBuiltinId::EncodeUriComponent
                    | StandardBuiltinId::DecodeUri
                    | StandardBuiltinId::DecodeUriComponent
                    | StandardBuiltinId::ErrorPrototypeToString
            )
        {
            assert!(builtin.may_run_user_code_synchronously());
        }
        // Reviewed numeric conversions/formatters, Math coercion/iteration and
        // Atomics preparation can synchronously invoke JavaScript. Preserve
        // their caller-flow effects even when their normal results are scalar.
        if matches!(
            builtin,
            StandardBuiltinId::NumberConstructor
                | StandardBuiltinId::NumberPrototypeToExponential
                | StandardBuiltinId::NumberPrototypeToFixed
                | StandardBuiltinId::NumberPrototypeToPrecision
                | StandardBuiltinId::NumberPrototypeToString
                | StandardBuiltinId::NumberPrototypeToLocaleString
                | StandardBuiltinId::BigIntConstructor
                | StandardBuiltinId::BigIntAsIntN
                | StandardBuiltinId::BigIntAsUintN
                | StandardBuiltinId::BigIntPrototypeToString
                | StandardBuiltinId::BigIntPrototypeToLocaleString
                | StandardBuiltinId::GlobalIsFinite
                | StandardBuiltinId::GlobalIsNaN
                | StandardBuiltinId::MathAbs
                | StandardBuiltinId::MathAcos
                | StandardBuiltinId::MathAcosh
                | StandardBuiltinId::MathAsin
                | StandardBuiltinId::MathAsinh
                | StandardBuiltinId::MathAtan
                | StandardBuiltinId::MathAtanh
                | StandardBuiltinId::MathCbrt
                | StandardBuiltinId::MathCeil
                | StandardBuiltinId::MathClz32
                | StandardBuiltinId::MathCos
                | StandardBuiltinId::MathCosh
                | StandardBuiltinId::MathExp
                | StandardBuiltinId::MathExpm1
                | StandardBuiltinId::MathF16Round
                | StandardBuiltinId::MathFloor
                | StandardBuiltinId::MathFround
                | StandardBuiltinId::MathLog
                | StandardBuiltinId::MathLog10
                | StandardBuiltinId::MathLog1p
                | StandardBuiltinId::MathLog2
                | StandardBuiltinId::MathRound
                | StandardBuiltinId::MathSign
                | StandardBuiltinId::MathSin
                | StandardBuiltinId::MathSinh
                | StandardBuiltinId::MathSqrt
                | StandardBuiltinId::MathTan
                | StandardBuiltinId::MathTanh
                | StandardBuiltinId::MathTrunc
                | StandardBuiltinId::MathAtan2
                | StandardBuiltinId::MathHypot
                | StandardBuiltinId::MathImul
                | StandardBuiltinId::MathPow
                | StandardBuiltinId::MathSumPrecise
                | StandardBuiltinId::MathMin
                | StandardBuiltinId::MathMax
                | StandardBuiltinId::AtomicsAdd
                | StandardBuiltinId::AtomicsAnd
                | StandardBuiltinId::AtomicsCompareExchange
                | StandardBuiltinId::AtomicsExchange
                | StandardBuiltinId::AtomicsLoad
                | StandardBuiltinId::AtomicsNotify
                | StandardBuiltinId::AtomicsOr
                | StandardBuiltinId::AtomicsStore
                | StandardBuiltinId::AtomicsSub
                | StandardBuiltinId::AtomicsWait
                | StandardBuiltinId::AtomicsWaitAsync
                | StandardBuiltinId::AtomicsXor
                | StandardBuiltinId::AtomicsIsLockFree
        ) {
            assert!(builtin.may_run_user_code_synchronously());
        }
        // BinaryData constructors and these methods observe ToIndex, element
        // conversion, species, NewTarget or iteration. Their metadata must
        // invalidate caller facts before any native body can invoke a hook.
        if matches!(
            builtin,
            StandardBuiltinId::ArrayBufferConstructor
                | StandardBuiltinId::SharedArrayBufferConstructor
                | StandardBuiltinId::DataViewConstructor
                | StandardBuiltinId::ArrayBufferPrototypeResize
                | StandardBuiltinId::ArrayBufferPrototypeSlice
                | StandardBuiltinId::ArrayBufferPrototypeTransfer
                | StandardBuiltinId::ArrayBufferPrototypeTransferToFixedLength
                | StandardBuiltinId::ArrayBufferPrototypeTransferToImmutable
                | StandardBuiltinId::ArrayBufferPrototypeSliceToImmutable
                | StandardBuiltinId::SharedArrayBufferPrototypeGrow
                | StandardBuiltinId::SharedArrayBufferPrototypeSlice
                | StandardBuiltinId::TypedArrayPrototypeSubarray
                | StandardBuiltinId::Float64ArrayConstructor
                | StandardBuiltinId::Float32ArrayConstructor
                | StandardBuiltinId::Float16ArrayConstructor
                | StandardBuiltinId::Int32ArrayConstructor
                | StandardBuiltinId::Int16ArrayConstructor
                | StandardBuiltinId::Int8ArrayConstructor
                | StandardBuiltinId::Uint32ArrayConstructor
                | StandardBuiltinId::Uint16ArrayConstructor
                | StandardBuiltinId::Uint8ArrayConstructor
                | StandardBuiltinId::Uint8ClampedArrayConstructor
                | StandardBuiltinId::BigInt64ArrayConstructor
                | StandardBuiltinId::BigUint64ArrayConstructor
                | StandardBuiltinId::DataViewPrototypeGetUint8
                | StandardBuiltinId::DataViewPrototypeSetUint8
                | StandardBuiltinId::DataViewPrototypeGetInt8
                | StandardBuiltinId::DataViewPrototypeSetInt8
                | StandardBuiltinId::DataViewPrototypeGetUint16
                | StandardBuiltinId::DataViewPrototypeSetUint16
                | StandardBuiltinId::DataViewPrototypeGetInt16
                | StandardBuiltinId::DataViewPrototypeSetInt16
                | StandardBuiltinId::DataViewPrototypeGetUint32
                | StandardBuiltinId::DataViewPrototypeSetUint32
                | StandardBuiltinId::DataViewPrototypeGetInt32
                | StandardBuiltinId::DataViewPrototypeSetInt32
                | StandardBuiltinId::DataViewPrototypeGetFloat16
                | StandardBuiltinId::DataViewPrototypeSetFloat16
                | StandardBuiltinId::DataViewPrototypeGetFloat32
                | StandardBuiltinId::DataViewPrototypeSetFloat32
                | StandardBuiltinId::DataViewPrototypeGetFloat64
                | StandardBuiltinId::DataViewPrototypeSetFloat64
                | StandardBuiltinId::DataViewPrototypeGetBigInt64
                | StandardBuiltinId::DataViewPrototypeSetBigInt64
                | StandardBuiltinId::DataViewPrototypeGetBigUint64
                | StandardBuiltinId::DataViewPrototypeSetBigUint64
                | StandardBuiltinId::TypedArrayFrom
                | StandardBuiltinId::TypedArrayOf
        ) {
            assert!(builtin.may_run_user_code_synchronously());
        }
        // These bodies use runtime tags or internal primitive slots and do not
        // coerce inputs. Randomness remains a distinct host effect.
        if matches!(
            builtin,
            StandardBuiltinId::NumberIsInteger
                | StandardBuiltinId::NumberIsSafeInteger
                | StandardBuiltinId::NumberIsFinite
                | StandardBuiltinId::NumberIsNaN
                | StandardBuiltinId::NumberPrototypeValueOf
                | StandardBuiltinId::BigIntPrototypeValueOf
                | StandardBuiltinId::MathRandom
                | StandardBuiltinId::AtomicsPause
        ) {
            assert!(!builtin.may_run_user_code_synchronously());
        }
        index += 1;
    }
};

impl StandardBuiltinId {
    /// Whether this builtin body calls the pinned Intl provider. Import planning
    /// uses compiled bodies, including dependencies discovered during emission.
    pub const fn requires_intl_host(self) -> bool {
        self.flags().contains(BuiltinFlags::INTL_HOST)
    }

    /// Whether this builtin reads the realm's host randomness provider. This
    /// catalog bit is the sole authority for importing `scriptw_host.random_f64`.
    pub const fn requires_random(self) -> bool {
        self.flags().contains(BuiltinFlags::RANDOM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_values_method_is_nonconstructible_observable_and_wire_free() {
        let method = StandardBuiltinId::IntlSupportedValuesOf;
        assert_eq!(method.native_function_name(), Some("supportedValuesOf"));
        assert_eq!(
            method.function_id(),
            crate::names::BUILTIN_INTL_SUPPORTED_VALUES_OF_FUNCTION_ID
        );
        assert!(!method.constructable());
        assert!(method.may_run_user_code_synchronously());
        assert!(!method.requires_intl_host());
        assert_eq!(method.intrinsic_installer(), StandardBuiltinInstaller::None);
    }

    #[test]
    fn intl_provider_callers_declare_the_host_import() {
        let callers = StandardBuiltinId::all_functions()
            .iter()
            .copied()
            .filter(|builtin| builtin.requires_intl_host())
            .collect::<Vec<_>>();
        assert_eq!(
            callers,
            [
                StandardBuiltinId::DateConstructor,
                StandardBuiltinId::DateParse,
                StandardBuiltinId::DatePrototypeGetFullYear,
                StandardBuiltinId::DatePrototypeGetMonth,
                StandardBuiltinId::DatePrototypeGetDate,
                StandardBuiltinId::DatePrototypeGetDay,
                StandardBuiltinId::DatePrototypeGetHours,
                StandardBuiltinId::DatePrototypeGetMinutes,
                StandardBuiltinId::DatePrototypeGetSeconds,
                StandardBuiltinId::DatePrototypeGetMilliseconds,
                StandardBuiltinId::DatePrototypeGetTimezoneOffset,
                StandardBuiltinId::DatePrototypeGetYear,
                StandardBuiltinId::DatePrototypeSetYear,
                StandardBuiltinId::DatePrototypeSetFullYear,
                StandardBuiltinId::DatePrototypeSetMonth,
                StandardBuiltinId::DatePrototypeSetDate,
                StandardBuiltinId::DatePrototypeSetHours,
                StandardBuiltinId::DatePrototypeSetMinutes,
                StandardBuiltinId::DatePrototypeSetSeconds,
                StandardBuiltinId::DatePrototypeSetMilliseconds,
                StandardBuiltinId::DatePrototypeToDateString,
                StandardBuiltinId::DatePrototypeToLocaleDateString,
                StandardBuiltinId::DatePrototypeToLocaleString,
                StandardBuiltinId::DatePrototypeToLocaleTimeString,
                StandardBuiltinId::DatePrototypeToTimeString,
                StandardBuiltinId::DatePrototypeToString,
                StandardBuiltinId::TemporalPlainDateFrom,
                StandardBuiltinId::TemporalPlainDateCompare,
                StandardBuiltinId::TemporalPlainDatePrototypeEquals,
                StandardBuiltinId::TemporalPlainDatePrototypeToLocaleString,
                StandardBuiltinId::TemporalPlainDatePrototypeUntil,
                StandardBuiltinId::TemporalPlainDatePrototypeSince,
                StandardBuiltinId::TemporalPlainDatePrototypeToPlainDateTime,
                StandardBuiltinId::TemporalPlainYearMonthPrototypeToLocaleString,
                StandardBuiltinId::TemporalPlainMonthDayPrototypeToLocaleString,
                StandardBuiltinId::TemporalPlainTimeFrom,
                StandardBuiltinId::TemporalPlainTimeCompare,
                StandardBuiltinId::TemporalPlainTimePrototypeUntil,
                StandardBuiltinId::TemporalPlainTimePrototypeSince,
                StandardBuiltinId::TemporalPlainTimePrototypeEquals,
                StandardBuiltinId::TemporalPlainTimePrototypeToLocaleString,
                StandardBuiltinId::TemporalPlainDateTimePrototypeWithPlainTime,
                StandardBuiltinId::TemporalPlainDateTimePrototypeToLocaleString,
                StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime,
                StandardBuiltinId::TemporalDurationCompare,
                StandardBuiltinId::TemporalDurationPrototypeRound,
                StandardBuiltinId::TemporalDurationPrototypeTotal,
                StandardBuiltinId::TemporalDurationPrototypeToLocaleString,
                StandardBuiltinId::TemporalNowZonedDateTimeIso,
                StandardBuiltinId::TemporalInstantPrototypeToString,
                StandardBuiltinId::TemporalInstantPrototypeToJson,
                StandardBuiltinId::TemporalZonedDateTimeConstructor,
                StandardBuiltinId::TemporalZonedDateTimeFrom,
                StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetNanosecondsGetter,
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
                StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDateTime,
                StandardBuiltinId::TemporalZonedDateTimePrototypeWithTimeZone,
                StandardBuiltinId::TemporalZonedDateTimePrototypeAdd,
                StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract,
                StandardBuiltinId::TemporalZonedDateTimePrototypeUntil,
                StandardBuiltinId::TemporalZonedDateTimePrototypeSince,
                StandardBuiltinId::IntlGetCanonicalLocales,
                StandardBuiltinId::IntlLocaleConstructor,
                StandardBuiltinId::IntlDateTimeFormatConstructor,
                StandardBuiltinId::IntlDateTimeFormatSupportedLocalesOf,
                StandardBuiltinId::IntlDateTimeFormatPrototypeFormatToParts,
                StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRange,
                StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRangeToParts,
                StandardBuiltinId::IntlDateTimeFormatBoundFormat,
                StandardBuiltinId::BigIntPrototypeToLocaleString,
                StandardBuiltinId::NumberPrototypeToLocaleString,
                StandardBuiltinId::StringPrototypeLocaleCompare,
                StandardBuiltinId::StringPrototypeToLocaleLowerCase,
                StandardBuiltinId::StringPrototypeToLocaleUpperCase,
                StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfWeekGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfYearGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeWeekOfYearGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeYearOfWeekGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInWeekGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInMonthGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInYearGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeMonthsInYearGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeInLeapYearGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeToString,
                StandardBuiltinId::TemporalZonedDateTimePrototypeRound,
                StandardBuiltinId::TemporalZonedDateTimePrototypeGetTimeZoneTransition,
                StandardBuiltinId::TemporalZonedDateTimePrototypeHoursInDayGetter,
                StandardBuiltinId::TemporalZonedDateTimePrototypeStartOfDay,
                StandardBuiltinId::TemporalPlainDatePrototypeToZonedDateTime,
                StandardBuiltinId::TemporalZonedDateTimePrototypeWith,
                StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDate,
                StandardBuiltinId::IntlLocalePrototypeMaximize,
                StandardBuiltinId::IntlLocalePrototypeMinimize,
                StandardBuiltinId::IntlNumberFormatConstructor,
                StandardBuiltinId::IntlNumberFormatSupportedLocalesOf,
                StandardBuiltinId::IntlNumberFormatPrototypeFormatToParts,
                StandardBuiltinId::IntlNumberFormatPrototypeFormatRange,
                StandardBuiltinId::IntlNumberFormatPrototypeFormatRangeToParts,
                StandardBuiltinId::IntlNumberFormatBoundFormat,
                StandardBuiltinId::TemporalInstantPrototypeToLocaleString,
                StandardBuiltinId::TemporalZonedDateTimePrototypeToJson,
                StandardBuiltinId::TemporalZonedDateTimePrototypeToLocaleString,
                StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainTime,
                StandardBuiltinId::TemporalZonedDateTimePrototypeWithPlainTime,
                StandardBuiltinId::TemporalNowPlainDateTimeIso,
                StandardBuiltinId::TemporalNowPlainDateIso,
                StandardBuiltinId::TemporalNowPlainTimeIso,
                StandardBuiltinId::TemporalInstantPrototypeToZonedDateTimeIso,
                StandardBuiltinId::IntlPluralRulesConstructor,
                StandardBuiltinId::IntlPluralRulesSupportedLocalesOf,
                StandardBuiltinId::IntlPluralRulesPrototypeSelect,
                StandardBuiltinId::IntlPluralRulesPrototypeSelectRange,
                StandardBuiltinId::IntlListFormatConstructor,
                StandardBuiltinId::IntlListFormatSupportedLocalesOf,
                StandardBuiltinId::IntlListFormatPrototypeFormat,
                StandardBuiltinId::IntlListFormatPrototypeFormatToParts,
                StandardBuiltinId::IntlCollatorConstructor,
                StandardBuiltinId::IntlCollatorSupportedLocalesOf,
                StandardBuiltinId::IntlCollatorBoundCompare,
                StandardBuiltinId::IntlDisplayNamesConstructor,
                StandardBuiltinId::IntlDisplayNamesSupportedLocalesOf,
                StandardBuiltinId::IntlDisplayNamesPrototypeOf,
                StandardBuiltinId::IntlRelativeTimeFormatConstructor,
                StandardBuiltinId::IntlRelativeTimeFormatSupportedLocalesOf,
                StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormat,
                StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormatToParts,
                StandardBuiltinId::IntlSegmenterConstructor,
                StandardBuiltinId::IntlSegmenterSupportedLocalesOf,
                StandardBuiltinId::IntlSegmenterPrototypeSegment,
                StandardBuiltinId::IntlDurationFormatConstructor,
                StandardBuiltinId::IntlDurationFormatSupportedLocalesOf,
                StandardBuiltinId::IntlDurationFormatPrototypeFormat,
                StandardBuiltinId::IntlDurationFormatPrototypeFormatToParts,
                StandardBuiltinId::IntlLocalePrototypeGetWeekInfo,
                StandardBuiltinId::IntlLocalePrototypeGetTextInfo,
                StandardBuiltinId::IntlLocalePrototypeGetHourCycles,
                StandardBuiltinId::IntlLocalePrototypeGetNumberingSystems,
                StandardBuiltinId::IntlLocalePrototypeGetCalendars,
                StandardBuiltinId::IntlLocalePrototypeGetCollations,
                StandardBuiltinId::IntlLocalePrototypeGetTimeZones,
            ]
        );
    }

    #[test]
    fn collator_entries_keep_real_indices_installation_and_observable_callers() {
        for (ordinal, builtin, host) in [
            (860, StandardBuiltinId::IntlCollatorConstructor, true),
            (861, StandardBuiltinId::IntlCollatorSupportedLocalesOf, true),
            (
                862,
                StandardBuiltinId::IntlCollatorPrototypeResolvedOptions,
                false,
            ),
            (
                863,
                StandardBuiltinId::IntlCollatorPrototypeCompareGetter,
                false,
            ),
            (864, StandardBuiltinId::IntlCollatorBoundCompare, true),
        ] {
            assert_eq!(StandardBuiltinId::all_functions()[ordinal], builtin);
            assert_eq!(
                StandardBuiltinId::from_function_id(&builtin.function_id()),
                Some(builtin)
            );
            assert_eq!(builtin.requires_intl_host(), host);
        }
        assert_eq!(
            StandardBuiltinId::IntlCollatorConstructor.intrinsic_installer(),
            StandardBuiltinInstaller::IntlCollator
        );
        assert!(StandardBuiltinId::IntlCollatorConstructor.constructable());
        assert!(!StandardBuiltinId::IntlCollatorBoundCompare.constructable());
        assert_eq!(
            StandardBuiltinId::IntlCollatorBoundCompare.native_function_name(),
            Some("")
        );
        assert!(StandardBuiltinId::StringPrototypeLocaleCompare.requires_intl_host());
        assert!(StandardBuiltinId::StringPrototypeLocaleCompare.may_run_user_code_synchronously());
    }

    #[test]
    fn uint8_array_codecs_preserve_ordinals_and_observable_effects() {
        let members = crate::UINT8_ARRAY_CODEC_STATIC_MEMBERS
            .into_iter()
            .chain(crate::UINT8_ARRAY_CODEC_PROTOTYPE_MEMBERS);
        for (index, (name, builtin)) in members.enumerate() {
            assert_eq!(StandardBuiltinId::all_functions()[804 + index], builtin);
            assert_eq!(builtin.native_function_name(), Some(name));
            assert_eq!(
                StandardBuiltinId::from_function_id(&builtin.function_id()),
                Some(builtin)
            );
            assert!(!builtin.constructable());
            assert_eq!(builtin.is_static_method(), index < 2);
            assert_eq!(
                builtin.may_run_user_code_synchronously(),
                name.ends_with("Base64")
            );
            assert_eq!(builtin.mutates_indexed_receiver(), name.starts_with("set"));
        }
    }

    #[test]
    fn typed_array_intrinsic_has_one_hidden_constructable_identity() {
        let builtin = StandardBuiltinId::TypedArrayConstructor;
        let function_id = builtin.function_id();

        assert_eq!(function_id, BUILTIN_TYPED_ARRAY_CONSTRUCTOR_FUNCTION_ID);
        assert_eq!(
            StandardBuiltinId::from_function_id(&function_id),
            Some(builtin)
        );
        assert_eq!(builtin.global_name(), None);
        assert_eq!(builtin.native_function_name(), Some(TYPED_ARRAY_NAME));
        assert!(builtin.constructable());
        assert!(builtin.always_throws());
        assert!(StandardBuiltinId::all_functions().contains(&builtin));
        assert!(!StandardBuiltinId::all_globals().contains(&builtin));
    }

    #[test]
    fn disposable_stack_surface_has_one_closed_builtin_identity_per_algorithm() {
        let constructor = StandardBuiltinId::DisposableStackConstructor;
        for (builtin, native_name) in [
            (constructor, "DisposableStack"),
            (StandardBuiltinId::DisposableStackPrototypeUse, "use"),
            (StandardBuiltinId::DisposableStackPrototypeAdopt, "adopt"),
            (StandardBuiltinId::DisposableStackPrototypeDefer, "defer"),
            (StandardBuiltinId::DisposableStackPrototypeMove, "move"),
            (
                StandardBuiltinId::DisposableStackPrototypeDispose,
                "dispose",
            ),
            (
                StandardBuiltinId::DisposableStackPrototypeDisposedGetter,
                "get disposed",
            ),
        ] {
            assert_eq!(
                StandardBuiltinId::from_function_id(&builtin.function_id()),
                Some(builtin)
            );
            assert_eq!(builtin.native_function_name(), Some(native_name));
            assert!(StandardBuiltinId::all_functions().contains(&builtin));
            assert_eq!(builtin.constructable(), builtin == constructor);
        }
        assert!(StandardBuiltinId::all_globals().contains(&constructor));
    }

    #[test]
    fn math_random_alone_declares_the_host_random_import() {
        assert!(StandardBuiltinId::MathRandom.requires_random());
        assert_eq!(
            StandardBuiltinId::all_functions()
                .iter()
                .copied()
                .filter(|builtin| builtin.requires_random())
                .collect::<Vec<_>>(),
            vec![StandardBuiltinId::MathRandom]
        );
    }

    #[test]
    fn object_accessor_definers_have_call_only_catalog_identities() {
        for (builtin, name) in [
            (
                StandardBuiltinId::ObjectPrototypeDefineGetter,
                "__defineGetter__",
            ),
            (
                StandardBuiltinId::ObjectPrototypeDefineSetter,
                "__defineSetter__",
            ),
        ] {
            assert_eq!(
                StandardBuiltinId::from_function_id(&builtin.function_id()),
                Some(builtin)
            );
            assert_eq!(builtin.native_function_name(), Some(name));
            assert!(!builtin.constructable());
            assert!(!builtin.is_static_method());
            assert!(builtin.may_run_user_code_synchronously());
        }
    }

    #[test]
    fn object_and_reflect_observable_builtins_declare_synchronous_user_code() {
        for builtin in [
            StandardBuiltinId::ObjectConstructor,
            StandardBuiltinId::ObjectAssign,
            StandardBuiltinId::ObjectCreate,
            StandardBuiltinId::ObjectGetPrototypeOf,
            StandardBuiltinId::ObjectSetPrototypeOf,
            StandardBuiltinId::ObjectDefineProperty,
            StandardBuiltinId::ObjectDefineProperties,
            StandardBuiltinId::ObjectGetOwnPropertyDescriptor,
            StandardBuiltinId::ObjectGetOwnPropertyDescriptors,
            StandardBuiltinId::ObjectGetOwnPropertyNames,
            StandardBuiltinId::ObjectGetOwnPropertySymbols,
            StandardBuiltinId::ObjectKeys,
            StandardBuiltinId::ObjectValues,
            StandardBuiltinId::ObjectEntries,
            StandardBuiltinId::ObjectHasOwn,
            StandardBuiltinId::ObjectIsSealed,
            StandardBuiltinId::ObjectIsFrozen,
            StandardBuiltinId::ObjectSeal,
            StandardBuiltinId::ObjectFreeze,
            StandardBuiltinId::ObjectIsExtensible,
            StandardBuiltinId::ObjectPreventExtensions,
            StandardBuiltinId::ObjectPrototypeHasOwnProperty,
            StandardBuiltinId::ObjectPrototypeDefineGetter,
            StandardBuiltinId::ObjectPrototypeDefineSetter,
            StandardBuiltinId::ObjectPrototypeLookupGetter,
            StandardBuiltinId::ObjectPrototypeLookupSetter,
            StandardBuiltinId::ObjectPrototypeProtoGetter,
            StandardBuiltinId::ObjectPrototypeProtoSetter,
            StandardBuiltinId::ObjectPrototypePropertyIsEnumerable,
            StandardBuiltinId::ObjectPrototypeIsPrototypeOf,
            StandardBuiltinId::ObjectPrototypeToString,
            StandardBuiltinId::ObjectPrototypeToLocaleString,
            StandardBuiltinId::ReflectConstruct,
            StandardBuiltinId::ReflectApply,
            StandardBuiltinId::ReflectGet,
            StandardBuiltinId::ReflectGetPrototypeOf,
            StandardBuiltinId::ReflectGetOwnPropertyDescriptor,
            StandardBuiltinId::ReflectSet,
            StandardBuiltinId::ReflectHas,
            StandardBuiltinId::ReflectDefineProperty,
            StandardBuiltinId::ReflectDeleteProperty,
            StandardBuiltinId::ReflectIsExtensible,
            StandardBuiltinId::ReflectPreventExtensions,
            StandardBuiltinId::ReflectSetPrototypeOf,
            StandardBuiltinId::ReflectOwnKeys,
        ] {
            assert!(
                builtin.may_run_user_code_synchronously(),
                "{} must account for property hooks",
                builtin.debug_name()
            );
        }

        for builtin in [
            StandardBuiltinId::ObjectIs,
            StandardBuiltinId::ObjectPrototypeValueOf,
        ] {
            assert!(
                !builtin.may_run_user_code_synchronously(),
                "{} cannot invoke synchronous user code",
                builtin.debug_name()
            );
        }
    }

    #[test]
    fn promise_synchronous_user_code_catalog_is_a_closed_partition() {
        use StandardBuiltinId as Builtin;

        let synchronous_user_code = std::collections::BTreeSet::from([
            Builtin::PromiseConstructor,
            Builtin::PromisePrototypeThen,
            Builtin::PromisePrototypeCatch,
            Builtin::PromisePrototypeFinally,
            Builtin::PromiseThenFinally,
            Builtin::PromiseCatchFinally,
            Builtin::PromiseResolve,
            Builtin::PromiseWithResolvers,
            Builtin::PromiseTry,
            Builtin::PromiseReject,
            Builtin::PromiseAll,
            Builtin::PromiseAllSettled,
            Builtin::PromiseAllKeyed,
            Builtin::PromiseAllSettledKeyed,
            Builtin::PromiseAny,
            Builtin::PromiseRace,
            Builtin::PromiseAllResolveElement,
            Builtin::PromiseAllSettledResolveElement,
            Builtin::PromiseAllSettledRejectElement,
            Builtin::PromiseAnyRejectElement,
            Builtin::PromiseAllKeyedResolveElement,
            Builtin::PromiseAllSettledKeyedResolveElement,
            Builtin::PromiseAllSettledKeyedRejectElement,
            Builtin::PromiseResolveFunction,
        ]);
        let synchronous_pure = std::collections::BTreeSet::from([
            Builtin::PromiseValueThunk,
            Builtin::PromiseThrower,
            Builtin::PromiseSpeciesGetter,
            Builtin::PromiseCapabilityExecutor,
            Builtin::PromiseRejectFunction,
        ]);
        assert!(synchronous_user_code.is_disjoint(&synchronous_pure));

        let classified = synchronous_user_code
            .union(&synchronous_pure)
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let declared = StandardBuiltinId::all_functions()
            .iter()
            .copied()
            .filter(|builtin| {
                builtin
                    .function_id()
                    .starts_with(BUILTIN_PROMISE_FUNCTION_ID)
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            declared, classified,
            "every Promise builtin needs a caller-flow classification"
        );

        for builtin in synchronous_user_code {
            assert!(
                builtin.may_run_user_code_synchronously(),
                "{} can synchronously invoke user code",
                builtin.debug_name()
            );
        }
        for builtin in synchronous_pure {
            assert!(
                !builtin.may_run_user_code_synchronously(),
                "{} is synchronously pure",
                builtin.debug_name()
            );
        }
    }

    #[test]
    fn intrinsic_installer_roots_are_explicit_and_stable() {
        use StandardBuiltinId as Builtin;
        use StandardBuiltinInstaller as Installer;

        let roots = StandardBuiltinId::all_functions()
            .iter()
            .copied()
            .map(|builtin| (builtin, builtin.intrinsic_installer()))
            .filter(|(_, installer)| *installer != Installer::None)
            .collect::<Vec<_>>();

        assert_eq!(
            roots.as_slice(),
            &[
                (Builtin::FunctionConstructor, Installer::Function),
                (Builtin::ObjectConstructor, Installer::Object),
                (Builtin::ProxyConstructor, Installer::Proxy),
                (Builtin::ArrayConstructor, Installer::Array),
                (Builtin::IteratorConstructor, Installer::Iterator),
                (Builtin::ArrayBufferConstructor, Installer::ArrayBuffer),
                (
                    Builtin::SharedArrayBufferConstructor,
                    Installer::ArrayBuffer,
                ),
                (Builtin::DataViewConstructor, Installer::DataView),
                (Builtin::DateConstructor, Installer::Date),
                (
                    Builtin::TemporalPlainDateConstructor,
                    Installer::TemporalPlainDate,
                ),
                (
                    Builtin::TemporalPlainYearMonthConstructor,
                    Installer::TemporalPlainYearMonth,
                ),
                (
                    Builtin::TemporalPlainMonthDayConstructor,
                    Installer::TemporalPlainMonthDay,
                ),
                (
                    Builtin::TemporalPlainTimeConstructor,
                    Installer::TemporalPlainTime,
                ),
                (
                    Builtin::TemporalPlainDateTimeConstructor,
                    Installer::TemporalPlainDateTime,
                ),
                (
                    Builtin::TemporalDurationConstructor,
                    Installer::TemporalDuration,
                ),
                (
                    Builtin::TemporalInstantConstructor,
                    Installer::TemporalInstant,
                ),
                (
                    Builtin::TemporalZonedDateTimeConstructor,
                    Installer::TemporalZonedDateTime,
                ),
                (Builtin::IntlLocaleConstructor, Installer::IntlLocale),
                (
                    Builtin::IntlDateTimeFormatConstructor,
                    Installer::IntlDateTimeFormat,
                ),
                (Builtin::RegExpConstructor, Installer::RegExp),
                (Builtin::Uint8ArrayConstructor, Installer::Uint8Array),
                (Builtin::BigIntConstructor, Installer::BigInt),
                (Builtin::NumberConstructor, Installer::Number),
                (Builtin::StringConstructor, Installer::String),
                (Builtin::BooleanConstructor, Installer::Boolean),
                (Builtin::PromiseConstructor, Installer::Promise),
                (Builtin::MapConstructor, Installer::Map),
                (Builtin::WeakMapConstructor, Installer::WeakMap),
                (Builtin::WeakSetConstructor, Installer::WeakSet),
                (Builtin::WeakRefConstructor, Installer::WeakRef),
                (
                    Builtin::FinalizationRegistryConstructor,
                    Installer::FinalizationRegistry,
                ),
                (
                    Builtin::AsyncDisposableStackConstructor,
                    Installer::AsyncDisposableStack,
                ),
                (Builtin::SetConstructor, Installer::Set),
                (Builtin::SymbolConstructor, Installer::Symbol),
                (Builtin::ErrorConstructor, Installer::Error),
                (
                    Builtin::DisposableStackConstructor,
                    Installer::DisposableStack,
                ),
                (
                    Builtin::IntlNumberFormatConstructor,
                    Installer::IntlNumberFormat,
                ),
                (
                    Builtin::IntlPluralRulesConstructor,
                    Installer::IntlPluralRules
                ),
                (
                    Builtin::IntlListFormatConstructor,
                    Installer::IntlListFormat
                ),
                (Builtin::IntlCollatorConstructor, Installer::IntlCollator),
                (
                    Builtin::IntlDisplayNamesConstructor,
                    Installer::IntlDisplayNames
                ),
                (
                    Builtin::IntlRelativeTimeFormatConstructor,
                    Installer::IntlRelativeTimeFormat
                ),
                (Builtin::IntlSegmenterConstructor, Installer::IntlSegmenter),
                (
                    Builtin::IntlDurationFormatConstructor,
                    Installer::IntlDurationFormat
                ),
                (Builtin::ShadowRealmConstructor, Installer::ShadowRealm),
                (Builtin::TextEncoderConstructor, Installer::TextEncoder),
                (Builtin::TextDecoderConstructor, Installer::TextDecoder),
            ]
        );
    }
}

// Input ToString can reenter JavaScript; getter reads only the captured state.
const _: () = {
    assert!(StandardBuiltinId::RegExpLegacyStaticSetter.may_run_user_code_synchronously());
    assert!(!StandardBuiltinId::RegExpLegacyStaticGetter.may_run_user_code_synchronously());
};
