//! Complete IR integration inventory; original test bodies and helpers stay in place.

#[path = "annex_b_direct_function_collection_structure.rs"]
mod annex_b_direct_function_collection_structure;
#[path = "apply_argument_provenance_structure.rs"]
mod apply_argument_provenance_structure;
#[path = "array_arguments_primitive.rs"]
mod array_arguments_primitive;
#[path = "async_array_patterns.rs"]
mod async_array_patterns;
#[path = "async_for_in.rs"]
mod async_for_in;
#[path = "async_for_of_activation.rs"]
mod async_for_of_activation;
#[path = "async_for_of_continuations.rs"]
mod async_for_of_continuations;
#[path = "async_generator_catch_patterns.rs"]
mod async_generator_catch_patterns;
#[path = "async_generator_classic_regions.rs"]
mod async_generator_classic_regions;
#[path = "async_generator_expression_regions.rs"]
mod async_generator_expression_regions;
#[path = "async_generator_for_in_regions.rs"]
mod async_generator_for_in_regions;
#[path = "async_generator_for_of_regions.rs"]
mod async_generator_for_of_regions;
#[path = "async_generator_pattern_regions.rs"]
mod async_generator_pattern_regions;
#[path = "async_generator_resource_regions.rs"]
mod async_generator_resource_regions;
#[path = "async_generator_switch_regions.rs"]
mod async_generator_switch_regions;
#[path = "async_generator_with_regions.rs"]
mod async_generator_with_regions;
#[path = "async_if_continuations.rs"]
mod async_if_continuations;
#[path = "async_property_assignment.rs"]
mod async_property_assignment;
#[path = "async_super_construct.rs"]
mod async_super_construct;
#[path = "async_switch_continuations.rs"]
mod async_switch_continuations;
#[path = "async_while_condition.rs"]
mod async_while_condition;
#[path = "async_with.rs"]
mod async_with;
#[path = "attributed_reexport_request_structure.rs"]
mod attributed_reexport_request_structure;
#[path = "bigint_loose_equality.rs"]
mod bigint_loose_equality;
#[path = "borrowed_eval_loop_heads.rs"]
mod borrowed_eval_loop_heads;
#[path = "builtin_getter_receiver_provenance_structure.rs"]
mod builtin_getter_receiver_provenance_structure;
#[path = "bytes_modules.rs"]
mod bytes_modules;
#[path = "catch_pattern_owners.rs"]
mod catch_pattern_owners;
#[path = "class_computed_name_suspension.rs"]
mod class_computed_name_suspension;
#[path = "class_name_source.rs"]
mod class_name_source;
#[path = "complete_module_catalog.rs"]
mod complete_module_catalog;
#[path = "completion_abi_slot_domain_structure.rs"]
mod completion_abi_slot_domain_structure;
#[path = "compound_addition_flow.rs"]
mod compound_addition_flow;
#[path = "compound_assignment_saved_value.rs"]
mod compound_assignment_saved_value;
#[path = "computed_numeric_method_calls.rs"]
mod computed_numeric_method_calls;
#[path = "conditional_await_expressions.rs"]
mod conditional_await_expressions;
#[path = "constructor_throw_domain.rs"]
mod constructor_throw_domain;
#[path = "declaration_completion.rs"]
mod declaration_completion;
#[path = "define_property_array_length.rs"]
mod define_property_array_length;
#[path = "delete_reference.rs"]
mod delete_reference;
#[path = "descriptor_source_text_attribute_selection_structure.rs"]
mod descriptor_source_text_attribute_selection_structure;
#[path = "direct_eval_call_site_structure.rs"]
mod direct_eval_call_site_structure;
#[path = "direct_eval_environment.rs"]
mod direct_eval_environment;
#[path = "direct_eval_spread.rs"]
mod direct_eval_spread;
#[path = "dynamic_component_authority_structure.rs"]
mod dynamic_component_authority_structure;
#[path = "dynamic_import_site_metadata.rs"]
mod dynamic_import_site_metadata;
#[path = "dynamic_import_slash_meaning_structure.rs"]
mod dynamic_import_slash_meaning_structure;
#[path = "dynamic_source_proof_structure.rs"]
mod dynamic_source_proof_structure;
#[path = "empty_function_and_erased_eval.rs"]
mod empty_function_and_erased_eval;
#[path = "environment_kind_structure.rs"]
mod environment_kind_structure;
#[path = "error_instance_prototype_resolution.rs"]
mod error_instance_prototype_resolution;
#[path = "finite_function_constructor.rs"]
mod finite_function_constructor;
#[path = "finite_indirect_eval.rs"]
mod finite_indirect_eval;
#[path = "finite_regexp_source.rs"]
mod finite_regexp_source;
#[path = "float16_array.rs"]
mod float16_array;
#[path = "for_await_identifier_assignment_head_structure.rs"]
mod for_await_identifier_assignment_head_structure;
#[path = "for_in_enumeration.rs"]
mod for_in_enumeration;
#[path = "forwarded_dynamic_source_call.rs"]
mod forwarded_dynamic_source_call;
#[path = "forwarded_dynamic_source_call_structure.rs"]
mod forwarded_dynamic_source_call_structure;
#[path = "function_names.rs"]
mod function_names;
#[path = "function_source_spans.rs"]
mod function_source_spans;
#[path = "generator_array_patterns.rs"]
mod generator_array_patterns;
#[path = "generator_branch_lexicals.rs"]
mod generator_branch_lexicals;
#[path = "generator_for_in.rs"]
mod generator_for_in;
#[path = "generator_invocation_boundaries.rs"]
mod generator_invocation_boundaries;
#[path = "generator_linear_assignment.rs"]
mod generator_linear_assignment;
#[path = "generator_object_patterns.rs"]
mod generator_object_patterns;
#[path = "generator_optional_calls.rs"]
mod generator_optional_calls;
#[path = "generator_optional_regions.rs"]
mod generator_optional_regions;
#[path = "generator_pattern_assignments.rs"]
mod generator_pattern_assignments;
#[path = "generator_pattern_initializers.rs"]
mod generator_pattern_initializers;
#[path = "generator_switch_regions.rs"]
mod generator_switch_regions;
#[path = "generator_throw_regions.rs"]
mod generator_throw_regions;
#[path = "generator_value_branches.rs"]
mod generator_value_branches;
#[path = "generator_with.rs"]
mod generator_with;
#[path = "graph_async_evaluation_structure.rs"]
mod graph_async_evaluation_structure;
#[path = "graph_build_structure.rs"]
mod graph_build_structure;
#[path = "graph_evaluation_classification_structure.rs"]
mod graph_evaluation_classification_structure;
#[path = "graph_evaluation_order_structure.rs"]
mod graph_evaluation_order_structure;
#[path = "graph_resolution_structure.rs"]
mod graph_resolution_structure;
#[path = "graph_tests_structure.rs"]
mod graph_tests_structure;
#[path = "grouped_optional_reference_await.rs"]
mod grouped_optional_reference_await;
#[path = "host_builtin_surface_domain_structure.rs"]
mod host_builtin_surface_domain_structure;
#[path = "import_phase_structure.rs"]
mod import_phase_structure;
#[path = "indirect_eval_admission.rs"]
mod indirect_eval_admission;
#[path = "instantiated_frame_structure.rs"]
mod instantiated_frame_structure;
#[path = "intl_display_names_value_flow.rs"]
mod intl_display_names_value_flow;
#[path = "intl_duration_format_value_flow.rs"]
mod intl_duration_format_value_flow;
#[path = "intl_locale_getters.rs"]
mod intl_locale_getters;
#[path = "intl_locale_information_value_flow.rs"]
mod intl_locale_information_value_flow;
#[path = "intl_segmenter_value_flow.rs"]
mod intl_segmenter_value_flow;
#[path = "intrinsic_method_resolution.rs"]
mod intrinsic_method_resolution;
#[path = "invocation_effects_owner_structure.rs"]
mod invocation_effects_owner_structure;
#[path = "ir_diagnostic_payload_structure.rs"]
mod ir_diagnostic_payload_structure;
#[path = "json_modules.rs"]
mod json_modules;
#[path = "legacy_utf16_pair_structure.rs"]
mod legacy_utf16_pair_structure;
#[path = "link_error_structure.rs"]
mod link_error_structure;
#[path = "loaded_sources_structure.rs"]
mod loaded_sources_structure;
#[path = "logical_assignment_await.rs"]
mod logical_assignment_await;
#[path = "logical_assignment_reachability_structure.rs"]
mod logical_assignment_reachability_structure;
#[path = "logical_await_expressions.rs"]
mod logical_await_expressions;
#[path = "module_captured_cells.rs"]
mod module_captured_cells;
#[path = "module_default_export_names.rs"]
mod module_default_export_names;
#[path = "module_entry_completion.rs"]
mod module_entry_completion;
#[path = "module_evaluation_mode_structure.rs"]
mod module_evaluation_mode_structure;
#[path = "module_import_jobs.rs"]
mod module_import_jobs;
#[path = "module_instantiation.rs"]
mod module_instantiation;
#[path = "module_key_structure.rs"]
mod module_key_structure;
#[path = "module_lowering_gaps.rs"]
mod module_lowering_gaps;
#[path = "module_namespace_construction.rs"]
mod module_namespace_construction;
#[path = "module_unit_structure.rs"]
mod module_unit_structure;
#[path = "number_constructor_coercion.rs"]
mod number_constructor_coercion;
#[path = "number_remainder.rs"]
mod number_remainder;
#[path = "object_binding_algorithm_owner.rs"]
mod object_binding_algorithm_owner;
#[path = "object_constructor_boxing.rs"]
mod object_constructor_boxing;
#[path = "obsolete_lowering_specialization_absence_structure.rs"]
mod obsolete_lowering_specialization_absence_structure;
#[path = "obsolete_static_generator_cache_absence_structure.rs"]
mod obsolete_static_generator_cache_absence_structure;
#[path = "optional_call_await.rs"]
mod optional_call_await;
#[path = "optional_call_source_authority_structure.rs"]
mod optional_call_source_authority_structure;
#[path = "optional_property_await.rs"]
mod optional_property_await;
#[path = "prepared_dynamic_function.rs"]
mod prepared_dynamic_function;
#[path = "prepared_script.rs"]
mod prepared_script;
#[path = "prepared_script_host_globals.rs"]
mod prepared_script_host_globals;
#[path = "primitive_call_evaluation.rs"]
mod primitive_call_evaluation;
#[path = "private_numeric_update.rs"]
mod private_numeric_update;
#[path = "property_update_op_ownership_structure.rs"]
mod property_update_op_ownership_structure;
#[path = "proxy_trap_signature_capability_structure.rs"]
mod proxy_trap_signature_capability_structure;
#[path = "realm_module_catalog.rs"]
mod realm_module_catalog;
#[path = "regexp_backreference_folding.rs"]
mod regexp_backreference_folding;
#[path = "regexp_case_folding.rs"]
mod regexp_case_folding;
#[path = "regexp_legacy_pooled_class.rs"]
mod regexp_legacy_pooled_class;
#[path = "regexp_lookaround.rs"]
mod regexp_lookaround;
#[path = "regexp_lookbehind_anchors.rs"]
mod regexp_lookbehind_anchors;
#[path = "regexp_lookbehind_polarity.rs"]
mod regexp_lookbehind_polarity;
#[path = "regexp_lookbehind_polarity_structure.rs"]
mod regexp_lookbehind_polarity_structure;
#[path = "regexp_pure_epsilon.rs"]
mod regexp_pure_epsilon;
#[path = "regexp_raw_utf16.rs"]
mod regexp_raw_utf16;
#[path = "regexp_reverse_whitespace.rs"]
mod regexp_reverse_whitespace;
#[path = "regexp_unicode_property_aliases.rs"]
mod regexp_unicode_property_aliases;
#[path = "regexp_word_boundary.rs"]
mod regexp_word_boundary;
#[path = "resolved_binding_structure.rs"]
mod resolved_binding_structure;
#[path = "resumable_catch_patterns.rs"]
mod resumable_catch_patterns;
#[path = "span_stable_replacement_error_structure.rs"]
mod span_stable_replacement_error_structure;
#[path = "synchronous_resource_loops.rs"]
mod synchronous_resource_loops;
#[path = "temporal_instant_methods.rs"]
mod temporal_instant_methods;
#[path = "throw_type_error_realm.rs"]
mod throw_type_error_realm;
#[path = "unsupported_dynamic_source_call_structure.rs"]
mod unsupported_dynamic_source_call_structure;
#[path = "with_has_binding.rs"]
mod with_has_binding;
#[path = "with_var_hoisting.rs"]
mod with_var_hoisting;

#[path = "text_modules.rs"]
mod text_modules;
