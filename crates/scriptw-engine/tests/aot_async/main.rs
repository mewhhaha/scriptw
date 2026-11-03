//! Consolidated integration tests: async functions, await, for-await and async generators.
//!
//! One binary per area bounds the number of linked Wasmtime test executables
//! while keeping per-process memory bounded.

mod aot_async_array_patterns;
mod aot_async_classic_loops;
mod aot_async_disposable_stack_realm;
mod aot_async_for_in;
mod aot_async_for_of;
mod aot_async_for_of_continuations;
mod aot_async_generator_array_lifecycle;
mod aot_async_generator_assignment;
mod aot_async_generator_catch_patterns;
mod aot_async_generator_classic_regions;
mod aot_async_generator_expression_regions;
mod aot_async_generator_for_in_initialization;
mod aot_async_generator_for_in_lifecycle;
mod aot_async_generator_for_in_regions;
mod aot_async_generator_for_of_lifecycle;
mod aot_async_generator_for_of_regions;
mod aot_async_generator_loop_lifecycle;
mod aot_async_generator_pattern_regions;
mod aot_async_generator_resource_lifecycle;
mod aot_async_generator_resource_regions;
mod aot_async_generator_switch_lifecycle;
mod aot_async_generator_switch_regions;
mod aot_async_generator_with_lifecycle;
mod aot_async_generator_with_regions;
mod aot_async_if;
mod aot_async_loop_bindings;
mod aot_async_property_assignment;
mod aot_async_resource_loops;
mod aot_async_super_construct;
mod aot_async_switch_block_disposal;
mod aot_async_switch_continuations;
mod aot_async_switch_operands;
mod aot_async_while_branch_condition;
mod aot_async_while_condition;
mod aot_async_with;
mod aot_await_rejection_realm;
mod aot_captured_for_await;
mod aot_conditional_await_expressions;
mod aot_for_await_rejection_close;
mod aot_grouped_optional_reference_await;
mod aot_host_tasks;
mod aot_logical_assignment_await;
mod aot_logical_await_expressions;
mod aot_optional_call_await;
mod aot_optional_property_await;
