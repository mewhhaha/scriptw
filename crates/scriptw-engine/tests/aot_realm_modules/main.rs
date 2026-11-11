//! Consolidated integration tests: realms, ShadowRealm, eval and dynamic code, prepared scripts, and the module system.
//!
//! One binary per area bounds the number of linked Wasmtime test executables
//! while keeping per-process memory bounded.

mod aot_abstract_module_source;
mod aot_aggregate_error_constructor_realm;
mod aot_array_by_copy_realm;
mod aot_bigint_conversion_error_realm;
mod aot_bytes_modules;
mod aot_created_realm_array_iterator;
mod aot_created_realm_async_disposable_stack;
mod aot_created_realm_disposable_stack;
mod aot_created_realm_dynamic_functions;
mod aot_define_property_realm;
mod aot_direct_eval;
mod aot_direct_eval_call_identity;
mod aot_direct_eval_environment;
mod aot_direct_eval_escaped_arrows;
mod aot_direct_eval_spread;
mod aot_dynamic_source_capability;
mod aot_embedded_module_graph;
mod aot_empty_dynamic_functions;
mod aot_error_stack_accessors;
mod aot_execution_global_environment;
mod aot_finite_function_constructor;
mod aot_finite_indirect_eval;
mod aot_fresh_script_global_lexicals;
mod aot_global_error_caller_effects;
mod aot_has_instance_local_entry_realm;
mod aot_indirect_eval_admission;
mod aot_intrinsic_receiver_observation;
mod aot_json_modules;
mod aot_module_async_lifecycle;
mod aot_module_captured_cells;
mod aot_module_default_export_names;
mod aot_module_entry_completion;
mod aot_module_import_jobs;
mod aot_module_instantiation;
mod aot_module_namespace;
mod aot_module_scope_isolation;
mod aot_native_function_source_syntax;
mod aot_nul_source;
mod aot_ordinary_global_assignment_reference;
mod aot_prepared_dynamic_function;
mod aot_prepared_global_declarations;
mod aot_prepared_script;
mod aot_prepared_script_hosts;
mod aot_proxy_helper_realms;
mod aot_replaced_builtin_methods;
mod aot_replaced_error_to_string;
mod aot_runtime_import_reachability;
mod aot_script_computed_import;
mod aot_script_import_jobs;
mod aot_shadow_realm_construction;
mod aot_shadow_realm_import_value;
mod aot_shadow_realm_prepared_evaluate;
mod aot_shadow_realm_wrapped_callables;
mod aot_source_identity;
mod aot_suppressed_error_constructor_realm;
mod aot_text_modules;
mod aot_throw_type_error_realm;
mod aot_typed_array_constructor_error_realm;

mod aot_cell_call_fast_paths;
