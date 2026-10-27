//! Consolidated integration tests: RegExp compilation and matching.
//!
//! One binary per area bounds the number of linked Wasmtime test executables
//! while keeping per-process memory bounded.

mod aot_regexp_backreference;
mod aot_regexp_backreference_folding;
mod aot_regexp_capture_only_counts;
mod aot_regexp_case_folding;
mod aot_regexp_computed_finite_strings;
mod aot_regexp_computed_unicode_sets;
mod aot_regexp_constructor_and_iterator;
mod aot_regexp_counted_quantifiers;
mod aot_regexp_exact_bound_admission;
mod aot_regexp_finite_source;
mod aot_regexp_forward_whitespace;
mod aot_regexp_instance_property_facts;
mod aot_regexp_iv_class_strings;
mod aot_regexp_legacy_pooled_class;
mod aot_regexp_linked_choices;
mod aot_regexp_lookaround;
mod aot_regexp_lookbehind_anchors;
mod aot_regexp_property_escape;
mod aot_regexp_recompile;
mod aot_regexp_required_choice_runs;
mod aot_regexp_required_empty_replay;
mod aot_regexp_reverse_whitespace;
mod aot_regexp_runtime_gap;
mod aot_regexp_unicode_property_data;
mod aot_regexp_word_boundary;
