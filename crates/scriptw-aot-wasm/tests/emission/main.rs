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

mod async_generator_scheduling_size;
mod async_labelled_region_emission;
mod class_initializer_function_identity;
mod constant_number_condition_emission;
mod embedding;
mod emit_golden;
mod foreign_functions;
mod host_html_dda_structure;
mod iterator_helper_dispatch;
mod numeric_bitwise_emission;
mod packed_array_storage;
mod product_artifact;
mod proper_tail_call_emission;
mod property_read_plans;
mod string_match_call_emission;
