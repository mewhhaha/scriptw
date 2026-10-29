//! Consolidated integration tests: Intl and locale-sensitive Date behavior.
//!
//! One binary per area bounds the number of linked Wasmtime test executables
//! while keeping per-process memory bounded.

mod aot_date_locale;
mod aot_date_system_time_zone;
mod aot_intl_bound_format_realm;
mod aot_intl_collator;
mod aot_intl_compilation_profile;
mod aot_intl_created_realm;
mod aot_intl_datetime_buddhist;
mod aot_intl_datetime_calendar16;
mod aot_intl_datetime_numbering;
mod aot_intl_datetime_provider;
mod aot_intl_datetime_range_endpoints;
mod aot_intl_demand_initialization;
mod aot_intl_display_names;
mod aot_intl_duration_format;
mod aot_intl_keyword_aliases;
mod aot_intl_legacy_constructor;
mod aot_intl_list_format;
mod aot_intl_locale_constructor;
mod aot_intl_locale_host_import;
mod aot_intl_locale_hour_cycles;
mod aot_intl_locale_information_lists;
mod aot_intl_locale_likely_subtags;
mod aot_intl_locale_numbering_systems;
mod aot_intl_locale_options;
mod aot_intl_locale_text_info;
mod aot_intl_locale_week_info;
mod aot_intl_named_time_zones;
mod aot_intl_numberformat;
mod aot_intl_numbering_tols;
mod aot_intl_plural_rules;
mod aot_intl_relative_time_format;
mod aot_intl_segmenter;
mod aot_intl_supported_values;

fn configure_compilation_jobs() {
    // The whole target runs with one libtest thread. Allow native compilation
    // to use the runner's bounded CPU allocation while retaining serial setup
    // for direct invocations without an explicit budget.
    let requested = match std::env::var("SCRIPTW_JOBS") {
        Ok(value) => value
            .parse::<std::num::NonZeroUsize>()
            .expect("SCRIPTW_JOBS must be a positive integer")
            .get(),
        Err(std::env::VarError::NotPresent) => 1,
        Err(std::env::VarError::NotUnicode(_)) => panic!("SCRIPTW_JOBS must be a positive integer"),
    };
    let available = std::thread::available_parallelism().map_or(1, usize::from);
    scriptw_engine::configure_compilation_jobs(requested.min(available))
        .expect("consistent bounded native compilation budget");
}
