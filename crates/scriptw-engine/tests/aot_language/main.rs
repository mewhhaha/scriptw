//! Consolidated integration tests: core language semantics (control flow, classes, destructuring, references, with, ...).
//!
//! One binary per area bounds the number of linked Wasmtime test executables
//! while keeping per-process memory bounded.

mod aot_arguments_concat;
mod aot_arguments_index_descriptors;
mod aot_arguments_iteration;
mod aot_arguments_iterator_mutation;
mod aot_boxed_string_environment_writes;
mod aot_callable_capture_lifecycle;
mod aot_callable_entry_roles;
mod aot_catch_pattern_owners;
mod aot_class_field_named_evaluation;
mod aot_class_initializer_grammar_context;
mod aot_class_name_source;
mod aot_compact_array_literals;
mod aot_compact_native_metadata;
mod aot_compound_addition_flow;
mod aot_compound_assignment_saved_value;
mod aot_computed_numeric_method_calls;
mod aot_constant_number_condition;
mod aot_constructor_throw_domain;
mod aot_control_flow;
mod aot_declaration_completion;
mod aot_delete_reference;
mod aot_destructuring_var_binding_order;
mod aot_empty_function;
mod aot_equality_operand_effects;
mod aot_exact_context_budget;
mod aot_for_of_head_completion;
mod aot_in_operand_order;
mod aot_numeric_call_rows;
mod aot_object_binding_single_get;
mod aot_optional_private_calls;
mod aot_packed_array_producers;
mod aot_parameter_environment_lifetime;
mod aot_private_element_extensibility;
mod aot_private_get_value_effects;
mod aot_private_numeric_updates;
mod aot_property_flow_boundaries;
mod aot_public_class_fields;
mod aot_remaining_invocation_references;
mod aot_runtime_error_for_throw;
mod aot_scalar_execution;
mod aot_source_recursion;
mod aot_string_addition;
mod aot_super_assignment_reference;
mod aot_tail_call_environment;
mod aot_thrown_exception_constructor;
mod aot_undefined_metadata_snapshots;
mod aot_unobserved_arguments;
mod aot_utf16_property_keys;
mod aot_with_has_binding;
mod aot_with_var_hoisting;
