//! Consolidated integration tests. Intl and Temporal: namespace plans, host imports, calendar and
//! time-zone data, method-ownership guards.
//!
//! One binary per area: every integration-test binary links the emitter and the ICU data, so
//! a few area targets cost far less disk and link time than one target per file. Each module
//! is one former test file; run just one with `-- <module>::`.

#[path = "../fixtures/linked_bodies.rs"]
mod linked_bodies;

mod intl_canonical_locale_tag_invocation_structure;
mod intl_dtf_time_zone_name_style_privacy_structure;
mod intl_host_imports;
mod intl_namespace_plan_structure;
mod intl_supported_values_structure;
mod locale_information_list_emission;
mod system_time_zone_imports;
mod temporal_calendar_canonicalization_context_structure;
mod temporal_calendar_shared_size;
mod temporal_conversion_overflow_options_structure;
mod temporal_date_field_read_mode_structure;
mod temporal_duration_arithmetic_operation_structure;
mod temporal_duration_field_transform_structure;
mod temporal_duration_number_fields_structure;
mod temporal_instant_diagnostic_privacy_structure;
mod temporal_instant_epoch_proof_structure;
mod temporal_instant_methods_structure;
mod temporal_namespace_plan_structure;
mod temporal_plain_arithmetic_operation_structure;
mod temporal_plain_date_time_component_structure;
mod temporal_plain_date_time_field_read_mode_structure;
mod temporal_plain_difference_operation_structure;
mod temporal_plain_month_day_parsed_year_privacy_structure;
mod temporal_plain_time_field_authority_structure;
mod temporal_plain_year_month_field_read_mode_structure;
mod temporal_unit_option_property_domain_structure;
mod temporal_zoned_date_time_calendar_coercion_structure;
mod temporal_zoned_date_time_difference_defaults_structure;
mod temporal_zoned_date_time_dispatch_structure;
