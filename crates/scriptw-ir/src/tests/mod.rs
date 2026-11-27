use super::*;
use crate::lowering::ScriptLowerer;
use scriptw_front::{parse, ParseOptions};

// Whole test items retain their existing tests::name namespace.
// Each source family owns its controls; shared walkers stay test-private.
include!("support.rs");
include!("environment_analysis.rs");
include!("strict_parameter_storage.rs");
include!("entry_and_modules.rs");
include!("call_references.rs");
include!("closed_local_calls.rs");
include!("array_expression.rs");
include!("operators.rs");
include!("destructuring.rs");
include!("conversion_and_globals.rs");
include!("source_identity.rs");
include!("function_entry.rs");
include!("class_initialization.rs");
include!("object_literal.rs");
include!("iterator_from.rs");
include!("generator_expression.rs");
include!("generator_loop.rs");
include!("generator_staged_operands.rs");
include!("generator_eager_values.rs");
include!("generator_object_literal.rs");
include!("async_expression.rs");
include!("async_generator.rs");
include!("async_loop.rs");
include!("async_generator_expression.rs");
include!("async_callable.rs");
include!("async_iteration.rs");
include!("generator_control_flow.rs");
include!("capture_storage.rs");
include!("class_capture.rs");
include!("binding_and_globals.rs");
include!("json.rs");
include!("regexp.rs");
include!("optional_chain.rs");
include!("caller_flow.rs");
include!("array_native_invocation.rs");
include!("string_native_invocation.rs");
include!("native_invocation.rs");
include!("iteration_effects.rs");
include!("prepared_eval.rs");
include!("global_effects.rs");
include!("builtin_entry_guard.rs");
include!("collections.rs");
include!("annex_b.rs");
include!("template_literal.rs");
include!("resource_disposal.rs");

include!("generator_identifier_reference.rs");

include!("inspection.rs");
