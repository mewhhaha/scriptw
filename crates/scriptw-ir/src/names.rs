use crate::native_error::NativeErrorKind;
use crate::StandardBuiltinId;

pub const SHADOW_REALM_NAME: &str = "ShadowRealm";
pub const SHADOW_REALM_PROTOTYPE_METHODS: &[(&str, StandardBuiltinId)] = &[
    ("evaluate", StandardBuiltinId::ShadowRealmPrototypeEvaluate),
    (
        "importValue",
        StandardBuiltinId::ShadowRealmPrototypeImportValue,
    ),
];

pub(crate) const SCRIPT_OWNER_ID: &str = "$script";
pub(crate) const MAX_STATIC_ARRAY_SHAPE_INDEX: usize = 1_000_000;
pub(crate) const MAX_ARRAY_INDEX: f64 = 4_294_967_294.0;
pub use scriptw_front::{
    decode_js_string_utf16, encode_js_string_utf16, JS_STRING_SURROGATE_SENTINEL,
};

/// Source-unspellable capture of a direct eval caller execution context.
pub const DIRECT_EVAL_EXECUTION_CONTEXT_NAME: &str = "\0direct.eval.context";
/// Compiler-private binding storage names contain `.`, which no
/// IdentifierName can contain (ECMA-262 12.7), so no source declaration,
/// reference or assignment, in any owner or through any capture, can reach
/// one. The fixed names below follow that rule; every temporary is minted by
/// [`PrivateBindingName::temporary`].
pub const PRIVATE_BINDING_NAME_MARK: char = '.';

/// A minted compiler-private temporary binding name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrivateBindingName(String);

impl PrivateBindingName {
    /// `$<hint>.<index>`: `hint` names the temporary's role (a trailing `.`
    /// is folded into the separator) and `index` makes it unique.
    pub(crate) fn temporary(hint: &str, index: usize) -> Self {
        let hint = hint.trim_end_matches(PRIVATE_BINDING_NAME_MARK);
        assert!(
            !hint.is_empty() && !hint.contains(char::is_whitespace),
            "a private binding hint is a nonempty role name"
        );
        Self(format!("${hint}{PRIVATE_BINDING_NAME_MARK}{index}"))
    }

    pub(crate) fn into_storage_name(self) -> String {
        self.0
    }
}

/// The value parameter of a synthesized auto-accessor setter.
pub(crate) const AUTO_ACCESSOR_VALUE_NAME: &str = "$auto.accessor.value";
/// Lexical `this`, `arguments` and `[[HomeObject]]` cells an arrow captures.
pub const LEXICAL_THIS_NAME: &str = "$.this";
pub const LEXICAL_ARGUMENTS_NAME: &str = "$.arguments";
pub const LEXICAL_NEW_TARGET_NAME: &str = "$new.target";
/// Compiler-private lexical `[[HomeObject]]` binding for arrows using `super`.
pub const LEXICAL_HOME_OBJECT_NAME: &str = "$.homeObject";
/// Compiler-private environment bindings for lexical `super()` in arrows.
pub const DERIVED_ACTIVATION_THIS_NAME: &str = "$derived.this";
pub const DERIVED_ACTIVATION_THIS_STATUS_NAME: &str = "$derived.thisStatus";
pub const DERIVED_ACTIVATION_NEW_TARGET_NAME: &str = "$derived.newTarget";
pub const DERIVED_ACTIVATION_FUNCTION_NAME: &str = "$derived.activeFunction";
/// The reserved storage *spelling* for a placeholder scope entry — a
/// for-in/for-of head binding (14.7.5.5) and a formal parameter before its
/// initialization (10.2.11 step 21).
///
/// This is a name domain, not a lifecycle state. The state lives on
/// `BindingInfo::initialization` (`binding_lifecycle::Initialization`), and no
/// read or write site may decide whether to throw by testing for this prefix.
/// Mint and test it only through `binding_lifecycle::TdzPlaceholderName`.
pub(crate) const TDZ_BINDING_STORAGE_PREFIX: &str = "$tdz.";

/// `[[ExportName]]` shared by every `export default` form (16.2.3.7).
pub const MODULE_DEFAULT_EXPORT_NAME: &str = "default";
/// Spec `[[LocalName]]` of an anonymous `export default` declaration.
///
/// 8.2.2 gives `export default function () {}` the bound name `*default*`,
/// which no `BindingIdentifier` can spell — so it can never collide with a
/// name from source.
pub const MODULE_ANONYMOUS_DEFAULT_LOCAL_NAME: &str = "*default*";

// The eleven module name-minting functions that used to live here are gone.
//
// Five of them had no product call site at all (`module_function_id`,
// `module_function_id_prefix`, `is_user_function_id`,
// `module_import_meta_cell_name`, and `module_component_completion_cell_name`,
// whose one caller wrote a field nothing read); two of those five could not
// have worked, because their names contained a `.` and so were not
// `IdentifierReference`s. The remaining six returned a bare `String` from a
// bare `u32` with no relation to each other and no relation to the byte budgets
// their spellings had to respect.
//
// They are replaced by `crate::binding_names`, which spells the same names
// through `LocalName::merged_in` and `MergedName::minted(unit, UnitCellRole)` —
// one `format!` for the whole crate, a closed role set, and const assertions
// tying the two length budgets to the constants `modules::source` and
// `modules::record` actually match on. See
// `docs/rust-rewrite/contracts/Module binding-name domains: [[LocalName]] vs
// [[ExportName]] vs merged storage name.md`.
//
// No `FunctionId` is module-qualified anywhere, which is why the three
// `FunctionId` helpers had no callers: `modules::link` merges unit bodies on
// source text before lowering, so every span-derived `FunctionId` is unique by
// construction and needs no per-unit prefix.

pub const GLOBAL_THIS_NAME: &str = "globalThis";
pub const MATH_NAME: &str = "Math";
pub const PRINT_NAME: &str = "print";
pub const GC_NAME: &str = "gc";
pub const ASSERT_THROWS_NAME: &str = "__scriptwAssertThrows";
pub const IS_CONSTRUCTOR_NAME: &str = "__scriptwIsConstructor";
pub const CREATE_REALM_NAME: &str = "__scriptwCreateRealm";
pub const CREATE_HTMLDDA_NAME: &str = "__scriptwCreateHTMLDDA";
pub const PARSE_INT_NAME: &str = "parseInt";
pub const PARSE_FLOAT_NAME: &str = "parseFloat";
pub const ESCAPE_NAME: &str = "escape";
pub const UNESCAPE_NAME: &str = "unescape";
pub const ENCODE_URI_NAME: &str = "encodeURI";
pub const ENCODE_URI_COMPONENT_NAME: &str = "encodeURIComponent";
pub const DECODE_URI_NAME: &str = "decodeURI";
pub const DECODE_URI_COMPONENT_NAME: &str = "decodeURIComponent";
pub const HOST_PRINT_FUNCTION_ID: &str = "$host.print";
pub const HOST_GC_FUNCTION_ID: &str = "$host.gc";
pub const HOST_ASSERT_THROWS_FUNCTION_ID: &str = "$host.assertThrows";
pub const HOST_IS_CONSTRUCTOR_FUNCTION_ID: &str = "$host.isConstructor";
pub const HOST_CREATE_REALM_FUNCTION_ID: &str = "$host.createRealm";
pub const HOST_CREATE_HTMLDDA_FUNCTION_ID: &str = "$host.createHTMLDDA";
pub const HOST_HTMLDDA_FUNCTION_ID: &str = "$host.htmlDDA";
pub const HOST_PARSE_INT_FUNCTION_ID: &str = "$host.parseInt";
pub const HOST_PARSE_FLOAT_FUNCTION_ID: &str = "$host.parseFloat";
pub const DETACH_ARRAY_BUFFER_NAME: &str = "__scriptwDetachArrayBuffer";
pub const HOST_DETACH_ARRAY_BUFFER_FUNCTION_ID: &str = "$host.detachArrayBuffer";
pub const AGENT_START_NAME: &str = "__scriptwAgentStart";
pub const AGENT_BROADCAST_NAME: &str = "__scriptwAgentBroadcast";
pub const AGENT_RECEIVE_BROADCAST_NAME: &str = "__scriptwAgentReceiveBroadcast";
pub const AGENT_REPORT_NAME: &str = "__scriptwAgentReport";
pub const AGENT_GET_REPORT_NAME: &str = "__scriptwAgentGetReport";
pub const AGENT_SLEEP_NAME: &str = "__scriptwAgentSleep";
pub const AGENT_MONOTONIC_NOW_NAME: &str = "__scriptwAgentMonotonicNow";
pub const AGENT_LEAVING_NAME: &str = "__scriptwAgentLeaving";
pub const HOST_AGENT_START_FUNCTION_ID: &str = "$host.agentStart";
pub const HOST_AGENT_BROADCAST_FUNCTION_ID: &str = "$host.agentBroadcast";
pub const HOST_AGENT_RECEIVE_BROADCAST_FUNCTION_ID: &str = "$host.agentReceiveBroadcast";
pub const HOST_AGENT_REPORT_FUNCTION_ID: &str = "$host.agentReport";
pub const HOST_AGENT_GET_REPORT_FUNCTION_ID: &str = "$host.agentGetReport";
pub const HOST_AGENT_SLEEP_FUNCTION_ID: &str = "$host.agentSleep";
pub const HOST_AGENT_MONOTONIC_NOW_FUNCTION_ID: &str = "$host.agentMonotonicNow";
pub const HOST_AGENT_LEAVING_FUNCTION_ID: &str = "$host.agentLeaving";
pub const DYNAMIC_GENERATOR_FUNCTION_CONSTRUCTOR_FUNCTION_ID: &str = "$dynamic.GeneratorFunction";
pub const DYNAMIC_ASYNC_FUNCTION_CONSTRUCTOR_FUNCTION_ID: &str = "$dynamic.AsyncFunction";
pub const DYNAMIC_ASYNC_GENERATOR_FUNCTION_CONSTRUCTOR_FUNCTION_ID: &str =
    "$dynamic.AsyncGeneratorFunction";
pub const DYNAMIC_REALM_EVAL_SCRIPT_FUNCTION_ID: &str = "$host.realmEvalScript";
pub const REALM_EVAL_SCRIPT_NAME: &str = "__scriptwRealmEvalScript";
pub const REALM_EVAL_SCRIPT_METHOD_NAME: &str = "evalScript";
pub const FUNCTION_NAME: &str = "Function";
pub const OBJECT_NAME: &str = "Object";
pub const ARRAY_NAME: &str = "Array";
pub const ARRAY_BUFFER_NAME: &str = "ArrayBuffer";
pub const SHARED_ARRAY_BUFFER_NAME: &str = "SharedArrayBuffer";
pub const DATA_VIEW_NAME: &str = "DataView";
pub const TYPED_ARRAY_NAME: &str = "TypedArray";
pub const DATE_NAME: &str = "Date";
pub const TEMPORAL_NAME: &str = "Temporal";
pub const TEMPORAL_NOW_NAME: &str = "Now";
pub const TEMPORAL_INSTANT_NAME: &str = "Instant";
pub const TEMPORAL_PLAIN_DATE_NAME: &str = "PlainDate";
pub const TEMPORAL_PLAIN_TIME_NAME: &str = "PlainTime";
pub const TEMPORAL_PLAIN_DATE_TIME_NAME: &str = "PlainDateTime";
pub const TEMPORAL_PLAIN_YEAR_MONTH_NAME: &str = "PlainYearMonth";
pub const TEMPORAL_PLAIN_MONTH_DAY_NAME: &str = "PlainMonthDay";
pub const TEMPORAL_ZONED_DATE_TIME_NAME: &str = "ZonedDateTime";
pub const TEMPORAL_DURATION_NAME: &str = "Duration";
pub const INTL_NAME: &str = "Intl";
pub const INTL_LOCALE_NAME: &str = "Locale";
pub const INTL_DATE_TIME_FORMAT_NAME: &str = "DateTimeFormat";
pub const INTL_NUMBER_FORMAT_NAME: &str = "NumberFormat";
pub const INTL_PLURAL_RULES_NAME: &str = "PluralRules";
pub const INTL_LIST_FORMAT_NAME: &str = "ListFormat";
pub const INTL_COLLATOR_NAME: &str = "Collator";
pub const INTL_DISPLAY_NAMES_NAME: &str = "DisplayNames";
pub const INTL_RELATIVE_TIME_FORMAT_NAME: &str = "RelativeTimeFormat";
pub const INTL_SEGMENTER_NAME: &str = "Segmenter";
pub const INTL_DURATION_FORMAT_NAME: &str = "DurationFormat";

/// Ordinary methods on the Intl namespace, in entry/created-Realm property order.
/// The IR shape and both installers consume this exact list.
pub const INTL_NAMESPACE_METHODS: &[(&str, StandardBuiltinId)] = &[
    (
        "getCanonicalLocales",
        StandardBuiltinId::IntlGetCanonicalLocales,
    ),
    (
        "supportedValuesOf",
        StandardBuiltinId::IntlSupportedValuesOf,
    ),
];

/// The `Intl` namespace object's constructor-valued members, in **installation
/// order** — `Object.getOwnPropertyNames(Intl)` reports this order, so it is
/// observable and both the IR shape and the emitter must walk it.
///
/// This slice is the single declaration of constructor members on `Intl`. Before it
/// existed, `ScriptLowerer::intl_object_value_info` and
/// `FunctionBuilder::init_intl_object` were two hand-maintained lists of the
/// same set, and they had already drifted: `DateTimeFormat` was in the shape and
/// not in the installer, so constant-folded `new Intl.DateTimeFormat()` worked
/// while `Object.getOwnPropertyDescriptor(Intl, "DateTimeFormat")` saw nothing.
/// That is `intl402/DateTimeFormat/prop-desc.js`.
///
/// Namespace methods have their separate shared list above; Symbol.toStringTag
/// is installed as a primitive data property.

pub const INTL_NAMESPACE_CONSTRUCTORS: &[(&str, StandardBuiltinId)] = &[
    (
        INTL_DATE_TIME_FORMAT_NAME,
        StandardBuiltinId::IntlDateTimeFormatConstructor,
    ),
    (INTL_LOCALE_NAME, StandardBuiltinId::IntlLocaleConstructor),
    (
        INTL_NUMBER_FORMAT_NAME,
        StandardBuiltinId::IntlNumberFormatConstructor,
    ),
    (
        INTL_PLURAL_RULES_NAME,
        StandardBuiltinId::IntlPluralRulesConstructor,
    ),
    (
        INTL_LIST_FORMAT_NAME,
        StandardBuiltinId::IntlListFormatConstructor,
    ),
    (
        INTL_COLLATOR_NAME,
        StandardBuiltinId::IntlCollatorConstructor,
    ),
    (
        INTL_DISPLAY_NAMES_NAME,
        StandardBuiltinId::IntlDisplayNamesConstructor,
    ),
    (
        INTL_RELATIVE_TIME_FORMAT_NAME,
        StandardBuiltinId::IntlRelativeTimeFormatConstructor,
    ),
    (
        INTL_SEGMENTER_NAME,
        StandardBuiltinId::IntlSegmenterConstructor,
    ),
    (
        INTL_DURATION_FORMAT_NAME,
        StandardBuiltinId::IntlDurationFormatConstructor,
    ),
];

