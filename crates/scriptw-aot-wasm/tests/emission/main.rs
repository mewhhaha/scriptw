//! Consolidated integration tests. Artifact, function-body, size and emission tests over compiled
//! programs, plus the byte-identity golden capture.
//!
//! One binary per area: every integration-test binary links the emitter and the ICU data, so
//! a few area targets cost far less disk and link time than one target per file. Each module
//! is one former test file; run just one with `-- <module>::`.

#[path = "../fixtures/linked_bodies.rs"]
mod linked_bodies;
#[path = "../fixtures/product_programs.rs"]
mod product_programs;

mod aot_guarded_number_update;
mod arguments_elision;
mod async_generator_scheduling_size;
mod async_labelled_region_emission;
mod class_initializer_function_identity;
mod compact_birth_layouts;
mod constant_number_condition_emission;
mod embedding;
mod emit_golden;
mod for_in_prefix_copy;
mod foreign_functions;
mod function_own_data_probe;
mod guarded_push;
mod has_instance_helper;
mod host_html_dda_structure;
mod infallible_booleans;
mod iterator_helper_dispatch;
mod numeric_bitwise_emission;
mod own_keys_helper;
mod own_property_helper;
mod packed_array_storage;
mod product_artifact;
mod program_utf16_sharing;
mod proper_tail_call_emission;
mod property_read_plans;
mod scalar_masked_rows;
mod singleton_comparisons;
mod string_match_call_emission;
mod throw_guard_dedup;
mod typeof_comparisons;

mod regexp_shared_recovery;

mod scalar_dictionary_rows;

mod scalar_row_modes;

mod scalar_row_projection;

mod retained_operands;

mod computed_key_projection;
