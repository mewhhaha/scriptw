//! Consolidated integration tests. Source and structure guards for async functions, generators,
//! for-await, iterator protocols and resource management (`using`).
//!
//! One binary per area: every integration-test binary links the emitter and the ICU data, so
//! a few area targets cost far less disk and link time than one target per file. Each module
//! is one former test file; run just one with `-- <module>::`.

mod array_iterator_runtime_ownership;
mod async_array_destructuring_structure;
mod async_disposable_stack_dispose_completion_kind_structure;
mod async_for_in_enumeration_structure;
mod async_generator_await_using_structure;
mod async_generator_classic_loop_structure;
mod async_generator_complete_step_kind_structure;
mod async_generator_delegation_kind_structure;
mod async_generator_for_in_structure;
mod async_generator_switch_structure;
mod async_generator_synchronous_using_structure;
mod async_generator_with_structure;
mod async_with_environment_structure;
mod for_await_activation_layout_structure;
mod for_await_iterator_symbol_domain_structure;
mod for_of_array_iterator_protocol_structure;
mod for_of_string_iterator_protocol_structure;
mod generator_array_destructuring_structure;
mod generator_delegate_property_domain_structure;
mod generator_delegate_protocol_error_authority_structure;
mod generator_for_in_enumeration_structure;
mod generator_instance_prototype_structure;
mod generator_with_environment_structure;
mod host_schema;
mod plain_async_classic_for_await_using_structure;
mod plain_async_for_of_await_using_structure;
mod plain_async_function_await_using_structure;
mod plain_async_function_synchronous_using_structure;
mod plain_async_sync_for_of_iterator_record_structure;
mod plain_generator_synchronous_using_structure;
mod resumable_loop_iteration_environment_structure;
mod sync_dispose_completion_continuation_structure;
mod sync_iterator_consumer_capability_structure;
mod sync_iterator_locals_release_ownership_structure;
mod synchronous_using_classic_for_structure;
mod synchronous_using_for_of_structure;
mod synchronous_using_scope_structure;