/// The constructor-valued members of the `Temporal` namespace, in emitted
/// property order. The IR namespace shape and Wasm bootstrap both walk this
/// slice, so neither can advertise a constructor the other omits.
pub const TEMPORAL_NAMESPACE_CONSTRUCTORS: &[(&str, StandardBuiltinId)] = &[
    (
        TEMPORAL_INSTANT_NAME,
        StandardBuiltinId::TemporalInstantConstructor,
    ),
    (
        TEMPORAL_PLAIN_DATE_NAME,
        StandardBuiltinId::TemporalPlainDateConstructor,
    ),
    (
        TEMPORAL_ZONED_DATE_TIME_NAME,
        StandardBuiltinId::TemporalZonedDateTimeConstructor,
    ),
    (
        TEMPORAL_PLAIN_TIME_NAME,
        StandardBuiltinId::TemporalPlainTimeConstructor,
    ),
    (
        TEMPORAL_PLAIN_DATE_TIME_NAME,
        StandardBuiltinId::TemporalPlainDateTimeConstructor,
    ),
    (
        TEMPORAL_PLAIN_YEAR_MONTH_NAME,
        StandardBuiltinId::TemporalPlainYearMonthConstructor,
    ),
    (
        TEMPORAL_PLAIN_MONTH_DAY_NAME,
        StandardBuiltinId::TemporalPlainMonthDayConstructor,
    ),
    (
        TEMPORAL_DURATION_NAME,
        StandardBuiltinId::TemporalDurationConstructor,
    ),
];

/// The function-valued members of `Temporal.Now`, in emitted property order.
/// These are separate from [`TEMPORAL_NAMESPACE_CONSTRUCTORS`] because the
/// namespace members are non-constructable functions with no constructor
/// global to load.
pub const TEMPORAL_NOW_NAMESPACE_MEMBERS: &[(&str, StandardBuiltinId)] = &[
    ("timeZoneId", StandardBuiltinId::TemporalNowTimeZoneId),
    ("instant", StandardBuiltinId::TemporalNowInstant),
    (
        "zonedDateTimeISO",
        StandardBuiltinId::TemporalNowZonedDateTimeIso,
    ),
    (
        "plainDateTimeISO",
        StandardBuiltinId::TemporalNowPlainDateTimeIso,
    ),
    ("plainDateISO", StandardBuiltinId::TemporalNowPlainDateIso),
    ("plainTimeISO", StandardBuiltinId::TemporalNowPlainTimeIso),
];

/// The `Temporal.ZonedDateTime.prototype` DATA-PROPERTY METHODS, as ONE table.
///
/// Same shape and same reason as [`INTL_NAMESPACE_CONSTRUCTORS`] above: two
/// consumers in two crates must agree on this membership exactly.
///
/// * `lowering::temporal_zoned_date_time_prototype_shape` — what the IR
///   believes the prototype carries, which is what a static-key `CallMethod`
///   guard reads;
/// * `scriptw-aot-wasm`'s
///   `install_temporal_zoned_date_time_constructor_intrinsics` — what the
///   emitted prototype object actually carries.
///
/// A name in one list and not the other is a shape that promises a method the
/// prototype does not have. Nothing fails to build; it reads back at the call
/// site as `TypeError: value is not callable` — the exact label all 28
/// `era-boundary-*.js` cases carried before these five members existed. Batch 6
/// first shipped the list twice, once per crate, each copy preceded by a prose
/// comment telling the reader they must match. One `const` iterated from both
/// sites makes the disagreement unrepresentable instead of commented against
/// (AGENTS.md, "Code Invariants Before Test Invariants").
///
/// **This covers the whole data-property membership, not only the five members
/// batch 6 added.** It first shipped holding those five while `equals`,
/// `toInstant`, `withTimeZone` and `toPlainDateTime` stayed written out twice —
/// once as `properties.insert` literals in the shape, once as
/// `emit_object_define_function_data` calls in the installer — so for four of
/// the nine members the guarantee this doc claims was still just a comment, and
/// a tenth member added to one block and not the other would still compile and
/// still produce the `TypeError: value is not callable` the table exists to make
/// impossible. The four are folded in, in install order, ahead of the five.
///
/// The accessors (`era`, `hoursInDay`, the calendar/time fields, …) are NOT
/// here. They are installed with
/// `emit_object_append_accessor_property_with_flags`, not
/// `emit_object_define_function_data`, and the shape records them as
/// `ObjectShapeProperty::Accessor`; one table cannot describe both without
/// carrying a kind column that has exactly one non-default row. If accessors are
/// ever folded in too, that is the shape to give this.
///
/// **Order is observable.** The install loop appends in this order and
/// `Object.keys` on the prototype shows it, so the order is part of the
/// contract. Do not sort this list, and append rather than interleave. The
/// leading four are in the order the installer already used (`equals` before
/// `withTimeZone`, which is not spec order); preserving that is what makes the
/// fold byte-neutral.
pub const TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_METHODS: &[(&str, StandardBuiltinId)] = &[
    (
        "with",
        StandardBuiltinId::TemporalZonedDateTimePrototypeWith,
    ),
    (
        "toPlainDate",
        StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDate,
    ),
    (
        "equals",
        StandardBuiltinId::TemporalZonedDateTimePrototypeEquals,
    ),
    (
        "toInstant",
        StandardBuiltinId::TemporalZonedDateTimePrototypeToInstant,
    ),
    (
        "withTimeZone",
        StandardBuiltinId::TemporalZonedDateTimePrototypeWithTimeZone,
    ),
    (
        "toPlainDateTime",
        StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDateTime,
    ),
    (
        "withCalendar",
        StandardBuiltinId::TemporalZonedDateTimePrototypeWithCalendar,
    ),
    ("add", StandardBuiltinId::TemporalZonedDateTimePrototypeAdd),
    (
        "subtract",
        StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract,
    ),
    (
        "until",
        StandardBuiltinId::TemporalZonedDateTimePrototypeUntil,
    ),
    (
        "since",
        StandardBuiltinId::TemporalZonedDateTimePrototypeSince,
    ),
    (
        "toString",
        StandardBuiltinId::TemporalZonedDateTimePrototypeToString,
    ),
    (
        "round",
        StandardBuiltinId::TemporalZonedDateTimePrototypeRound,
    ),
    (
        "getTimeZoneTransition",
        StandardBuiltinId::TemporalZonedDateTimePrototypeGetTimeZoneTransition,
    ),
    (
        "startOfDay",
        StandardBuiltinId::TemporalZonedDateTimePrototypeStartOfDay,
    ),
    (
        "toPlainTime",
        StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainTime,
    ),
    (
        "toLocaleString",
        StandardBuiltinId::TemporalZonedDateTimePrototypeToLocaleString,
    ),
    (
        "toJSON",
        StandardBuiltinId::TemporalZonedDateTimePrototypeToJson,
    ),
    (
        "valueOf",
        StandardBuiltinId::TemporalZonedDateTimePrototypeValueOf,
    ),
    (
        "withPlainTime",
        StandardBuiltinId::TemporalZonedDateTimePrototypeWithPlainTime,
    ),
];

pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_TO_PLAIN_DATE_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.toPlainDate";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_WITH_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.with";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_ROUND_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.round";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_GET_TIME_ZONE_TRANSITION_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.getTimeZoneTransition";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_HOURS_IN_DAY_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.hoursInDay.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_START_OF_DAY_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.startOfDay";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_ZONED_DATE_TIME_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.toZonedDateTime";

pub const REGEXP_NAME: &str = "RegExp";
pub const JSON_NAME: &str = "JSON";
pub const ATOMICS_NAME: &str = "Atomics";
pub const FLOAT64_ARRAY_NAME: &str = "Float64Array";
pub const FLOAT32_ARRAY_NAME: &str = "Float32Array";
pub const FLOAT16_ARRAY_NAME: &str = "Float16Array";
pub const INT32_ARRAY_NAME: &str = "Int32Array";
pub const INT16_ARRAY_NAME: &str = "Int16Array";
pub const INT8_ARRAY_NAME: &str = "Int8Array";
pub const UINT32_ARRAY_NAME: &str = "Uint32Array";
pub const UINT16_ARRAY_NAME: &str = "Uint16Array";
pub const UINT8_ARRAY_NAME: &str = "Uint8Array";
pub const UINT8_ARRAY_CODEC_STATIC_MEMBERS: [(&str, StandardBuiltinId); 2] = [
    ("fromBase64", StandardBuiltinId::Uint8ArrayFromBase64),
    ("fromHex", StandardBuiltinId::Uint8ArrayFromHex),
];
pub const UINT8_ARRAY_CODEC_PROTOTYPE_MEMBERS: [(&str, StandardBuiltinId); 4] = [
    (
        "setFromBase64",
        StandardBuiltinId::Uint8ArrayPrototypeSetFromBase64,
    ),
    (
        "setFromHex",
        StandardBuiltinId::Uint8ArrayPrototypeSetFromHex,
    ),
    ("toBase64", StandardBuiltinId::Uint8ArrayPrototypeToBase64),
    ("toHex", StandardBuiltinId::Uint8ArrayPrototypeToHex),
];
pub const UINT8_CLAMPED_ARRAY_NAME: &str = "Uint8ClampedArray";
pub const BIGINT64_ARRAY_NAME: &str = "BigInt64Array";
pub const BIGUINT64_ARRAY_NAME: &str = "BigUint64Array";
pub const BIGINT_NAME: &str = "BigInt";
pub const PROXY_NAME: &str = "Proxy";
pub const REFLECT_NAME: &str = "Reflect";
pub const NUMBER_NAME: &str = "Number";
pub const STRING_NAME: &str = "String";
pub const BOOLEAN_NAME: &str = "Boolean";
pub const SYMBOL_NAME: &str = "Symbol";
pub const PROMISE_NAME: &str = "Promise";
pub const MAP_NAME: &str = "Map";
pub const WEAK_MAP_NAME: &str = "WeakMap";
pub const WEAK_SET_NAME: &str = "WeakSet";
pub const WEAK_REF_NAME: &str = "WeakRef";
pub const FINALIZATION_REGISTRY_NAME: &str = "FinalizationRegistry";
pub const ASYNC_DISPOSABLE_STACK_NAME: &str = "AsyncDisposableStack";
pub const DISPOSABLE_STACK_NAME: &str = "DisposableStack";
pub const SET_NAME: &str = "Set";
// The nine error intrinsic names are a closed domain owned by
// `crate::native_error::NativeErrorKind`, which is the single spelling
// authority (contract invariant E2). These consts are *defined from* it rather
// than repeating its literals, so there is exactly one spelling of each name in
// the crate and no second list to drift.
//
// They survive as `&'static str` because `crates/scriptw-aot-wasm` still keys
// its four error-prototype tables on the name, and one of the files holding
// those uses is outside this lane. They are structural-match `&str` consts, so
// they remain legal in the pattern positions those tables use. New code should
// take a `NativeErrorKind` instead; see
// `docs/rust-rewrite/contracts/closed-name-domains.md`, ledger entry R4.
pub const ERROR_NAME: &str = NativeErrorKind::Error.as_str();
pub const EVAL_ERROR_NAME: &str = NativeErrorKind::EvalError.as_str();
pub const AGGREGATE_ERROR_NAME: &str = NativeErrorKind::AggregateError.as_str();
pub const SUPPRESSED_ERROR_NAME: &str = NativeErrorKind::SuppressedError.as_str();
pub const RANGE_ERROR_NAME: &str = NativeErrorKind::RangeError.as_str();
pub const SYNTAX_ERROR_NAME: &str = NativeErrorKind::SyntaxError.as_str();
pub const TYPE_ERROR_NAME: &str = NativeErrorKind::TypeError.as_str();
pub const URI_ERROR_NAME: &str = NativeErrorKind::URIError.as_str();
pub const REFERENCE_ERROR_NAME: &str = NativeErrorKind::ReferenceError.as_str();
pub const BUILTIN_FUNCTION_FUNCTION_ID: &str = "$builtin.Function";
pub const BUILTIN_FUNCTION_PROTOTYPE_FUNCTION_ID: &str = "$builtin.Function.prototype";
pub const BUILTIN_FUNCTION_PROTOTYPE_CALL_FUNCTION_ID: &str = "$builtin.Function.prototype.call";
pub const BUILTIN_FUNCTION_PROTOTYPE_APPLY_FUNCTION_ID: &str = "$builtin.Function.prototype.apply";
pub const BUILTIN_FUNCTION_PROTOTYPE_BIND_FUNCTION_ID: &str = "$builtin.Function.prototype.bind";
pub const BUILTIN_FUNCTION_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.Function.prototype.toString";
pub const BUILTIN_FUNCTION_PROTOTYPE_SYMBOL_HAS_INSTANCE_FUNCTION_ID: &str =
    "$builtin.Function.prototype[Symbol.hasInstance]";
pub const BUILTIN_EVAL_FUNCTION_ID: &str = "$builtin.eval";
pub const BUILTIN_OBJECT_FUNCTION_ID: &str = "$builtin.Object";
pub const BUILTIN_OBJECT_GROUP_BY_FUNCTION_ID: &str = "$builtin.Object.groupBy";
pub const BUILTIN_OBJECT_FROM_ENTRIES_FUNCTION_ID: &str = "$builtin.Object.fromEntries";
pub const BUILTIN_OBJECT_ASSIGN_FUNCTION_ID: &str = "$builtin.Object.assign";
pub const BUILTIN_OBJECT_CREATE_FUNCTION_ID: &str = "$builtin.Object.create";
pub const BUILTIN_OBJECT_GET_PROTOTYPE_OF_FUNCTION_ID: &str = "$builtin.Object.getPrototypeOf";
pub const BUILTIN_OBJECT_SET_PROTOTYPE_OF_FUNCTION_ID: &str = "$builtin.Object.setPrototypeOf";
pub const BUILTIN_OBJECT_DEFINE_PROPERTY_FUNCTION_ID: &str = "$builtin.Object.defineProperty";
pub const BUILTIN_OBJECT_DEFINE_PROPERTIES_FUNCTION_ID: &str = "$builtin.Object.defineProperties";
pub const BUILTIN_OBJECT_GET_OWN_PROPERTY_DESCRIPTOR_FUNCTION_ID: &str =
    "$builtin.Object.getOwnPropertyDescriptor";
pub const BUILTIN_OBJECT_GET_OWN_PROPERTY_DESCRIPTORS_FUNCTION_ID: &str =
    "$builtin.Object.getOwnPropertyDescriptors";
pub const BUILTIN_OBJECT_GET_OWN_PROPERTY_NAMES_FUNCTION_ID: &str =
    "$builtin.Object.getOwnPropertyNames";
pub const BUILTIN_OBJECT_GET_OWN_PROPERTY_SYMBOLS_FUNCTION_ID: &str =
    "$builtin.Object.getOwnPropertySymbols";
