mod array;
mod array_from_async;
mod async_disposable_stack;
pub(crate) use async_disposable_stack::AsyncDisposableStackDisposeCompletionKind;
mod async_iterator;
mod atomics;
mod bigint;
mod binary_data;
mod boolean;
mod bootstrap;
pub(crate) use bootstrap::{
    created_realm_global_bindings, BootstrapRealm, IntlIntrinsicBuild, TemporalIntrinsicBuild,
};
mod collections;
mod data_view_access;
mod date;
mod decimal;
mod disposable_stack;
mod errors;
mod function;
mod runtime_semantics;
mod shadow_realm;
pub(crate) use function::{
    append_empty_dynamic_function_bodies, is_empty_dynamic_function_body,
    is_empty_dynamic_function_id,
};
mod global_numeric;
mod host;
mod intl;
mod intl_collator;
mod intl_datetimeformat;
mod intl_displaynames;
mod intl_durationformat;
mod intl_legacy_constructor;
mod intl_listformat;
mod intl_number;
mod intl_provider_wire;
mod intl_relativetime;
mod intl_segmenter;
pub(crate) use intl_collator::intl_collator_pool_strings;
pub(crate) use intl_displaynames::intl_display_names_pool_strings;
pub(crate) use intl_durationformat::intl_durationformat_pool_strings;
pub(crate) use intl_listformat::intl_list_format_pool_strings;
pub(crate) use intl_relativetime::intl_relative_time_pool_strings;
pub(crate) use intl_segmenter::intl_segmenter_pool_strings;
mod intl_numberformat;
mod intl_pluralrules;
mod intl_supported_values;
pub(crate) use intl_datetimeformat::intl_date_time_format_pool_strings;
pub(crate) use intl_numberformat::intl_number_format_pool_strings;
pub(crate) use intl_pluralrules::intl_plural_rules_pool_strings;
mod iterators;
mod json;
pub(crate) use json::{JsonParseFrameState, JsonReviverFrameState, JsonReviverPropertyRole};
mod math;
mod number;
mod object;
mod promise;
pub(crate) use promise::{
    AsyncExecutionRealmContext, AsyncGeneratorCompleteStepKind, ModuleReactionContinuation,
};
mod proxy;
mod reflect;
mod regexp;
mod standard;
mod string;
mod system_time_zone;
pub(crate) use string::StringNormalizationForm;
mod symbol;
mod temporal;
/// Shared zoned option spellings and diagnostics are also the pool authority.
pub(crate) use temporal::ZonedDateTimeOptionKey;
mod temporal_calendar_arithmetic;
mod temporal_duration;
mod temporal_duration_methods;
mod temporal_duration_relative;
mod temporal_instant;
mod temporal_options;
mod temporal_plain_date;
/// The calendar table, re-exported for `data.rs`: the string pool derives the
/// interned calendar spellings *and* every era spelling by walking
/// `TemporalCalendarId::ALL -> eras() -> spellings()`, which is exactly the
/// table `emit_temporal_resolve_era_to_year` matches an incoming `era`
/// against. `Era::code()` is `Era::spellings()[0]`, so an alias such as `ad`
/// or `bc` added to that table is interned, accepted by
/// `CalendarResolveFields` and excluded from the `era` accessor's answer
/// without a second edit anywhere — and a *calendar* added with a complete
/// `eras()` is interned without an edit here at all.
pub(crate) use temporal_plain_date::TemporalCalendarId;
/// Difference guards select messages from the shared runtime-error catalog.
pub(crate) use temporal_plain_date::TemporalDifferenceGuard;
mod temporal_difference;
mod temporal_plain_date_methods;
mod temporal_plain_date_time;
mod temporal_plain_date_time_methods;
mod temporal_plain_date_zoned;
mod temporal_plain_month_day;
mod temporal_plain_time;
mod temporal_plain_time_methods;
mod temporal_plain_year_month;
mod temporal_plain_year_month_methods;
mod temporal_zone_provider;
mod temporal_zoned_arithmetic;
mod temporal_zoned_date_time_day;
/// `Temporal.ZonedDateTime.prototype.{add,subtract,until,since,withCalendar}`.
///
/// Split from `temporal.rs` on the same boundary
/// `temporal_plain_date_time_methods` is split from
/// `temporal_plain_date_time`: record/constructor/accessors on one side,
/// prototype method bodies on the other. `check-module-boundaries.sh` requires
/// both, so the split cannot silently collapse back.
mod temporal_zoned_date_time_format;
mod temporal_zoned_date_time_methods;
mod temporal_zoned_date_time_round;
mod temporal_zoned_date_time_with;
mod typed_array_fill;
mod typed_array_set;
mod uint8array_base64_decode;
mod uint8array_base64_encode;
mod uint8array_codecs;
mod uint8array_hex;
mod uri;
mod weak;
