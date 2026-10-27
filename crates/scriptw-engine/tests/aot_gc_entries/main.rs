//! Consolidated integration tests: Wasm GC host-entry builtins (aot_gc_*).
//!
//! One binary per area bounds the number of linked Wasmtime test executables
//! while keeping per-process memory bounded.

mod aot_gc_agent_harness_contract;
mod aot_gc_array_from_async_entries;
mod aot_gc_array_internal_descriptors;
mod aot_gc_array_mutation_entries;
mod aot_gc_array_set_data;
mod aot_gc_async_iterator_disposal;
mod aot_gc_binary_data_entries;
mod aot_gc_collection_entries;
mod aot_gc_compact_data_storage;
mod aot_gc_disposable_stack_entries;
mod aot_gc_generator_entries;
mod aot_gc_iterator_chunking;
mod aot_gc_iterator_constructor_entry;
mod aot_gc_iterator_entries;
mod aot_gc_iterator_helper_entries;
mod aot_gc_native_array_entries;
mod aot_gc_native_caller_effects;
mod aot_gc_numeric_values;
mod aot_gc_object_entries;
mod aot_gc_packed_array_storage;
mod aot_gc_prepared_script_entries;
mod aot_gc_property_read_plans;
mod aot_gc_reflect_entries;
mod aot_gc_regexp_legacy_entries;
mod aot_gc_sparse_array_storage;
mod aot_gc_string_regexp_entries;
mod aot_gc_string_unicode_entries;
mod aot_gc_temporal_entries;
mod aot_gc_typed_array_immutable_properties;
mod aot_gc_typed_array_method_entries;
mod aot_gc_uint8array_codec_entries;
mod aot_gc_uri_entries;
mod aot_gc_value_backing_transitions;