pub const BUILTIN_OBJECT_KEYS_FUNCTION_ID: &str = "$builtin.Object.keys";
pub const BUILTIN_OBJECT_VALUES_FUNCTION_ID: &str = "$builtin.Object.values";
pub const BUILTIN_OBJECT_ENTRIES_FUNCTION_ID: &str = "$builtin.Object.entries";
pub const BUILTIN_OBJECT_HAS_OWN_FUNCTION_ID: &str = "$builtin.Object.hasOwn";
pub const BUILTIN_OBJECT_IS_FUNCTION_ID: &str = "$builtin.Object.is";
pub const BUILTIN_OBJECT_IS_SEALED_FUNCTION_ID: &str = "$builtin.Object.isSealed";
pub const BUILTIN_OBJECT_IS_FROZEN_FUNCTION_ID: &str = "$builtin.Object.isFrozen";
pub const BUILTIN_OBJECT_SEAL_FUNCTION_ID: &str = "$builtin.Object.seal";
pub const BUILTIN_OBJECT_FREEZE_FUNCTION_ID: &str = "$builtin.Object.freeze";
pub const BUILTIN_OBJECT_IS_EXTENSIBLE_FUNCTION_ID: &str = "$builtin.Object.isExtensible";
pub const BUILTIN_OBJECT_PREVENT_EXTENSIONS_FUNCTION_ID: &str = "$builtin.Object.preventExtensions";
pub const BUILTIN_OBJECT_PROTOTYPE_HAS_OWN_PROPERTY_FUNCTION_ID: &str =
    "$builtin.Object.prototype.hasOwnProperty";
pub const BUILTIN_OBJECT_PROTOTYPE_LOOKUP_GETTER_FUNCTION_ID: &str =
    "$builtin.Object.prototype.__lookupGetter__";
pub const BUILTIN_OBJECT_PROTOTYPE_LOOKUP_SETTER_FUNCTION_ID: &str =
    "$builtin.Object.prototype.__lookupSetter__";
pub const BUILTIN_OBJECT_PROTOTYPE_PROTO_GETTER_FUNCTION_ID: &str =
    "$builtin.Object.prototype.__proto__.get";
pub const BUILTIN_OBJECT_PROTOTYPE_PROTO_SETTER_FUNCTION_ID: &str =
    "$builtin.Object.prototype.__proto__.set";
pub const BUILTIN_OBJECT_PROTOTYPE_PROPERTY_IS_ENUMERABLE_FUNCTION_ID: &str =
    "$builtin.Object.prototype.propertyIsEnumerable";
pub const BUILTIN_OBJECT_PROTOTYPE_IS_PROTOTYPE_OF_FUNCTION_ID: &str =
    "$builtin.Object.prototype.isPrototypeOf";
pub const BUILTIN_OBJECT_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.Object.prototype.toString";
pub const BUILTIN_OBJECT_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID: &str =
    "$builtin.Object.prototype.toLocaleString";
pub const BUILTIN_OBJECT_PROTOTYPE_VALUE_OF_FUNCTION_ID: &str = "$builtin.Object.prototype.valueOf";
pub const BUILTIN_PROXY_FUNCTION_ID: &str = "$builtin.Proxy";
pub const BUILTIN_PROXY_REVOCABLE_FUNCTION_ID: &str = "$builtin.Proxy.revocable";
pub const BUILTIN_PROXY_REVOKE_FUNCTION_ID: &str = "$builtin.[[ProxyRevoke]]";
pub const BUILTIN_REFLECT_CONSTRUCT_FUNCTION_ID: &str = "$builtin.Reflect.construct";
pub const BUILTIN_REFLECT_APPLY_FUNCTION_ID: &str = "$builtin.Reflect.apply";
pub const BUILTIN_REFLECT_GET_FUNCTION_ID: &str = "$builtin.Reflect.get";
pub const BUILTIN_REFLECT_GET_PROTOTYPE_OF_FUNCTION_ID: &str = "$builtin.Reflect.getPrototypeOf";
pub const BUILTIN_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR_FUNCTION_ID: &str =
    "$builtin.Reflect.getOwnPropertyDescriptor";
pub const BUILTIN_REFLECT_SET_FUNCTION_ID: &str = "$builtin.Reflect.set";
pub const BUILTIN_REFLECT_HAS_FUNCTION_ID: &str = "$builtin.Reflect.has";
pub const BUILTIN_REFLECT_DEFINE_PROPERTY_FUNCTION_ID: &str = "$builtin.Reflect.defineProperty";
pub const BUILTIN_REFLECT_DELETE_PROPERTY_FUNCTION_ID: &str = "$builtin.Reflect.deleteProperty";
pub const BUILTIN_REFLECT_IS_EXTENSIBLE_FUNCTION_ID: &str = "$builtin.Reflect.isExtensible";
pub const BUILTIN_REFLECT_PREVENT_EXTENSIONS_FUNCTION_ID: &str =
    "$builtin.Reflect.preventExtensions";
pub const BUILTIN_REFLECT_SET_PROTOTYPE_OF_FUNCTION_ID: &str = "$builtin.Reflect.setPrototypeOf";
pub const BUILTIN_REFLECT_OWN_KEYS_FUNCTION_ID: &str = "$builtin.Reflect.ownKeys";
pub const BUILTIN_ARRAY_FUNCTION_ID: &str = "$builtin.Array";
pub const BUILTIN_ARRAY_FROM_FUNCTION_ID: &str = "$builtin.Array.from";
pub const BUILTIN_ARRAY_FROM_ASYNC_FUNCTION_ID: &str = "$builtin.Array.fromAsync";
pub const BUILTIN_ARRAY_FROM_ASYNC_FULFILLED_FUNCTION_ID: &str = "$builtin.ArrayFromAsyncFulfilled";
pub const BUILTIN_ARRAY_FROM_ASYNC_REJECTED_FUNCTION_ID: &str = "$builtin.ArrayFromAsyncRejected";
pub const BUILTIN_ARRAY_OF_FUNCTION_ID: &str = "$builtin.Array.of";
pub const BUILTIN_ARRAY_IS_ARRAY_FUNCTION_ID: &str = "$builtin.Array.isArray";
pub const BUILTIN_ARRAY_SPECIES_GETTER_FUNCTION_ID: &str = "$builtin.Array[Symbol.species].get";
pub const BUILTIN_ARRAY_PROTOTYPE_CONCAT_FUNCTION_ID: &str = "$builtin.Array.prototype.concat";
pub const BUILTIN_ARRAY_PROTOTYPE_JOIN_FUNCTION_ID: &str = "$builtin.Array.prototype.join";
pub const BUILTIN_ARRAY_PROTOTYPE_SLICE_FUNCTION_ID: &str = "$builtin.Array.prototype.slice";
pub const BUILTIN_ARRAY_PROTOTYPE_SPLICE_FUNCTION_ID: &str = "$builtin.Array.prototype.splice";
pub const BUILTIN_ARRAY_PROTOTYPE_SORT_FUNCTION_ID: &str = "$builtin.Array.prototype.sort";
pub const BUILTIN_ARRAY_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID: &str =
    "$builtin.Array.prototype.toLocaleString";
pub const BUILTIN_ARRAY_PROTOTYPE_FLAT_FUNCTION_ID: &str = "$builtin.Array.prototype.flat";
pub const BUILTIN_ARRAY_PROTOTYPE_FLAT_MAP_FUNCTION_ID: &str = "$builtin.Array.prototype.flatMap";
pub const BUILTIN_ARRAY_PROTOTYPE_AT_FUNCTION_ID: &str = "$builtin.Array.prototype.at";
pub const BUILTIN_ARRAY_PROTOTYPE_TO_REVERSED_FUNCTION_ID: &str =
    "$builtin.Array.prototype.toReversed";
pub const BUILTIN_ARRAY_PROTOTYPE_TO_SPLICED_FUNCTION_ID: &str =
    "$builtin.Array.prototype.toSpliced";
pub const BUILTIN_ARRAY_PROTOTYPE_TO_SORTED_FUNCTION_ID: &str = "$builtin.Array.prototype.toSorted";
pub const BUILTIN_ARRAY_PROTOTYPE_WITH_FUNCTION_ID: &str = "$builtin.Array.prototype.with";
pub const BUILTIN_ARRAY_PROTOTYPE_REVERSE_FUNCTION_ID: &str = "$builtin.Array.prototype.reverse";
pub const BUILTIN_ARRAY_PROTOTYPE_COPY_WITHIN_FUNCTION_ID: &str =
    "$builtin.Array.prototype.copyWithin";
pub const BUILTIN_ARRAY_PROTOTYPE_INCLUDES_FUNCTION_ID: &str = "$builtin.Array.prototype.includes";
pub const BUILTIN_ARRAY_PROTOTYPE_INDEX_OF_FUNCTION_ID: &str = "$builtin.Array.prototype.indexOf";
pub const BUILTIN_ARRAY_PROTOTYPE_LAST_INDEX_OF_FUNCTION_ID: &str =
    "$builtin.Array.prototype.lastIndexOf";
pub const BUILTIN_ARRAY_PROTOTYPE_FIND_FUNCTION_ID: &str = "$builtin.Array.prototype.find";
pub const BUILTIN_ARRAY_PROTOTYPE_FIND_INDEX_FUNCTION_ID: &str =
    "$builtin.Array.prototype.findIndex";
pub const BUILTIN_ARRAY_PROTOTYPE_FIND_LAST_FUNCTION_ID: &str = "$builtin.Array.prototype.findLast";
pub const BUILTIN_ARRAY_PROTOTYPE_FIND_LAST_INDEX_FUNCTION_ID: &str =
    "$builtin.Array.prototype.findLastIndex";
pub const BUILTIN_ARRAY_PROTOTYPE_EVERY_FUNCTION_ID: &str = "$builtin.Array.prototype.every";
pub const BUILTIN_ARRAY_PROTOTYPE_SOME_FUNCTION_ID: &str = "$builtin.Array.prototype.some";
pub const BUILTIN_ARRAY_PROTOTYPE_FOR_EACH_FUNCTION_ID: &str = "$builtin.Array.prototype.forEach";
pub const BUILTIN_ARRAY_PROTOTYPE_FILTER_FUNCTION_ID: &str = "$builtin.Array.prototype.filter";
pub const BUILTIN_ARRAY_PROTOTYPE_MAP_FUNCTION_ID: &str = "$builtin.Array.prototype.map";
pub const BUILTIN_ARRAY_PROTOTYPE_REDUCE_FUNCTION_ID: &str = "$builtin.Array.prototype.reduce";
pub const BUILTIN_ARRAY_PROTOTYPE_REDUCE_RIGHT_FUNCTION_ID: &str =
    "$builtin.Array.prototype.reduceRight";
pub const BUILTIN_ARRAY_PROTOTYPE_POP_FUNCTION_ID: &str = "$builtin.Array.prototype.pop";
pub const BUILTIN_ARRAY_PROTOTYPE_PUSH_FUNCTION_ID: &str = "$builtin.Array.prototype.push";
pub const BUILTIN_ARRAY_PROTOTYPE_SHIFT_FUNCTION_ID: &str = "$builtin.Array.prototype.shift";
pub const BUILTIN_ARRAY_PROTOTYPE_UNSHIFT_FUNCTION_ID: &str = "$builtin.Array.prototype.unshift";
pub const BUILTIN_ARRAY_PROTOTYPE_FILL_FUNCTION_ID: &str = "$builtin.Array.prototype.fill";
pub const BUILTIN_ARRAY_PROTOTYPE_KEYS_FUNCTION_ID: &str = "$builtin.Array.prototype.keys";
pub const BUILTIN_ARRAY_PROTOTYPE_ENTRIES_FUNCTION_ID: &str = "$builtin.Array.prototype.entries";
pub const BUILTIN_ARRAY_PROTOTYPE_VALUES_FUNCTION_ID: &str = "$builtin.Array.prototype.values";
pub const BUILTIN_ARRAY_ITERATOR_NEXT_FUNCTION_ID: &str = "$builtin.ArrayIterator.next";
pub const BUILTIN_ARRAY_ITERATOR_IDENTITY_FUNCTION_ID: &str = "$builtin.ArrayIterator.identity";
pub const BUILTIN_STRING_ITERATOR_NEXT_FUNCTION_ID: &str = "$builtin.StringIterator.next";
pub const BUILTIN_REGEXP_STRING_ITERATOR_NEXT_FUNCTION_ID: &str =
    "$builtin.RegExpStringIterator.next";
pub const BUILTIN_GENERATOR_PROTOTYPE_NEXT_FUNCTION_ID: &str = "$builtin.Generator.prototype.next";
pub const BUILTIN_GENERATOR_PROTOTYPE_RETURN_FUNCTION_ID: &str =
    "$builtin.Generator.prototype.return";
pub const BUILTIN_GENERATOR_PROTOTYPE_THROW_FUNCTION_ID: &str =
    "$builtin.Generator.prototype.throw";
pub const BUILTIN_ASYNC_GENERATOR_PROTOTYPE_NEXT_FUNCTION_ID: &str =
    "$builtin.AsyncGenerator.prototype.next";
pub const BUILTIN_ASYNC_GENERATOR_PROTOTYPE_RETURN_FUNCTION_ID: &str =
    "$builtin.AsyncGenerator.prototype.return";
pub const BUILTIN_ASYNC_GENERATOR_PROTOTYPE_THROW_FUNCTION_ID: &str =
    "$builtin.AsyncGenerator.prototype.throw";
pub const BUILTIN_ASYNC_ITERATOR_PROTOTYPE_ASYNC_DISPOSE_FUNCTION_ID: &str =
    "$builtin.AsyncIterator.prototype.asyncDispose";
pub const BUILTIN_ASYNC_ITERATOR_PROTOTYPE_ASYNC_DISPOSE_FULFILLED_FUNCTION_ID: &str =
    "$builtin.AsyncIterator.prototype.asyncDispose.fulfilled";
pub const BUILTIN_ASYNC_ITERATOR_PROTOTYPE_ASYNC_DISPOSE_REJECTED_FUNCTION_ID: &str =
    "$builtin.AsyncIterator.prototype.asyncDispose.rejected";
pub const BUILTIN_ITERATOR_FUNCTION_ID: &str = "$builtin.Iterator";
pub const BUILTIN_ITERATOR_FROM_FUNCTION_ID: &str = "$builtin.Iterator.from";
pub const BUILTIN_ITERATOR_CONCAT_FUNCTION_ID: &str = "$builtin.Iterator.concat";
pub const BUILTIN_ITERATOR_CONCAT_NEXT_FUNCTION_ID: &str = "$builtin.Iterator.concat.next";
pub const BUILTIN_ITERATOR_CONCAT_RETURN_FUNCTION_ID: &str = "$builtin.Iterator.concat.return";
pub const BUILTIN_ITERATOR_ZIP_FUNCTION_ID: &str = "$builtin.Iterator.zip";
pub const BUILTIN_ITERATOR_ZIP_KEYED_FUNCTION_ID: &str = "$builtin.Iterator.zipKeyed";
pub const BUILTIN_ITERATOR_ZIP_NEXT_FUNCTION_ID: &str = "$builtin.Iterator.zip.next";
pub const BUILTIN_ITERATOR_ZIP_RETURN_FUNCTION_ID: &str = "$builtin.Iterator.zip.return";
pub const BUILTIN_ITERATOR_HELPER_NEXT_FUNCTION_ID: &str = "$builtin.Iterator.helper.next";
pub const BUILTIN_ITERATOR_HELPER_RETURN_FUNCTION_ID: &str = "$builtin.Iterator.helper.return";
pub const BUILTIN_ITERATOR_PROTOTYPE_TO_ARRAY_FUNCTION_ID: &str =
    "$builtin.Iterator.prototype.toArray";
