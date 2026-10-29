//! Consolidated integration tests: Temporal.
//!
//! One binary per area bounds the number of linked Wasmtime test executables
//! while keeping per-process memory bounded.

mod aot_temporal_buddhist_calendar;
mod aot_temporal_created_realm;
mod aot_temporal_demand_initialization;
mod aot_temporal_duration_relative;
mod aot_temporal_duration_wide_fields;
mod aot_temporal_duration_zoned_relative;
mod aot_temporal_east_asian_calendars;
mod aot_temporal_hebrew_calendar;
mod aot_temporal_indian_calendar;
mod aot_temporal_instant_methods;
mod aot_temporal_islamic_tabular_calendars;
mod aot_temporal_month_code;
mod aot_temporal_named_arithmetic;
mod aot_temporal_named_central;
mod aot_temporal_named_conversions;
mod aot_temporal_named_zdt_leaves;
mod aot_temporal_partial_date_format;
mod aot_temporal_persian_calendar;
mod aot_temporal_plain_date_difference;
mod aot_temporal_plain_date_zoned;
mod aot_temporal_plain_year_month;
mod aot_temporal_relative_bag;
mod aot_temporal_thirteen_month_calendars;
mod aot_temporal_umalqura_calendar;
mod aot_temporal_zone_authority;
mod aot_temporal_zoned_date_time_day;
mod aot_temporal_zoned_date_time_difference;
mod aot_temporal_zoned_date_time_format;
mod aot_temporal_zoned_date_time_round;
mod aot_temporal_zoned_date_time_surface;
mod aot_temporal_zoned_locale;
mod aot_temporal_zoned_with;
