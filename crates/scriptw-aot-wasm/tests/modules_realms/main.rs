//! Consolidated integration tests. Module units and packages, created realms, ShadowRealm bootstrap
//! and per-realm error and projection authority.
//!
//! One binary per area: every integration-test binary links the emitter and the ICU data, so
//! a few area targets cost far less disk and link time than one target per file. Each module
//! is one former test file; run just one with `-- <module>::`.

#[path = "../fixtures/linked_bodies.rs"]
mod linked_bodies;

mod async_execution_realm_structure;
mod async_generator_request_promise_realm_structure;
mod compiled_module_package_structure;
mod created_realm_array_prototype_structure;
mod created_realm_data_view_publication_structure;
mod created_realm_host_hooks_structure;
mod created_realm_promise_publication_structure;
mod created_realm_weak_collection_publication_structure;
mod data_view_positive_bounds_realm_structure;
mod function_module_state_structure;
mod indexed_read_realm_structure;
mod iterator_close_error_realm_structure;
mod module_entry_completion;
mod module_unit_once;
mod numeric_conversion_realm_projection_capability_structure;
mod object_read_proxy_realm_structure;
mod object_write_proxy_realm_structure;
mod promise_callback_created_allocation_realm_structure;
mod promise_combinator_algorithm_error_realm_structure;
mod promise_internal_function_realm_context_structure;
mod promise_prototype_receiver_error_realm_structure;
mod promise_resolve_realm_authority_ownership_structure;
mod promise_resolve_realm_context_structure;
mod promise_species_realm_context_structure;
mod promise_try_callback_error_realm_structure;
mod promise_with_resolvers_result_realm_structure;
mod proxy_creation_execution_realm_structure;
mod shadow_realm_bootstrap_size;
mod to_index_error_realm_structure;