pub const BUILTIN_ITERATOR_PROTOTYPE_FOR_EACH_FUNCTION_ID: &str =
    "$builtin.Iterator.prototype.forEach";
pub const BUILTIN_ITERATOR_PROTOTYPE_EVERY_FUNCTION_ID: &str = "$builtin.Iterator.prototype.every";
pub const BUILTIN_ITERATOR_PROTOTYPE_SOME_FUNCTION_ID: &str = "$builtin.Iterator.prototype.some";
pub const BUILTIN_ITERATOR_PROTOTYPE_FIND_FUNCTION_ID: &str = "$builtin.Iterator.prototype.find";
pub const BUILTIN_ITERATOR_PROTOTYPE_REDUCE_FUNCTION_ID: &str =
    "$builtin.Iterator.prototype.reduce";
pub const BUILTIN_ITERATOR_PROTOTYPE_MAP_FUNCTION_ID: &str = "$builtin.Iterator.prototype.map";
pub const BUILTIN_ITERATOR_MAP_NEXT_FUNCTION_ID: &str = "$builtin.Iterator.map.next";
pub const BUILTIN_ITERATOR_MAP_RETURN_FUNCTION_ID: &str = "$builtin.Iterator.map.return";
pub const BUILTIN_ITERATOR_PROTOTYPE_FILTER_FUNCTION_ID: &str =
    "$builtin.Iterator.prototype.filter";
pub const BUILTIN_ITERATOR_FILTER_NEXT_FUNCTION_ID: &str = "$builtin.Iterator.filter.next";
pub const BUILTIN_ITERATOR_FILTER_RETURN_FUNCTION_ID: &str = "$builtin.Iterator.filter.return";
pub const BUILTIN_ITERATOR_PROTOTYPE_FLAT_MAP_FUNCTION_ID: &str =
    "$builtin.Iterator.prototype.flatMap";
pub const BUILTIN_ITERATOR_FLAT_MAP_NEXT_FUNCTION_ID: &str = "$builtin.Iterator.flatMap.next";
pub const BUILTIN_ITERATOR_FLAT_MAP_RETURN_FUNCTION_ID: &str = "$builtin.Iterator.flatMap.return";
pub const BUILTIN_ITERATOR_PROTOTYPE_TAKE_FUNCTION_ID: &str = "$builtin.Iterator.prototype.take";
pub const BUILTIN_ITERATOR_TAKE_NEXT_FUNCTION_ID: &str = "$builtin.Iterator.take.next";
pub const BUILTIN_ITERATOR_TAKE_RETURN_FUNCTION_ID: &str = "$builtin.Iterator.take.return";
pub const BUILTIN_ITERATOR_PROTOTYPE_DROP_FUNCTION_ID: &str = "$builtin.Iterator.prototype.drop";
pub const BUILTIN_ITERATOR_DROP_NEXT_FUNCTION_ID: &str = "$builtin.Iterator.drop.next";
pub const BUILTIN_ITERATOR_DROP_RETURN_FUNCTION_ID: &str = "$builtin.Iterator.drop.return";
pub const BUILTIN_ITERATOR_PROTOTYPE_CONSTRUCTOR_GETTER_FUNCTION_ID: &str =
    "$builtin.Iterator.prototype.constructor.get";
pub const BUILTIN_ITERATOR_PROTOTYPE_CONSTRUCTOR_SETTER_FUNCTION_ID: &str =
    "$builtin.Iterator.prototype.constructor.set";
pub const BUILTIN_ITERATOR_PROTOTYPE_SYMBOL_DISPOSE_FUNCTION_ID: &str =
    "$builtin.Iterator.prototype[Symbol.dispose]";
pub const BUILTIN_ITERATOR_PROTOTYPE_TO_STRING_TAG_GETTER_FUNCTION_ID: &str =
    "$builtin.Iterator.prototype[Symbol.toStringTag].get";
pub const BUILTIN_ITERATOR_PROTOTYPE_TO_STRING_TAG_SETTER_FUNCTION_ID: &str =
    "$builtin.Iterator.prototype[Symbol.toStringTag].set";
pub const BUILTIN_ITERATOR_FROM_WRAPPER_RETURN_FUNCTION_ID: &str =
    "$builtin.Iterator.from.wrapper.return";
pub const BUILTIN_ITERATOR_FROM_WRAPPER_NEXT_FUNCTION_ID: &str =
    "$builtin.Iterator.from.wrapper.next";
pub const BUILTIN_ARRAY_BUFFER_FUNCTION_ID: &str = "$builtin.ArrayBuffer";
pub const BUILTIN_SHARED_ARRAY_BUFFER_FUNCTION_ID: &str = "$builtin.SharedArrayBuffer";
pub const BUILTIN_ARRAY_BUFFER_IS_VIEW_FUNCTION_ID: &str = "$builtin.ArrayBuffer.isView";
pub const BUILTIN_ARRAY_BUFFER_SPECIES_GETTER_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer[Symbol.species].get";
pub const BUILTIN_ARRAY_BUFFER_PROTOTYPE_BYTE_LENGTH_GETTER_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer.prototype.byteLength.get";
pub const BUILTIN_SHARED_ARRAY_BUFFER_PROTOTYPE_BYTE_LENGTH_GETTER_FUNCTION_ID: &str =
    "$builtin.SharedArrayBuffer.prototype.byteLength.get";
pub const BUILTIN_SHARED_ARRAY_BUFFER_PROTOTYPE_MAX_BYTE_LENGTH_GETTER_FUNCTION_ID: &str =
    "$builtin.SharedArrayBuffer.prototype.maxByteLength.get";
pub const BUILTIN_SHARED_ARRAY_BUFFER_PROTOTYPE_GROWABLE_GETTER_FUNCTION_ID: &str =
    "$builtin.SharedArrayBuffer.prototype.growable.get";
pub const BUILTIN_SHARED_ARRAY_BUFFER_PROTOTYPE_GROW_FUNCTION_ID: &str =
    "$builtin.SharedArrayBuffer.prototype.grow";
pub const BUILTIN_ARRAY_BUFFER_PROTOTYPE_DETACHED_GETTER_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer.prototype.detached.get";
pub const BUILTIN_ARRAY_BUFFER_PROTOTYPE_MAX_BYTE_LENGTH_GETTER_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer.prototype.maxByteLength.get";
pub const BUILTIN_ARRAY_BUFFER_PROTOTYPE_IMMUTABLE_GETTER_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer.prototype.immutable.get";
pub const BUILTIN_ARRAY_BUFFER_PROTOTYPE_RESIZABLE_GETTER_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer.prototype.resizable.get";
pub const BUILTIN_ARRAY_BUFFER_PROTOTYPE_RESIZE_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer.prototype.resize";
pub const BUILTIN_ARRAY_BUFFER_PROTOTYPE_SLICE_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer.prototype.slice";
pub const BUILTIN_SHARED_ARRAY_BUFFER_PROTOTYPE_SLICE_FUNCTION_ID: &str =
    "$builtin.SharedArrayBuffer.prototype.slice";
pub const BUILTIN_ARRAY_BUFFER_PROTOTYPE_TRANSFER_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer.prototype.transfer";
pub const BUILTIN_ARRAY_BUFFER_PROTOTYPE_TRANSFER_TO_FIXED_LENGTH_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer.prototype.transferToFixedLength";
pub const BUILTIN_ARRAY_BUFFER_PROTOTYPE_TRANSFER_TO_IMMUTABLE_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer.prototype.transferToImmutable";
pub const BUILTIN_ARRAY_BUFFER_PROTOTYPE_SLICE_TO_IMMUTABLE_FUNCTION_ID: &str =
    "$builtin.ArrayBuffer.prototype.sliceToImmutable";
pub const BUILTIN_DATA_VIEW_FUNCTION_ID: &str = "$builtin.DataView";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_BUFFER_GETTER_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.buffer.get";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_BYTE_LENGTH_GETTER_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.byteLength.get";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_BYTE_OFFSET_GETTER_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.byteOffset.get";
pub const BUILTIN_TYPED_ARRAY_CONSTRUCTOR_FUNCTION_ID: &str = "$builtin.TypedArray";
pub const BUILTIN_TYPED_ARRAY_SPECIES_GETTER_FUNCTION_ID: &str =
    "$builtin.TypedArray[Symbol.species].get";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_BYTE_LENGTH_GETTER_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.byteLength.get";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_BYTE_OFFSET_GETTER_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.byteOffset.get";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_LENGTH_GETTER_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.length.get";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_BUFFER_GETTER_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.buffer.get";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_TO_STRING_TAG_GETTER_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype[Symbol.toStringTag].get";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.toString";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_AT_FUNCTION_ID: &str = "$builtin.TypedArray.prototype.at";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_COPY_WITHIN_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.copyWithin";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_FILL_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.fill";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_INCLUDES_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.includes";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_INDEX_OF_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.indexOf";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_LAST_INDEX_OF_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.lastIndexOf";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_FIND_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.find";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_FIND_INDEX_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.findIndex";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_FIND_LAST_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.findLast";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_FIND_LAST_INDEX_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.findLastIndex";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_EVERY_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.every";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_SOME_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.some";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_MAP_FUNCTION_ID: &str = "$builtin.TypedArray.prototype.map";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_FILTER_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.filter";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_FOR_EACH_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.forEach";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_REDUCE_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.reduce";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_REDUCE_RIGHT_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.reduceRight";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_VALUES_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.values";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_KEYS_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.keys";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_ENTRIES_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.entries";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_JOIN_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.join";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.toLocaleString";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_SUBARRAY_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.subarray";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_SLICE_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.slice";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_SET_FUNCTION_ID: &str = "$builtin.TypedArray.prototype.set";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_REVERSE_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.reverse";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_SORT_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.sort";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_TO_REVERSED_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.toReversed";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_TO_SORTED_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.toSorted";
pub const BUILTIN_TYPED_ARRAY_PROTOTYPE_WITH_FUNCTION_ID: &str =
    "$builtin.TypedArray.prototype.with";
pub const BUILTIN_TYPED_ARRAY_FROM_FUNCTION_ID: &str = "$builtin.TypedArray.from";
pub const BUILTIN_TYPED_ARRAY_OF_FUNCTION_ID: &str = "$builtin.TypedArray.of";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_GET_UINT8_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.getUint8";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_SET_UINT8_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.setUint8";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_GET_INT8_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.getInt8";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_SET_INT8_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.setInt8";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_GET_UINT16_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.getUint16";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_SET_UINT16_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.setUint16";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_GET_INT16_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.getInt16";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_SET_INT16_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.setInt16";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_GET_UINT32_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.getUint32";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_SET_UINT32_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.setUint32";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_GET_INT32_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.getInt32";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_SET_INT32_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.setInt32";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_GET_FLOAT16_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.getFloat16";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_SET_FLOAT16_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.setFloat16";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_GET_FLOAT32_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.getFloat32";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_SET_FLOAT32_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.setFloat32";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_GET_FLOAT64_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.getFloat64";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_SET_FLOAT64_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.setFloat64";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_GET_BIGINT64_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.getBigInt64";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_SET_BIGINT64_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.setBigInt64";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_GET_BIGUINT64_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.getBigUint64";
pub const BUILTIN_DATA_VIEW_PROTOTYPE_SET_BIGUINT64_FUNCTION_ID: &str =
    "$builtin.DataView.prototype.setBigUint64";
pub const BUILTIN_DATE_FUNCTION_ID: &str = "$builtin.Date";
pub const BUILTIN_DATE_NOW_FUNCTION_ID: &str = "$builtin.Date.now";
pub const BUILTIN_DATE_PARSE_FUNCTION_ID: &str = "$builtin.Date.parse";
pub const BUILTIN_DATE_UTC_FUNCTION_ID: &str = "$builtin.Date.UTC";
pub const BUILTIN_DATE_PROTOTYPE_GET_TIME_FUNCTION_ID: &str = "$builtin.Date.prototype.getTime";
pub const BUILTIN_DATE_PROTOTYPE_SET_TIME_FUNCTION_ID: &str = "$builtin.Date.prototype.setTime";
pub const BUILTIN_DATE_PROTOTYPE_VALUE_OF_FUNCTION_ID: &str = "$builtin.Date.prototype.valueOf";
pub const BUILTIN_DATE_PROTOTYPE_GET_FULL_YEAR_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getFullYear";
pub const BUILTIN_DATE_PROTOTYPE_GET_UTC_FULL_YEAR_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getUTCFullYear";
pub const BUILTIN_DATE_PROTOTYPE_GET_MONTH_FUNCTION_ID: &str = "$builtin.Date.prototype.getMonth";
pub const BUILTIN_DATE_PROTOTYPE_GET_UTC_MONTH_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getUTCMonth";
pub const BUILTIN_DATE_PROTOTYPE_GET_DATE_FUNCTION_ID: &str = "$builtin.Date.prototype.getDate";
pub const BUILTIN_DATE_PROTOTYPE_GET_UTC_DATE_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getUTCDate";
pub const BUILTIN_DATE_PROTOTYPE_GET_DAY_FUNCTION_ID: &str = "$builtin.Date.prototype.getDay";
pub const BUILTIN_DATE_PROTOTYPE_GET_UTC_DAY_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getUTCDay";
pub const BUILTIN_DATE_PROTOTYPE_GET_HOURS_FUNCTION_ID: &str = "$builtin.Date.prototype.getHours";
pub const BUILTIN_DATE_PROTOTYPE_GET_UTC_HOURS_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getUTCHours";
pub const BUILTIN_DATE_PROTOTYPE_GET_MINUTES_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getMinutes";
pub const BUILTIN_DATE_PROTOTYPE_GET_UTC_MINUTES_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getUTCMinutes";
pub const BUILTIN_DATE_PROTOTYPE_GET_SECONDS_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getSeconds";
pub const BUILTIN_DATE_PROTOTYPE_GET_UTC_SECONDS_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getUTCSeconds";
pub const BUILTIN_DATE_PROTOTYPE_GET_MILLISECONDS_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getMilliseconds";
pub const BUILTIN_DATE_PROTOTYPE_GET_UTC_MILLISECONDS_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getUTCMilliseconds";
pub const BUILTIN_DATE_PROTOTYPE_GET_TIMEZONE_OFFSET_FUNCTION_ID: &str =
    "$builtin.Date.prototype.getTimezoneOffset";
pub const BUILTIN_DATE_PROTOTYPE_GET_YEAR_FUNCTION_ID: &str = "$builtin.Date.prototype.getYear";
pub const BUILTIN_DATE_PROTOTYPE_SET_YEAR_FUNCTION_ID: &str = "$builtin.Date.prototype.setYear";
pub const BUILTIN_DATE_PROTOTYPE_SET_FULL_YEAR_FUNCTION_ID: &str =
    "$builtin.Date.prototype.setFullYear";
pub const BUILTIN_DATE_PROTOTYPE_SET_UTC_FULL_YEAR_FUNCTION_ID: &str =
    "$builtin.Date.prototype.setUTCFullYear";
pub const BUILTIN_DATE_PROTOTYPE_SET_MONTH_FUNCTION_ID: &str = "$builtin.Date.prototype.setMonth";
pub const BUILTIN_DATE_PROTOTYPE_SET_UTC_MONTH_FUNCTION_ID: &str =
    "$builtin.Date.prototype.setUTCMonth";
pub const BUILTIN_DATE_PROTOTYPE_SET_DATE_FUNCTION_ID: &str = "$builtin.Date.prototype.setDate";
pub const BUILTIN_DATE_PROTOTYPE_SET_UTC_DATE_FUNCTION_ID: &str =
    "$builtin.Date.prototype.setUTCDate";
pub const BUILTIN_DATE_PROTOTYPE_SET_HOURS_FUNCTION_ID: &str = "$builtin.Date.prototype.setHours";
pub const BUILTIN_DATE_PROTOTYPE_SET_UTC_HOURS_FUNCTION_ID: &str =
    "$builtin.Date.prototype.setUTCHours";
pub const BUILTIN_DATE_PROTOTYPE_SET_MINUTES_FUNCTION_ID: &str =
    "$builtin.Date.prototype.setMinutes";
pub const BUILTIN_DATE_PROTOTYPE_SET_UTC_MINUTES_FUNCTION_ID: &str =
    "$builtin.Date.prototype.setUTCMinutes";
pub const BUILTIN_DATE_PROTOTYPE_SET_SECONDS_FUNCTION_ID: &str =
    "$builtin.Date.prototype.setSeconds";
pub const BUILTIN_DATE_PROTOTYPE_SET_UTC_SECONDS_FUNCTION_ID: &str =
    "$builtin.Date.prototype.setUTCSeconds";
pub const BUILTIN_DATE_PROTOTYPE_SET_MILLISECONDS_FUNCTION_ID: &str =
    "$builtin.Date.prototype.setMilliseconds";
pub const BUILTIN_DATE_PROTOTYPE_SET_UTC_MILLISECONDS_FUNCTION_ID: &str =
    "$builtin.Date.prototype.setUTCMilliseconds";
pub const BUILTIN_DATE_PROTOTYPE_TO_ISO_STRING_FUNCTION_ID: &str =
    "$builtin.Date.prototype.toISOString";
pub const BUILTIN_DATE_PROTOTYPE_TO_JSON_FUNCTION_ID: &str = "$builtin.Date.prototype.toJSON";
pub const BUILTIN_DATE_PROTOTYPE_TO_PRIMITIVE_FUNCTION_ID: &str =
    "$builtin.Date.prototype.toPrimitive";
pub const BUILTIN_DATE_PROTOTYPE_TO_DATE_STRING_FUNCTION_ID: &str =
    "$builtin.Date.prototype.toDateString";
pub const BUILTIN_DATE_PROTOTYPE_TO_LOCALE_DATE_STRING_FUNCTION_ID: &str =
    "$builtin.Date.prototype.toLocaleDateString";
pub const BUILTIN_DATE_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID: &str =
    "$builtin.Date.prototype.toLocaleString";
pub const BUILTIN_DATE_PROTOTYPE_TO_LOCALE_TIME_STRING_FUNCTION_ID: &str =
    "$builtin.Date.prototype.toLocaleTimeString";
pub const BUILTIN_DATE_PROTOTYPE_TO_TEMPORAL_INSTANT_FUNCTION_ID: &str =
    "$builtin.Date.prototype.toTemporalInstant";
pub const BUILTIN_DATE_PROTOTYPE_TO_TIME_STRING_FUNCTION_ID: &str =
    "$builtin.Date.prototype.toTimeString";
pub const BUILTIN_DATE_PROTOTYPE_TO_STRING_FUNCTION_ID: &str = "$builtin.Date.prototype.toString";
pub const BUILTIN_DATE_PROTOTYPE_TO_UTC_STRING_FUNCTION_ID: &str =
    "$builtin.Date.prototype.toUTCString";
pub const BUILTIN_TEMPORAL_NOW_INSTANT_FUNCTION_ID: &str = "$builtin.Temporal.Now.instant";
pub const BUILTIN_TEMPORAL_NOW_TIME_ZONE_ID_FUNCTION_ID: &str = "$builtin.Temporal.Now.timeZoneId";
pub const BUILTIN_TEMPORAL_NOW_ZONED_DATE_TIME_ISO_FUNCTION_ID: &str =
    "$builtin.Temporal.Now.zonedDateTimeISO";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_FUNCTION_ID: &str = "$builtin.Temporal.PlainDate";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_FROM_FUNCTION_ID: &str = "$builtin.Temporal.PlainDate.from";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_COMPARE_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.compare";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_CALENDAR_ID_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.calendarId.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_ERA_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.era.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_ERA_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.eraYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.year.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_MONTH_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.month.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_MONTH_CODE_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.monthCode.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAY_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.day.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAY_OF_WEEK_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.dayOfWeek.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAY_OF_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.dayOfYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_WEEK_OF_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.weekOfYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_YEAR_OF_WEEK_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.yearOfWeek.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAYS_IN_WEEK_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.daysInWeek.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAYS_IN_MONTH_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.daysInMonth.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_DAYS_IN_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.daysInYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_MONTHS_IN_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.monthsInYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_IN_LEAP_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.inLeapYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_WITH_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.with";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_WITH_CALENDAR_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.withCalendar";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_EQUALS_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.equals";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.toString";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_JSON_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.toJSON";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.toLocaleString";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_VALUE_OF_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.valueOf";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_ADD_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.add";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_SUBTRACT_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.subtract";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_UNTIL_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.until";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_SINCE_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.since";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_PLAIN_DATE_TIME_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.toPlainDateTime";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_PLAIN_YEAR_MONTH_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.toPlainYearMonth";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_PROTOTYPE_TO_PLAIN_MONTH_DAY_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDate.prototype.toPlainMonthDay";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_FUNCTION_ID: &str = "$builtin.Temporal.PlainYearMonth";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_FROM_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.from";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_COMPARE_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.compare";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_CALENDAR_ID_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.calendarId.get";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_ERA_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.era.get";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_ERA_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.eraYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.year.get";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_MONTH_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.month.get";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_MONTH_CODE_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.monthCode.get";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_DAYS_IN_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.daysInYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_DAYS_IN_MONTH_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.daysInMonth.get";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_MONTHS_IN_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.monthsInYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_IN_LEAP_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.inLeapYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_WITH_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.with";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_ADD_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.add";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_SUBTRACT_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.subtract";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_UNTIL_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.until";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_SINCE_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.since";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_EQUALS_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.equals";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.toString";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_TO_JSON_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.toJSON";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.toLocaleString";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_VALUE_OF_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.valueOf";
pub const BUILTIN_TEMPORAL_PLAIN_YEAR_MONTH_PROTOTYPE_TO_PLAIN_DATE_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainYearMonth.prototype.toPlainDate";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_FUNCTION_ID: &str = "$builtin.Temporal.PlainMonthDay";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_FROM_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainMonthDay.from";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_CALENDAR_ID_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainMonthDay.prototype.calendarId.get";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_MONTH_CODE_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainMonthDay.prototype.monthCode.get";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_DAY_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainMonthDay.prototype.day.get";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_WITH_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainMonthDay.prototype.with";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_EQUALS_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainMonthDay.prototype.equals";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainMonthDay.prototype.toString";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_TO_JSON_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainMonthDay.prototype.toJSON";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainMonthDay.prototype.toLocaleString";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_VALUE_OF_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainMonthDay.prototype.valueOf";
pub const BUILTIN_TEMPORAL_PLAIN_MONTH_DAY_PROTOTYPE_TO_PLAIN_DATE_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainMonthDay.prototype.toPlainDate";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_FUNCTION_ID: &str = "$builtin.Temporal.PlainTime";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_FROM_FUNCTION_ID: &str = "$builtin.Temporal.PlainTime.from";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_COMPARE_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.compare";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_HOUR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.hour.get";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_MINUTE_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.minute.get";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_SECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.second.get";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_MILLISECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.millisecond.get";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_MICROSECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.microsecond.get";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_NANOSECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.nanosecond.get";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_WITH_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.with";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_ADD_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.add";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_SUBTRACT_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.subtract";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_UNTIL_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.until";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_SINCE_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.since";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_ROUND_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.round";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_EQUALS_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.equals";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.toString";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_TO_JSON_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.toJSON";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.toLocaleString";
pub const BUILTIN_TEMPORAL_PLAIN_TIME_PROTOTYPE_VALUE_OF_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainTime.prototype.valueOf";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_FUNCTION_ID: &str = "$builtin.Temporal.PlainDateTime";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_FROM_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.from";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_COMPARE_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.compare";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_CALENDAR_ID_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.calendarId.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_ERA_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.era.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_ERA_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.eraYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.year.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MONTH_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.month.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MONTH_CODE_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.monthCode.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAY_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.day.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_HOUR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.hour.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MINUTE_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.minute.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_SECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.second.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MILLISECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.millisecond.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MICROSECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.microsecond.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_NANOSECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.nanosecond.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAY_OF_WEEK_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.dayOfWeek.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAY_OF_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.dayOfYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_WEEK_OF_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.weekOfYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_YEAR_OF_WEEK_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.yearOfWeek.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAYS_IN_WEEK_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.daysInWeek.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAYS_IN_MONTH_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.daysInMonth.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_DAYS_IN_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.daysInYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_MONTHS_IN_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.monthsInYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_IN_LEAP_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.inLeapYear.get";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_WITH_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.with";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_WITH_PLAIN_TIME_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.withPlainTime";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_WITH_CALENDAR_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.withCalendar";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_ADD_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.add";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_SUBTRACT_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.subtract";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_UNTIL_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.until";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_SINCE_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.since";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_ROUND_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.round";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_EQUALS_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.equals";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.toString";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_JSON_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.toJSON";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.toLocaleString";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_VALUE_OF_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.valueOf";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_PLAIN_DATE_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.toPlainDate";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_PLAIN_TIME_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.toPlainTime";
pub const BUILTIN_TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_TO_ZONED_DATE_TIME_FUNCTION_ID: &str =
    "$builtin.Temporal.PlainDateTime.prototype.toZonedDateTime";
pub const BUILTIN_TEMPORAL_DURATION_FUNCTION_ID: &str = "$builtin.Temporal.Duration";
pub const BUILTIN_TEMPORAL_DURATION_FROM_FUNCTION_ID: &str = "$builtin.Temporal.Duration.from";
pub const BUILTIN_TEMPORAL_DURATION_COMPARE_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.compare";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_YEARS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.years.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_MONTHS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.months.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_WEEKS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.weeks.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_DAYS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.days.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_HOURS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.hours.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_MINUTES_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.minutes.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_SECONDS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.seconds.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_MILLISECONDS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.milliseconds.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_MICROSECONDS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.microseconds.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_NANOSECONDS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.nanoseconds.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_SIGN_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.sign.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_BLANK_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.blank.get";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_WITH_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.with";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_NEGATED_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.negated";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_ABS_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.abs";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_ADD_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.add";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_SUBTRACT_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.subtract";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_ROUND_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.round";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_TOTAL_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.total";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.toString";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_TO_JSON_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.toJSON";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.toLocaleString";
pub const BUILTIN_TEMPORAL_DURATION_PROTOTYPE_VALUE_OF_FUNCTION_ID: &str =
    "$builtin.Temporal.Duration.prototype.valueOf";
pub const BUILTIN_TEMPORAL_INSTANT_FUNCTION_ID: &str = "$builtin.Temporal.Instant";
pub const BUILTIN_TEMPORAL_INSTANT_FROM_FUNCTION_ID: &str = "$builtin.Temporal.Instant.from";
pub const BUILTIN_TEMPORAL_INSTANT_COMPARE_FUNCTION_ID: &str = "$builtin.Temporal.Instant.compare";
pub const BUILTIN_TEMPORAL_INSTANT_FROM_EPOCH_MILLISECONDS_FUNCTION_ID: &str =
    "$builtin.Temporal.Instant.fromEpochMilliseconds";
pub const BUILTIN_TEMPORAL_INSTANT_FROM_EPOCH_NANOSECONDS_FUNCTION_ID: &str =
    "$builtin.Temporal.Instant.fromEpochNanoseconds";
pub const BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_EPOCH_MILLISECONDS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Instant.prototype.epochMilliseconds.get";
pub const BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_EPOCH_NANOSECONDS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.Instant.prototype.epochNanoseconds.get";
pub const BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_EQUALS_FUNCTION_ID: &str =
    "$builtin.Temporal.Instant.prototype.equals";
pub const BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.Instant.prototype.toString";
pub const BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_TO_JSON_FUNCTION_ID: &str =
    "$builtin.Temporal.Instant.prototype.toJSON";
pub const BUILTIN_TEMPORAL_INSTANT_PROTOTYPE_VALUE_OF_FUNCTION_ID: &str =
    "$builtin.Temporal.Instant.prototype.valueOf";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_FUNCTION_ID: &str = "$builtin.Temporal.ZonedDateTime";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_COMPARE_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.compare";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAY_OF_WEEK_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.dayOfWeek.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAY_OF_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.dayOfYear.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_WEEK_OF_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.weekOfYear.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_YEAR_OF_WEEK_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.yearOfWeek.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAYS_IN_WEEK_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.daysInWeek.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAYS_IN_MONTH_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.daysInMonth.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAYS_IN_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.daysInYear.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MONTHS_IN_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.monthsInYear.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_IN_LEAP_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.inLeapYear.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.toString";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_FROM_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.from";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_EPOCH_MILLISECONDS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.epochMilliseconds.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_EPOCH_NANOSECONDS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.epochNanoseconds.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_OFFSET_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.offset.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_OFFSET_NANOSECONDS_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.offsetNanoseconds.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_TIME_ZONE_ID_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.timeZoneId.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_CALENDAR_ID_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.calendarId.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_ERA_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.era.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_ERA_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.eraYear.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_YEAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.year.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MONTH_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.month.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MONTH_CODE_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.monthCode.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_DAY_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.day.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_HOUR_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.hour.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MINUTE_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.minute.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_SECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.second.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MILLISECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.millisecond.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_MICROSECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.microsecond.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_NANOSECOND_GETTER_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.nanosecond.get";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_EQUALS_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.equals";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_TO_INSTANT_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.toInstant";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_TO_PLAIN_DATE_TIME_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.toPlainDateTime";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_WITH_TIME_ZONE_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.withTimeZone";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_WITH_CALENDAR_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.withCalendar";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_ADD_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.add";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_SUBTRACT_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.subtract";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_UNTIL_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.until";
pub const BUILTIN_TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_SINCE_FUNCTION_ID: &str =
    "$builtin.Temporal.ZonedDateTime.prototype.since";
pub const BUILTIN_INTL_SUPPORTED_VALUES_OF_FUNCTION_ID: &str = "$builtin.Intl.supportedValuesOf";
pub const BUILTIN_INTL_GET_CANONICAL_LOCALES_FUNCTION_ID: &str =
    "$builtin.Intl.getCanonicalLocales";
pub const BUILTIN_INTL_LOCALE_FUNCTION_ID: &str = "$builtin.Intl.Locale";
pub const BUILTIN_INTL_DATE_TIME_FORMAT_FUNCTION_ID: &str = "$builtin.Intl.DateTimeFormat";
pub const BUILTIN_INTL_DATE_TIME_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID: &str =
    "$builtin.Intl.DateTimeFormat.supportedLocalesOf";
pub const BUILTIN_INTL_DATE_TIME_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID: &str =
    "$builtin.Intl.DateTimeFormat.prototype.resolvedOptions";
pub const BUILTIN_INTL_DATE_TIME_FORMAT_PROTOTYPE_FORMAT_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.DateTimeFormat.prototype.format.get";
pub const BUILTIN_INTL_DATE_TIME_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID: &str =
    "$builtin.Intl.DateTimeFormat.prototype.formatToParts";
pub const BUILTIN_INTL_DATE_TIME_FORMAT_PROTOTYPE_FORMAT_RANGE_FUNCTION_ID: &str =
    "$builtin.Intl.DateTimeFormat.prototype.formatRange";
pub const BUILTIN_INTL_DATE_TIME_FORMAT_PROTOTYPE_FORMAT_RANGE_TO_PARTS_FUNCTION_ID: &str =
    "$builtin.Intl.DateTimeFormat.prototype.formatRangeToParts";
pub const BUILTIN_INTL_DATE_TIME_FORMAT_BOUND_FORMAT_FUNCTION_ID: &str =
    "$builtin.Intl.DateTimeFormat.boundFormat";
pub const BUILTIN_INTL_PLURAL_RULES_FUNCTION_ID: &str = "$builtin.Intl.PluralRules";
pub const BUILTIN_INTL_PLURAL_RULES_SUPPORTED_LOCALES_OF_FUNCTION_ID: &str =
    "$builtin.Intl.PluralRules.supportedLocalesOf";
pub const BUILTIN_INTL_PLURAL_RULES_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID: &str =
    "$builtin.Intl.PluralRules.prototype.resolvedOptions";
pub const BUILTIN_INTL_PLURAL_RULES_PROTOTYPE_SELECT_FUNCTION_ID: &str =
    "$builtin.Intl.PluralRules.prototype.select";
pub const BUILTIN_INTL_PLURAL_RULES_PROTOTYPE_SELECT_RANGE_FUNCTION_ID: &str =
    "$builtin.Intl.PluralRules.prototype.selectRange";
pub const BUILTIN_INTL_LIST_FORMAT_FUNCTION_ID: &str = "$builtin.Intl.ListFormat";
pub const BUILTIN_INTL_LIST_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID: &str =
    "$builtin.Intl.ListFormat.supportedLocalesOf";
pub const BUILTIN_INTL_LIST_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID: &str =
    "$builtin.Intl.ListFormat.prototype.resolvedOptions";
pub const BUILTIN_INTL_LIST_FORMAT_PROTOTYPE_FORMAT_FUNCTION_ID: &str =
    "$builtin.Intl.ListFormat.prototype.format";
pub const BUILTIN_INTL_LIST_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID: &str =
    "$builtin.Intl.ListFormat.prototype.formatToParts";
pub const BUILTIN_INTL_NUMBER_FORMAT_FUNCTION_ID: &str = "$builtin.Intl.NumberFormat";
pub const BUILTIN_INTL_NUMBER_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID: &str =
    "$builtin.Intl.NumberFormat.supportedLocalesOf";
pub const BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID: &str =
    "$builtin.Intl.NumberFormat.prototype.resolvedOptions";
pub const BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.NumberFormat.prototype.format.get";
pub const BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID: &str =
    "$builtin.Intl.NumberFormat.prototype.formatToParts";
pub const BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_RANGE_FUNCTION_ID: &str =
    "$builtin.Intl.NumberFormat.prototype.formatRange";
pub const BUILTIN_INTL_NUMBER_FORMAT_PROTOTYPE_FORMAT_RANGE_TO_PARTS_FUNCTION_ID: &str =
    "$builtin.Intl.NumberFormat.prototype.formatRangeToParts";
pub const BUILTIN_INTL_NUMBER_FORMAT_BOUND_FORMAT_FUNCTION_ID: &str =
    "$builtin.Intl.NumberFormat.boundFormat";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_LANGUAGE_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.language.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_SCRIPT_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.script.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_REGION_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.region.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_CALENDAR_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.calendar.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_COLLATION_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.collation.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_FIRST_DAY_OF_WEEK_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.firstDayOfWeek.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_HOUR_CYCLE_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.hourCycle.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_CASE_FIRST_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.caseFirst.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_NUMERIC_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.numeric.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_NUMBERING_SYSTEM_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.numberingSystem.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_VARIANTS_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.variants.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_BASE_NAME_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.baseName.get";
pub const BUILTIN_INTL_LOCALE_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.Intl.Locale.prototype.toString";
pub const BUILTIN_REGEXP_FUNCTION_ID: &str = "$builtin.RegExp";
pub const BUILTIN_REGEXP_SPECIES_GETTER_FUNCTION_ID: &str = "$builtin.RegExp[Symbol.species].get";
pub const BUILTIN_REGEXP_PROTOTYPE_FLAGS_GETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype.flags.get";
pub const BUILTIN_REGEXP_PROTOTYPE_SOURCE_GETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype.source.get";
pub const BUILTIN_REGEXP_PROTOTYPE_HAS_INDICES_GETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype.hasIndices.get";
pub const BUILTIN_REGEXP_PROTOTYPE_GLOBAL_GETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype.global.get";
pub const BUILTIN_REGEXP_PROTOTYPE_IGNORE_CASE_GETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype.ignoreCase.get";
pub const BUILTIN_REGEXP_PROTOTYPE_MULTILINE_GETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype.multiline.get";
pub const BUILTIN_REGEXP_PROTOTYPE_DOT_ALL_GETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype.dotAll.get";
pub const BUILTIN_REGEXP_PROTOTYPE_UNICODE_GETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype.unicode.get";
pub const BUILTIN_REGEXP_PROTOTYPE_UNICODE_SETS_GETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype.unicodeSets.get";
pub const BUILTIN_REGEXP_PROTOTYPE_STICKY_GETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype.sticky.get";
pub const BUILTIN_REGEXP_LEGACY_STATIC_GETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.legacyStatic.get";
pub const BUILTIN_REGEXP_LEGACY_STATIC_SETTER_FUNCTION_ID: &str =
    "$builtin.RegExp.legacyStatic.set";
pub const BUILTIN_REGEXP_PROTOTYPE_EXEC_FUNCTION_ID: &str = "$builtin.RegExp.prototype.exec";
pub const BUILTIN_REGEXP_PROTOTYPE_TEST_FUNCTION_ID: &str = "$builtin.RegExp.prototype.test";
pub const BUILTIN_REGEXP_PROTOTYPE_COMPILE_FUNCTION_ID: &str = "$builtin.RegExp.prototype.compile";
pub const BUILTIN_REGEXP_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype.toString";
pub const BUILTIN_REGEXP_PROTOTYPE_SYMBOL_MATCH_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype[Symbol.match]";
pub const BUILTIN_REGEXP_PROTOTYPE_SYMBOL_MATCH_ALL_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype[Symbol.matchAll]";
pub const BUILTIN_REGEXP_PROTOTYPE_SYMBOL_REPLACE_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype[Symbol.replace]";
pub const BUILTIN_REGEXP_PROTOTYPE_SYMBOL_SEARCH_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype[Symbol.search]";
pub const BUILTIN_REGEXP_PROTOTYPE_SYMBOL_SPLIT_FUNCTION_ID: &str =
    "$builtin.RegExp.prototype[Symbol.split]";
pub const BUILTIN_REGEXP_ESCAPE_FUNCTION_ID: &str = "$builtin.RegExp.escape";
pub const BUILTIN_JSON_PARSE_FUNCTION_ID: &str = "$builtin.JSON.parse";
pub const BUILTIN_JSON_STRINGIFY_FUNCTION_ID: &str = "$builtin.JSON.stringify";
pub const BUILTIN_JSON_RAW_JSON_FUNCTION_ID: &str = "$builtin.JSON.rawJSON";
pub const BUILTIN_JSON_IS_RAW_JSON_FUNCTION_ID: &str = "$builtin.JSON.isRawJSON";
pub const BUILTIN_ATOMICS_ADD_FUNCTION_ID: &str = "$builtin.Atomics.add";
pub const BUILTIN_ATOMICS_AND_FUNCTION_ID: &str = "$builtin.Atomics.and";
pub const BUILTIN_ATOMICS_COMPARE_EXCHANGE_FUNCTION_ID: &str = "$builtin.Atomics.compareExchange";
pub const BUILTIN_ATOMICS_EXCHANGE_FUNCTION_ID: &str = "$builtin.Atomics.exchange";
pub const BUILTIN_ATOMICS_LOAD_FUNCTION_ID: &str = "$builtin.Atomics.load";
pub const BUILTIN_ATOMICS_NOTIFY_FUNCTION_ID: &str = "$builtin.Atomics.notify";
pub const BUILTIN_ATOMICS_OR_FUNCTION_ID: &str = "$builtin.Atomics.or";
pub const BUILTIN_ATOMICS_PAUSE_FUNCTION_ID: &str = "$builtin.Atomics.pause";
pub const BUILTIN_ATOMICS_STORE_FUNCTION_ID: &str = "$builtin.Atomics.store";
pub const BUILTIN_ATOMICS_SUB_FUNCTION_ID: &str = "$builtin.Atomics.sub";
pub const BUILTIN_ATOMICS_WAIT_FUNCTION_ID: &str = "$builtin.Atomics.wait";
pub const BUILTIN_ATOMICS_WAIT_ASYNC_FUNCTION_ID: &str = "$builtin.Atomics.waitAsync";
pub const BUILTIN_ATOMICS_XOR_FUNCTION_ID: &str = "$builtin.Atomics.xor";
pub const BUILTIN_ATOMICS_IS_LOCK_FREE_FUNCTION_ID: &str = "$builtin.Atomics.isLockFree";
pub const BUILTIN_FLOAT64_ARRAY_FUNCTION_ID: &str = "$builtin.Float64Array";
pub const BUILTIN_FLOAT32_ARRAY_FUNCTION_ID: &str = "$builtin.Float32Array";
pub const BUILTIN_FLOAT16_ARRAY_FUNCTION_ID: &str = "$builtin.Float16Array";
pub const BUILTIN_INT32_ARRAY_FUNCTION_ID: &str = "$builtin.Int32Array";
pub const BUILTIN_INT16_ARRAY_FUNCTION_ID: &str = "$builtin.Int16Array";
pub const BUILTIN_INT8_ARRAY_FUNCTION_ID: &str = "$builtin.Int8Array";
pub const BUILTIN_UINT32_ARRAY_FUNCTION_ID: &str = "$builtin.Uint32Array";
pub const BUILTIN_UINT16_ARRAY_FUNCTION_ID: &str = "$builtin.Uint16Array";
pub const BUILTIN_UINT8_ARRAY_FUNCTION_ID: &str = "$builtin.Uint8Array";
pub const BUILTIN_UINT8_ARRAY_FROM_BASE64_FUNCTION_ID: &str = "$builtin.Uint8Array.fromBase64";
pub const BUILTIN_UINT8_ARRAY_FROM_HEX_FUNCTION_ID: &str = "$builtin.Uint8Array.fromHex";
pub const BUILTIN_UINT8_ARRAY_PROTOTYPE_SET_FROM_BASE64_FUNCTION_ID: &str =
    "$builtin.Uint8Array.prototype.setFromBase64";
pub const BUILTIN_UINT8_ARRAY_PROTOTYPE_SET_FROM_HEX_FUNCTION_ID: &str =
    "$builtin.Uint8Array.prototype.setFromHex";
pub const BUILTIN_UINT8_ARRAY_PROTOTYPE_TO_BASE64_FUNCTION_ID: &str =
    "$builtin.Uint8Array.prototype.toBase64";
pub const BUILTIN_UINT8_ARRAY_PROTOTYPE_TO_HEX_FUNCTION_ID: &str =
    "$builtin.Uint8Array.prototype.toHex";
pub const BUILTIN_UINT8_CLAMPED_ARRAY_FUNCTION_ID: &str = "$builtin.Uint8ClampedArray";
pub const BUILTIN_BIGINT64_ARRAY_FUNCTION_ID: &str = "$builtin.BigInt64Array";
pub const BUILTIN_BIGUINT64_ARRAY_FUNCTION_ID: &str = "$builtin.BigUint64Array";
pub const BUILTIN_BIGINT_FUNCTION_ID: &str = "$builtin.BigInt";
pub const BUILTIN_BIGINT_AS_INT_N_FUNCTION_ID: &str = "$builtin.BigInt.asIntN";
pub const BUILTIN_BIGINT_AS_UINT_N_FUNCTION_ID: &str = "$builtin.BigInt.asUintN";
pub const BUILTIN_BIGINT_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.BigInt.prototype.toString";
pub const BUILTIN_BIGINT_PROTOTYPE_TO_LOCALE_STRING_FUNCTION_ID: &str =
    "$builtin.BigInt.prototype.toLocaleString";
pub const BUILTIN_BIGINT_PROTOTYPE_VALUE_OF_FUNCTION_ID: &str = "$builtin.BigInt.prototype.valueOf";
pub const BUILTIN_SYMBOL_FUNCTION_ID: &str = "$builtin.Symbol";
pub const BUILTIN_SYMBOL_FOR_FUNCTION_ID: &str = "$builtin.Symbol.for";
pub const BUILTIN_SYMBOL_KEY_FOR_FUNCTION_ID: &str = "$builtin.Symbol.keyFor";
pub const BUILTIN_NUMBER_FUNCTION_ID: &str = "$builtin.Number";
pub const BUILTIN_NUMBER_IS_INTEGER_FUNCTION_ID: &str = "$builtin.Number.isInteger";
pub const BUILTIN_STRING_FUNCTION_ID: &str = "$builtin.String";
pub const BUILTIN_STRING_FROM_CHAR_CODE_FUNCTION_ID: &str = "$builtin.String.fromCharCode";
pub const BUILTIN_STRING_FROM_CODE_POINT_FUNCTION_ID: &str = "$builtin.String.fromCodePoint";
pub const BUILTIN_STRING_RAW_FUNCTION_ID: &str = "$builtin.String.raw";
pub const BUILTIN_STRING_PROTOTYPE_TO_STRING_FUNCTION_ID: &str =
    "$builtin.String.prototype.toString";
pub const BUILTIN_STRING_PROTOTYPE_VALUE_OF_FUNCTION_ID: &str = "$builtin.String.prototype.valueOf";
pub const BUILTIN_STRING_PROTOTYPE_CHAR_AT_FUNCTION_ID: &str = "$builtin.String.prototype.charAt";
pub const BUILTIN_STRING_PROTOTYPE_CONCAT_FUNCTION_ID: &str = "$builtin.String.prototype.concat";
pub const BUILTIN_STRING_PROTOTYPE_CHAR_CODE_AT_FUNCTION_ID: &str =
    "$builtin.String.prototype.charCodeAt";
pub const BUILTIN_STRING_PROTOTYPE_CODE_POINT_AT_FUNCTION_ID: &str =
    "$builtin.String.prototype.codePointAt";
pub const BUILTIN_STRING_PROTOTYPE_AT_FUNCTION_ID: &str = "$builtin.String.prototype.at";
pub const BUILTIN_STRING_PROTOTYPE_ANCHOR_FUNCTION_ID: &str = "$builtin.String.prototype.anchor";
pub const BUILTIN_STRING_PROTOTYPE_BIG_FUNCTION_ID: &str = "$builtin.String.prototype.big";
pub const BUILTIN_STRING_PROTOTYPE_BLINK_FUNCTION_ID: &str = "$builtin.String.prototype.blink";
pub const BUILTIN_STRING_PROTOTYPE_BOLD_FUNCTION_ID: &str = "$builtin.String.prototype.bold";
pub const BUILTIN_STRING_PROTOTYPE_FIXED_FUNCTION_ID: &str = "$builtin.String.prototype.fixed";
pub const BUILTIN_STRING_PROTOTYPE_FONTCOLOR_FUNCTION_ID: &str =
    "$builtin.String.prototype.fontcolor";
pub const BUILTIN_STRING_PROTOTYPE_FONTSIZE_FUNCTION_ID: &str =
    "$builtin.String.prototype.fontsize";
pub const BUILTIN_STRING_PROTOTYPE_ITALICS_FUNCTION_ID: &str = "$builtin.String.prototype.italics";
pub const BUILTIN_STRING_PROTOTYPE_LINK_FUNCTION_ID: &str = "$builtin.String.prototype.link";
pub const BUILTIN_STRING_PROTOTYPE_SMALL_FUNCTION_ID: &str = "$builtin.String.prototype.small";
pub const BUILTIN_STRING_PROTOTYPE_STRIKE_FUNCTION_ID: &str = "$builtin.String.prototype.strike";
pub const BUILTIN_STRING_PROTOTYPE_SUB_FUNCTION_ID: &str = "$builtin.String.prototype.sub";
pub const BUILTIN_STRING_PROTOTYPE_SUBSTR_FUNCTION_ID: &str = "$builtin.String.prototype.substr";
pub const BUILTIN_STRING_PROTOTYPE_SUBSTRING_FUNCTION_ID: &str =
    "$builtin.String.prototype.substring";
pub const BUILTIN_STRING_PROTOTYPE_SUP_FUNCTION_ID: &str = "$builtin.String.prototype.sup";
pub const BUILTIN_STRING_PROTOTYPE_MATCH_FUNCTION_ID: &str = "$builtin.String.prototype.match";
pub const BUILTIN_STRING_PROTOTYPE_MATCH_ALL_FUNCTION_ID: &str =
    "$builtin.String.prototype.matchAll";
pub const BUILTIN_STRING_PROTOTYPE_REPLACE_FUNCTION_ID: &str = "$builtin.String.prototype.replace";
pub const BUILTIN_STRING_PROTOTYPE_REPLACE_ALL_FUNCTION_ID: &str =
    "$builtin.String.prototype.replaceAll";
pub const BUILTIN_STRING_PROTOTYPE_SEARCH_FUNCTION_ID: &str = "$builtin.String.prototype.search";
pub const BUILTIN_STRING_PROTOTYPE_INDEX_OF_FUNCTION_ID: &str = "$builtin.String.prototype.indexOf";
pub const BUILTIN_STRING_PROTOTYPE_LAST_INDEX_OF_FUNCTION_ID: &str =
    "$builtin.String.prototype.lastIndexOf";
pub const BUILTIN_STRING_PROTOTYPE_SLICE_FUNCTION_ID: &str = "$builtin.String.prototype.slice";
pub const BUILTIN_STRING_PROTOTYPE_SPLIT_FUNCTION_ID: &str = "$builtin.String.prototype.split";
pub const BUILTIN_STRING_PROTOTYPE_PAD_START_FUNCTION_ID: &str =
    "$builtin.String.prototype.padStart";
pub const BUILTIN_STRING_PROTOTYPE_PAD_END_FUNCTION_ID: &str = "$builtin.String.prototype.padEnd";
pub const BUILTIN_STRING_PROTOTYPE_REPEAT_FUNCTION_ID: &str = "$builtin.String.prototype.repeat";
pub const BUILTIN_STRING_PROTOTYPE_ENDS_WITH_FUNCTION_ID: &str =
    "$builtin.String.prototype.endsWith";
pub const BUILTIN_STRING_PROTOTYPE_INCLUDES_FUNCTION_ID: &str =
    "$builtin.String.prototype.includes";
pub const BUILTIN_STRING_PROTOTYPE_STARTS_WITH_FUNCTION_ID: &str =
    "$builtin.String.prototype.startsWith";
pub const BUILTIN_STRING_PROTOTYPE_NORMALIZE_FUNCTION_ID: &str =
    "$builtin.String.prototype.normalize";
pub const BUILTIN_STRING_PROTOTYPE_LOCALE_COMPARE_FUNCTION_ID: &str =
    "$builtin.String.prototype.localeCompare";
pub const BUILTIN_STRING_PROTOTYPE_ITERATOR_FUNCTION_ID: &str =
    "$builtin.String.prototype[Symbol.iterator]";
pub const BUILTIN_STRING_PROTOTYPE_TO_UPPER_CASE_FUNCTION_ID: &str =
    "$builtin.String.prototype.toUpperCase";
pub const BUILTIN_STRING_PROTOTYPE_TO_LOWER_CASE_FUNCTION_ID: &str =
    "$builtin.String.prototype.toLowerCase";
pub const BUILTIN_STRING_PROTOTYPE_TO_LOCALE_LOWER_CASE_FUNCTION_ID: &str =
    "$builtin.String.prototype.toLocaleLowerCase";
pub const BUILTIN_STRING_PROTOTYPE_TO_LOCALE_UPPER_CASE_FUNCTION_ID: &str =
    "$builtin.String.prototype.toLocaleUpperCase";
pub const BUILTIN_STRING_PROTOTYPE_TRIM_FUNCTION_ID: &str = "$builtin.String.prototype.trim";
pub const BUILTIN_STRING_PROTOTYPE_TRIM_START_FUNCTION_ID: &str =
    "$builtin.String.prototype.trimStart";
pub const BUILTIN_STRING_PROTOTYPE_TRIM_END_FUNCTION_ID: &str = "$builtin.String.prototype.trimEnd";
pub const BUILTIN_STRING_PROTOTYPE_IS_WELL_FORMED_FUNCTION_ID: &str =
    "$builtin.String.prototype.isWellFormed";
pub const BUILTIN_STRING_PROTOTYPE_TO_WELL_FORMED_FUNCTION_ID: &str =
    "$builtin.String.prototype.toWellFormed";
pub const BUILTIN_BOOLEAN_FUNCTION_ID: &str = "$builtin.Boolean";
pub const BUILTIN_PROMISE_FUNCTION_ID: &str = "$builtin.Promise";
pub const BUILTIN_PROMISE_PROTOTYPE_THEN_FUNCTION_ID: &str = "$builtin.Promise.prototype.then";
pub const BUILTIN_PROMISE_PROTOTYPE_CATCH_FUNCTION_ID: &str = "$builtin.Promise.prototype.catch";
pub const BUILTIN_PROMISE_PROTOTYPE_FINALLY_FUNCTION_ID: &str =
    "$builtin.Promise.prototype.finally";
pub const BUILTIN_PROMISE_THEN_FINALLY_FUNCTION_ID: &str = "$builtin.PromiseThenFinally";
pub const BUILTIN_PROMISE_CATCH_FINALLY_FUNCTION_ID: &str = "$builtin.PromiseCatchFinally";
pub const BUILTIN_PROMISE_VALUE_THUNK_FUNCTION_ID: &str = "$builtin.PromiseValueThunk";
pub const BUILTIN_PROMISE_THROWER_FUNCTION_ID: &str = "$builtin.PromiseThrower";
pub const BUILTIN_PROMISE_SPECIES_GETTER_FUNCTION_ID: &str = "$builtin.Promise[Symbol.species].get";
pub const BUILTIN_PROMISE_STATIC_RESOLVE_FUNCTION_ID: &str = "$builtin.Promise.resolve";
pub const BUILTIN_PROMISE_STATIC_WITH_RESOLVERS_FUNCTION_ID: &str =
    "$builtin.Promise.withResolvers";
pub const BUILTIN_PROMISE_STATIC_TRY_FUNCTION_ID: &str = "$builtin.Promise.try";
pub const BUILTIN_PROMISE_STATIC_REJECT_FUNCTION_ID: &str = "$builtin.Promise.reject";
pub const BUILTIN_PROMISE_STATIC_ALL_FUNCTION_ID: &str = "$builtin.Promise.all";
pub const BUILTIN_PROMISE_STATIC_ALL_SETTLED_FUNCTION_ID: &str = "$builtin.Promise.allSettled";
pub const BUILTIN_PROMISE_STATIC_ALL_KEYED_FUNCTION_ID: &str = "$builtin.Promise.allKeyed";
pub const BUILTIN_PROMISE_STATIC_ALL_SETTLED_KEYED_FUNCTION_ID: &str =
    "$builtin.Promise.allSettledKeyed";
pub const BUILTIN_PROMISE_STATIC_ANY_FUNCTION_ID: &str = "$builtin.Promise.any";
pub const BUILTIN_PROMISE_STATIC_RACE_FUNCTION_ID: &str = "$builtin.Promise.race";
pub const BUILTIN_PROMISE_ALL_RESOLVE_ELEMENT_FUNCTION_ID: &str =
    "$builtin.PromiseAllResolveElement";
pub const BUILTIN_PROMISE_ALL_SETTLED_RESOLVE_ELEMENT_FUNCTION_ID: &str =
    "$builtin.PromiseAllSettledResolveElement";
pub const BUILTIN_PROMISE_ALL_SETTLED_REJECT_ELEMENT_FUNCTION_ID: &str =
    "$builtin.PromiseAllSettledRejectElement";
pub const BUILTIN_PROMISE_ANY_REJECT_ELEMENT_FUNCTION_ID: &str = "$builtin.PromiseAnyRejectElement";
pub const BUILTIN_PROMISE_ALL_KEYED_RESOLVE_ELEMENT_FUNCTION_ID: &str =
    "$builtin.PromiseAllKeyedResolveElement";
pub const BUILTIN_PROMISE_ALL_SETTLED_KEYED_RESOLVE_ELEMENT_FUNCTION_ID: &str =
    "$builtin.PromiseAllSettledKeyedResolveElement";
pub const BUILTIN_PROMISE_ALL_SETTLED_KEYED_REJECT_ELEMENT_FUNCTION_ID: &str =
    "$builtin.PromiseAllSettledKeyedRejectElement";
pub const BUILTIN_PROMISE_CAPABILITY_EXECUTOR_FUNCTION_ID: &str =
    "$builtin.PromiseCapabilityExecutor";
pub const BUILTIN_PROMISE_RESOLVE_FUNCTION_ID: &str = "$builtin.PromiseResolveFunction";
pub const BUILTIN_PROMISE_REJECT_FUNCTION_ID: &str = "$builtin.PromiseRejectFunction";
pub const BUILTIN_MAP_FUNCTION_ID: &str = "$builtin.Map";
pub const BUILTIN_MAP_SPECIES_GETTER_FUNCTION_ID: &str = "$builtin.Map[Symbol.species].get";
pub const BUILTIN_MAP_GROUP_BY_FUNCTION_ID: &str = "$builtin.Map.groupBy";
pub const BUILTIN_MAP_PROTOTYPE_CLEAR_FUNCTION_ID: &str = "$builtin.Map.prototype.clear";
pub const BUILTIN_MAP_PROTOTYPE_DELETE_FUNCTION_ID: &str = "$builtin.Map.prototype.delete";
pub const BUILTIN_MAP_PROTOTYPE_FOR_EACH_FUNCTION_ID: &str = "$builtin.Map.prototype.forEach";
pub const BUILTIN_MAP_PROTOTYPE_KEYS_FUNCTION_ID: &str = "$builtin.Map.prototype.keys";
pub const BUILTIN_MAP_PROTOTYPE_VALUES_FUNCTION_ID: &str = "$builtin.Map.prototype.values";
pub const BUILTIN_MAP_PROTOTYPE_ENTRIES_FUNCTION_ID: &str = "$builtin.Map.prototype.entries";
pub const BUILTIN_MAP_ITERATOR_NEXT_FUNCTION_ID: &str = "$builtin.MapIterator.next";
pub const BUILTIN_MAP_PROTOTYPE_GET_FUNCTION_ID: &str = "$builtin.Map.prototype.get";
pub const BUILTIN_MAP_PROTOTYPE_GET_OR_INSERT_FUNCTION_ID: &str =
    "$builtin.Map.prototype.getOrInsert";
pub const BUILTIN_MAP_PROTOTYPE_GET_OR_INSERT_COMPUTED_FUNCTION_ID: &str =
    "$builtin.Map.prototype.getOrInsertComputed";
pub const BUILTIN_MAP_PROTOTYPE_HAS_FUNCTION_ID: &str = "$builtin.Map.prototype.has";
pub const BUILTIN_MAP_PROTOTYPE_SET_FUNCTION_ID: &str = "$builtin.Map.prototype.set";
pub const BUILTIN_MAP_PROTOTYPE_SIZE_GETTER_FUNCTION_ID: &str = "$builtin.Map.prototype.size";
pub const BUILTIN_WEAK_MAP_FUNCTION_ID: &str = "$builtin.WeakMap";
pub const BUILTIN_WEAK_MAP_PROTOTYPE_DELETE_FUNCTION_ID: &str = "$builtin.WeakMap.prototype.delete";
pub const BUILTIN_WEAK_MAP_PROTOTYPE_GET_FUNCTION_ID: &str = "$builtin.WeakMap.prototype.get";
pub const BUILTIN_WEAK_MAP_PROTOTYPE_GET_OR_INSERT_FUNCTION_ID: &str =
    "$builtin.WeakMap.prototype.getOrInsert";
pub const BUILTIN_WEAK_MAP_PROTOTYPE_GET_OR_INSERT_COMPUTED_FUNCTION_ID: &str =
    "$builtin.WeakMap.prototype.getOrInsertComputed";
pub const BUILTIN_WEAK_MAP_PROTOTYPE_HAS_FUNCTION_ID: &str = "$builtin.WeakMap.prototype.has";
pub const BUILTIN_WEAK_MAP_PROTOTYPE_SET_FUNCTION_ID: &str = "$builtin.WeakMap.prototype.set";
pub const BUILTIN_WEAK_SET_FUNCTION_ID: &str = "$builtin.WeakSet";
pub const BUILTIN_WEAK_SET_PROTOTYPE_ADD_FUNCTION_ID: &str = "$builtin.WeakSet.prototype.add";
pub const BUILTIN_WEAK_SET_PROTOTYPE_DELETE_FUNCTION_ID: &str = "$builtin.WeakSet.prototype.delete";
pub const BUILTIN_WEAK_SET_PROTOTYPE_HAS_FUNCTION_ID: &str = "$builtin.WeakSet.prototype.has";
pub const BUILTIN_WEAK_REF_FUNCTION_ID: &str = "$builtin.WeakRef";
pub const BUILTIN_WEAK_REF_PROTOTYPE_DEREF_FUNCTION_ID: &str = "$builtin.WeakRef.prototype.deref";
pub const BUILTIN_FINALIZATION_REGISTRY_FUNCTION_ID: &str = "$builtin.FinalizationRegistry";
pub const BUILTIN_FINALIZATION_REGISTRY_PROTOTYPE_REGISTER_FUNCTION_ID: &str =
    "$builtin.FinalizationRegistry.prototype.register";
pub const BUILTIN_FINALIZATION_REGISTRY_PROTOTYPE_UNREGISTER_FUNCTION_ID: &str =
    "$builtin.FinalizationRegistry.prototype.unregister";
pub const BUILTIN_ASYNC_DISPOSABLE_STACK_FUNCTION_ID: &str = "$builtin.AsyncDisposableStack";
pub const BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_USE_FUNCTION_ID: &str =
    "$builtin.AsyncDisposableStack.prototype.use";
pub const BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_ADOPT_FUNCTION_ID: &str =
    "$builtin.AsyncDisposableStack.prototype.adopt";
pub const BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_DEFER_FUNCTION_ID: &str =
    "$builtin.AsyncDisposableStack.prototype.defer";
pub const BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_MOVE_FUNCTION_ID: &str =
    "$builtin.AsyncDisposableStack.prototype.move";
pub const BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_DISPOSE_ASYNC_FUNCTION_ID: &str =
    "$builtin.AsyncDisposableStack.prototype.disposeAsync";
pub const BUILTIN_ASYNC_DISPOSABLE_STACK_PROTOTYPE_DISPOSED_GETTER_FUNCTION_ID: &str =
    "$builtin.AsyncDisposableStack.prototype.disposed.get";
pub const BUILTIN_ASYNC_DISPOSABLE_STACK_DISPOSE_ASYNC_FULFILLED_FUNCTION_ID: &str =
    "$builtin.AsyncDisposableStack.prototype.disposeAsync.fulfilled";
pub const BUILTIN_ASYNC_DISPOSABLE_STACK_DISPOSE_ASYNC_REJECTED_FUNCTION_ID: &str =
    "$builtin.AsyncDisposableStack.prototype.disposeAsync.rejected";
pub const BUILTIN_DISPOSABLE_STACK_FUNCTION_ID: &str = "$builtin.DisposableStack";
pub const BUILTIN_DISPOSABLE_STACK_PROTOTYPE_USE_FUNCTION_ID: &str =
    "$builtin.DisposableStack.prototype.use";
pub const BUILTIN_DISPOSABLE_STACK_PROTOTYPE_ADOPT_FUNCTION_ID: &str =
    "$builtin.DisposableStack.prototype.adopt";
pub const BUILTIN_DISPOSABLE_STACK_PROTOTYPE_DEFER_FUNCTION_ID: &str =
    "$builtin.DisposableStack.prototype.defer";
pub const BUILTIN_DISPOSABLE_STACK_PROTOTYPE_MOVE_FUNCTION_ID: &str =
    "$builtin.DisposableStack.prototype.move";
pub const BUILTIN_DISPOSABLE_STACK_PROTOTYPE_DISPOSE_FUNCTION_ID: &str =
    "$builtin.DisposableStack.prototype.dispose";
pub const BUILTIN_DISPOSABLE_STACK_PROTOTYPE_DISPOSED_GETTER_FUNCTION_ID: &str =
    "$builtin.DisposableStack.prototype.disposed.get";
pub const BUILTIN_SET_FUNCTION_ID: &str = "$builtin.Set";
pub const BUILTIN_SET_SPECIES_GETTER_FUNCTION_ID: &str = "$builtin.Set[Symbol.species].get";
pub const BUILTIN_SET_PROTOTYPE_ADD_FUNCTION_ID: &str = "$builtin.Set.prototype.add";
pub const BUILTIN_SET_PROTOTYPE_CLEAR_FUNCTION_ID: &str = "$builtin.Set.prototype.clear";
pub const BUILTIN_SET_PROTOTYPE_DELETE_FUNCTION_ID: &str = "$builtin.Set.prototype.delete";
pub const BUILTIN_SET_PROTOTYPE_DIFFERENCE_FUNCTION_ID: &str = "$builtin.Set.prototype.difference";
pub const BUILTIN_SET_PROTOTYPE_FOR_EACH_FUNCTION_ID: &str = "$builtin.Set.prototype.forEach";
pub const BUILTIN_SET_PROTOTYPE_INTERSECTION_FUNCTION_ID: &str =
    "$builtin.Set.prototype.intersection";
pub const BUILTIN_SET_PROTOTYPE_IS_DISJOINT_FROM_FUNCTION_ID: &str =
    "$builtin.Set.prototype.isDisjointFrom";
pub const BUILTIN_SET_PROTOTYPE_IS_SUBSET_OF_FUNCTION_ID: &str =
    "$builtin.Set.prototype.isSubsetOf";
pub const BUILTIN_SET_PROTOTYPE_IS_SUPERSET_OF_FUNCTION_ID: &str =
    "$builtin.Set.prototype.isSupersetOf";
pub const BUILTIN_SET_PROTOTYPE_SYMMETRIC_DIFFERENCE_FUNCTION_ID: &str =
    "$builtin.Set.prototype.symmetricDifference";
pub const BUILTIN_SET_PROTOTYPE_UNION_FUNCTION_ID: &str = "$builtin.Set.prototype.union";
pub const BUILTIN_SET_PROTOTYPE_VALUES_FUNCTION_ID: &str = "$builtin.Set.prototype.values";
pub const BUILTIN_SET_PROTOTYPE_ENTRIES_FUNCTION_ID: &str = "$builtin.Set.prototype.entries";
pub const BUILTIN_SET_ITERATOR_NEXT_FUNCTION_ID: &str = "$builtin.SetIterator.next";
pub const BUILTIN_SET_PROTOTYPE_HAS_FUNCTION_ID: &str = "$builtin.Set.prototype.has";
pub const BUILTIN_SET_PROTOTYPE_SIZE_GETTER_FUNCTION_ID: &str = "$builtin.Set.prototype.size";
pub const BUILTIN_ERROR_FUNCTION_ID: &str = "$builtin.Error";
pub const BUILTIN_ERROR_IS_ERROR_FUNCTION_ID: &str = "$builtin.Error.isError";
pub const BUILTIN_EVAL_ERROR_FUNCTION_ID: &str = "$builtin.EvalError";
pub const BUILTIN_AGGREGATE_ERROR_FUNCTION_ID: &str = "$builtin.AggregateError";
pub const BUILTIN_SUPPRESSED_ERROR_FUNCTION_ID: &str = "$builtin.SuppressedError";
pub const BUILTIN_RANGE_ERROR_FUNCTION_ID: &str = "$builtin.RangeError";
pub const BUILTIN_SYNTAX_ERROR_FUNCTION_ID: &str = "$builtin.SyntaxError";
pub const BUILTIN_TYPE_ERROR_FUNCTION_ID: &str = "$builtin.TypeError";
pub const BUILTIN_URI_ERROR_FUNCTION_ID: &str = "$builtin.URIError";
pub const BUILTIN_REFERENCE_ERROR_FUNCTION_ID: &str = "$builtin.ReferenceError";
pub const BUILTIN_ERROR_PROTOTYPE_TO_STRING_FUNCTION_ID: &str = "$builtin.Error.prototype.toString";
pub const BUILTIN_THROW_TYPE_ERROR_FUNCTION_ID: &str = "$builtin.%ThrowTypeError%";
pub const BUILTIN_ESCAPE_FUNCTION_ID: &str = "$builtin.escape";
pub const BUILTIN_UNESCAPE_FUNCTION_ID: &str = "$builtin.unescape";
pub const BUILTIN_ENCODE_URI_FUNCTION_ID: &str = "$builtin.encodeURI";
pub const BUILTIN_ENCODE_URI_COMPONENT_FUNCTION_ID: &str = "$builtin.encodeURIComponent";
pub const BUILTIN_DECODE_URI_FUNCTION_ID: &str = "$builtin.decodeURI";
pub const BUILTIN_DECODE_URI_COMPONENT_FUNCTION_ID: &str = "$builtin.decodeURIComponent";
pub const SCRIPTW_GENERATOR_THROW_SLOT: &str = "$ScriptwGeneratorThrow";

pub const BUILTIN_INTL_COLLATOR_FUNCTION_ID: &str = "$builtin.Intl.Collator";
pub const BUILTIN_INTL_COLLATOR_SUPPORTED_LOCALES_OF_FUNCTION_ID: &str =
    "$builtin.Intl.Collator.supportedLocalesOf";
pub const BUILTIN_INTL_COLLATOR_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID: &str =
    "$builtin.Intl.Collator.prototype.resolvedOptions";
pub const BUILTIN_INTL_COLLATOR_PROTOTYPE_COMPARE_GETTER_FUNCTION_ID: &str =
    "$builtin.Intl.Collator.prototype.compare.get";
pub const BUILTIN_INTL_COLLATOR_BOUND_COMPARE_FUNCTION_ID: &str = "$builtin.Intl.Collator.Compare";

pub const BUILTIN_INTL_DISPLAY_NAMES_FUNCTION_ID: &str = "$builtin.Intl.DisplayNames";
pub const BUILTIN_INTL_DISPLAY_NAMES_SUPPORTED_LOCALES_OF_FUNCTION_ID: &str =
    "$builtin.Intl.DisplayNames.supportedLocalesOf";
pub const BUILTIN_INTL_DISPLAY_NAMES_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID: &str =
    "$builtin.Intl.DisplayNames.prototype.resolvedOptions";
pub const BUILTIN_INTL_DISPLAY_NAMES_PROTOTYPE_OF_FUNCTION_ID: &str =
    "$builtin.Intl.DisplayNames.prototype.of";
pub const BUILTIN_INTL_RELATIVE_TIME_FORMAT_FUNCTION_ID: &str = "$builtin.Intl.RelativeTimeFormat";
pub const BUILTIN_INTL_RELATIVE_TIME_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID: &str =
    "$builtin.Intl.RelativeTimeFormat.supportedLocalesOf";
pub const BUILTIN_INTL_RELATIVE_TIME_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID: &str =
    "$builtin.Intl.RelativeTimeFormat.prototype.resolvedOptions";
pub const BUILTIN_INTL_RELATIVE_TIME_FORMAT_PROTOTYPE_FORMAT_FUNCTION_ID: &str =
    "$builtin.Intl.RelativeTimeFormat.prototype.format";
pub const BUILTIN_INTL_RELATIVE_TIME_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID: &str =
    "$builtin.Intl.RelativeTimeFormat.prototype.formatToParts";

pub const BUILTIN_INTL_SEGMENTER_FUNCTION_ID: &str = "$builtin.Intl.Segmenter";
pub const BUILTIN_INTL_SEGMENTER_SUPPORTED_LOCALES_OF_FUNCTION_ID: &str =
    "$builtin.Intl.Segmenter.supportedLocalesOf";
pub const BUILTIN_INTL_SEGMENTER_PROTOTYPE_SEGMENT_FUNCTION_ID: &str =
    "$builtin.Intl.Segmenter.prototype.segment";
pub const BUILTIN_INTL_SEGMENTER_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID: &str =
    "$builtin.Intl.Segmenter.prototype.resolvedOptions";
pub const BUILTIN_INTL_SEGMENTS_PROTOTYPE_CONTAINING_FUNCTION_ID: &str =
    "$builtin.%IntlSegmentsPrototype%.containing";
pub const BUILTIN_INTL_SEGMENTS_PROTOTYPE_ITERATOR_FUNCTION_ID: &str =
    "$builtin.%IntlSegmentsPrototype%[Symbol.iterator]";
pub const BUILTIN_INTL_SEGMENT_ITERATOR_PROTOTYPE_NEXT_FUNCTION_ID: &str =
    "$builtin.%IntlSegmentIteratorPrototype%.next";

pub const BUILTIN_INTL_DURATION_FORMAT_FUNCTION_ID: &str = "$builtin.Intl.DurationFormat";
pub const BUILTIN_INTL_DURATION_FORMAT_SUPPORTED_LOCALES_OF_FUNCTION_ID: &str =
    "$builtin.Intl.DurationFormat.supportedLocalesOf";
pub const BUILTIN_INTL_DURATION_FORMAT_PROTOTYPE_RESOLVED_OPTIONS_FUNCTION_ID: &str =
    "$builtin.Intl.DurationFormat.prototype.resolvedOptions";
pub const BUILTIN_INTL_DURATION_FORMAT_PROTOTYPE_FORMAT_FUNCTION_ID: &str =
    "$builtin.Intl.DurationFormat.prototype.format";
pub const BUILTIN_INTL_DURATION_FORMAT_PROTOTYPE_FORMAT_TO_PARTS_FUNCTION_ID: &str =
    "$builtin.Intl.DurationFormat.prototype.formatToParts";

#[cfg(test)]
mod private_binding_name_tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};

    #[test]
    fn private_binding_names_are_not_identifier_names() {
        let minted = [
            PrivateBindingName::temporary("forof", 0).into_storage_name(),
            PrivateBindingName::temporary("forin", 12).into_storage_name(),
            PrivateBindingName::temporary("lex", 3).into_storage_name(),
            PrivateBindingName::temporary("object.spread.", 4).into_storage_name(),
        ];
        assert_eq!(minted[0], "$forof.0");
        assert_eq!(minted[3], "$object.spread.4");
        let fixed = [
            LEXICAL_THIS_NAME,
            LEXICAL_ARGUMENTS_NAME,
            LEXICAL_NEW_TARGET_NAME,
            LEXICAL_HOME_OBJECT_NAME,
            DERIVED_ACTIVATION_THIS_NAME,
            DERIVED_ACTIVATION_THIS_STATUS_NAME,
            DERIVED_ACTIVATION_NEW_TARGET_NAME,
            DERIVED_ACTIVATION_FUNCTION_NAME,
            AUTO_ACCESSOR_VALUE_NAME,
        ];
        for name in fixed.iter().copied().chain(minted.iter().map(String::as_str)) {
            assert!(name.contains(PRIVATE_BINDING_NAME_MARK), "{name}");
            for options in [ParseOptions::script(), ParseOptions::module()] {
                assert!(
                    parse(&format!("var {name};"), options).is_err(),
                    "{name} parses as a binding"
                );
            }
        }
    }
}
