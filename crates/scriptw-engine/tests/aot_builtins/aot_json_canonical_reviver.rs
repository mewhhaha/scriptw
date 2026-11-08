use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, ObservedCompletion, RealmBuilder,
    RunOptions,
};

fn assert_trace(source: &str, expected: &[&str]) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compiler worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{source}");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(30_000),
                    ..RunOptions::default()
                },
            )
            .expect("JSON.parse must execute its canonical emitted parser and reviver walk");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            observed.completion
        );
        assert_eq!(
            observed.output_events,
            expected
                .iter()
                .map(|line| HostOutputEvent::PrintLine((*line).into()))
                .collect::<Vec<_>>(),
            "{source}"
        );
    }
}

#[test]
fn same_tag_sibling_replacement_enters_actual_new_descendants() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/same_tag_replacement.js"),
        &[
            "start:0",
            "inserted:none",
            "0:none",
            "1:none",
            "2:none",
            "array:none",
            "deep:none",
            "fresh:none",
            "last:none",
            "object:none",
            "<root>:none",
            "true:true:3:7",
        ],
    );
}

#[test]
fn inserted_functions_and_arguments_supply_real_callback_holders() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/function_arguments_descendants.js"),
        &[
            "first:0",
            "leaf:none",
            "branch:none",
            "tail:none",
            "function-effect",
            "function-call:called:1",
            "function:none",
            "leaf:none",
            "0:none",
            "0:none",
            "extra:none",
            "arguments:none",
            "<root>:none",
            "true:13:15:6:1",
        ],
    );
}

#[test]
fn collection_entry_snapshots_preserve_live_forward_reads_and_inherited_indexes() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/live_snapshots.js"),
        &[
            "call:kick:number",
            "call:0:number",
            "inherited-one",
            "call:1:number",
            "call:2:number",
            "call:item:object",
            "get-a",
            "call:a:number",
            "call:b:undefined",
            "call:box:object",
            "call:<root>:object",
            "4:7:30:99:false:4",
        ],
    );
}

#[test]
fn source_context_uses_same_value_and_final_duplicate_source_without_static_values() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/source_context_mutation.js"),
        &[
            "change:0",
            "zero:none",
            "same:1e+0",
            "different:none",
            "n:none",
            "nest:none",
            "dup:5",
            "own:6",
            "__proto__:none",
            "<empty>:7",
            "<root>:none",
            "true:false:9:false:true:true",
        ],
    );
}

#[test]
fn ordinary_calls_acquire_callee_once_then_evaluate_all_operands_before_coercion() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/ordinary_operand_order.js"),
        &[
            "receiver",
            "get-parse",
            "input",
            "reviver",
            "ignored",
            "coerce:string",
            "call:value:1:1",
            "call:<root>:2:none",
            "1:1:2",
            "callee",
            "1:2",
        ],
    );
}

#[test]
fn canonical_mutation_and_callback_failures_preserve_arbitrary_throw_identity() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/abrupt_mutation.js"),
        &[
            "visit:first",
            "length",
            "true",
            "own-visit:first",
            "ownKeys",
            "true",
            "callback:b",
            "true:1",
            "input-throw",
            "true",
            "7",
        ],
    );
}

#[test]
fn private_parse_records_do_not_observe_inherited_hooks_for_new_current_keys() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/private_metadata_names.js"),
        &[
            "first:0",
            "keep:1",
            "current-get:z",
            "z:none",
            "object:none",
            "<root>:none",
            "10:true:0",
        ],
    );
}

#[test]
fn injected_proxy_key_snapshot_precedes_reviver_effects_and_reads_fresh_descendants() {
    assert_trace(
        r#"
var events = [], symbol = Symbol('skip'), target = {first: 1, hidden: 2, gone: 3}, proxy;
Object.defineProperty(target, 'hidden', {enumerable: false});
target[symbol] = 9;
proxy = new Proxy(target, {
  ownKeys: function() { events.push('keys'); return ['first', 'hidden', 'gone', symbol]; },
  getOwnPropertyDescriptor: function(object, key) {
    events.push('desc:' + String(key));
    if (key === 'first') { Object.defineProperty(object, 'hidden', {enumerable: true}); delete object.gone; }
    return Reflect.getOwnPropertyDescriptor(object, key);
  },
  get: function(object, key, receiver) { events.push('get:' + String(key)); return Reflect.get(object, key, receiver); }
});
var result = JSON.parse('{"kick":0,"later":null}', function(key, value) {
  events.push('visit:' + key);
  if (key === 'kick') this.later = proxy;
  if (key === 'first' && this === proxy) {
    target.hidden = {fresh: 7}; target.added = 9;
    Object.defineProperty(target, 'hidden', {enumerable: false});
    return value + 1;
  }
  return value;
});
print(result.later === proxy, target.first, target.hidden.fresh, target.added);
print(events.join('|'));
var marker = {}, checks = [];
var abrupt = new Proxy({first: 1, stop: 2, tail: 3}, {
  ownKeys: function() { checks.push('keys'); return ['first', 'stop', 'tail']; },
  getOwnPropertyDescriptor: function(object, key) {
    checks.push('desc:' + key);
    if (key === 'stop') throw marker;
    return Reflect.getOwnPropertyDescriptor(object, key);
  }
});
try {
  JSON.parse('{"kick":0,"later":null}', function(key, value) {
    checks.push('visit:' + key); if (key === 'kick') this.later = abrupt; return value;
  });
  throw 'missing descriptor failure';
} catch (error) { print(error === marker, checks.join('|')); }
"#,
        &[
            "true 2 7 9",
            "visit:kick|keys|desc:first|desc:hidden|desc:gone|get:first|visit:first|get:hidden|visit:fresh|visit:hidden|visit:later|visit:",
            "true visit:kick|keys|desc:first|desc:stop",
        ],
    );
}

// The GC cohort is independent of the preserved canonical trace cohorts above.
fn assert_json_gc_source(source: &str, marker: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{source}");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                CompileOptions {
                    host_surface_policy: scriptw_engine::HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(30_000),
                    ..RunOptions::default()
                },
            )
            .expect("the finite JSON GC cohort uses the compiled Wasm backend");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(scriptw_engine::ObservedJsValue::Number(
                scriptw_engine::ObservedNumber::from_f64(262.0)
            ))
        );
        assert_eq!(
            observed.output_events,
            vec![HostOutputEvent::PrintLine(marker.into())]
        );
    }
}

#[test]
fn gc_parser_reviver_retains_utf16_source_roots_mutations_and_error_realms() {
    assert_json_gc_source(
        include_str!("../fixtures/json_canonical_reviver/gc_utf16_and_realms.js"),
        "json-gc-parse-reviver:ok",
    );
}

#[test]
fn large_arrays_without_callable_revivers_keep_data_descriptors_and_json_grammar() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/large_array_without_reviver.js"),
        &[
            "65537:0:true",
            "true:true:true",
            "true:true:true:true:0",
            "syntax:5",
        ],
    );
}

#[test]
fn sequential_array_metadata_and_repeated_object_keys_keep_original_source_spelling() {
    assert_trace(
        include_str!(
            "../fixtures/json_canonical_reviver/fresh_array_and_duplicate_object_sources.js"
        ),
        &[
            "0:1e+0",
            "1:-0",
            r#"2:"\uD800""#,
            "n:3e0",
            "3:none",
            "<root>:none",
            "4:true:55296:3",
        ],
    );
}

#[test]
fn noncallable_reviver_proxies_are_ignored_without_any_observable_lookup() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/noncallable_reviver_does_not_observe_proxy_traps.js"),
        &["coerce:string", "3:1:null:true:0"],
    );
}

#[test]
fn revoking_callable_revivers_during_input_coercion_preserves_error_precedence() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/revoked_callable_reviver_retains_parse_error_precedence.js"),
        &[
            "coerce:valid",
            "valid:false:true",
            "coerce:invalid",
            "invalid:true:false",
        ],
    );
}

#[test]
fn fresh_array_appends_keep_independent_data_descriptors_and_bypass_prototype_hooks() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/fresh_array_descriptors.js"),
        &["fresh-array-descriptors:ok"],
    );
}

#[test]
fn fresh_array_descriptor_writes_do_not_mutate_private_reviver_value_snapshots() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/fresh_array_reviver_snapshot_aliases.js"),
        &[
            "0:1e+0",
            "1:none",
            "2:-0",
            "0:4e0",
            "3:none",
            "dup:6e0",
            "4:none",
            "<root>:none",
            "values:1:20:true:4:6",
        ],
    );
}

#[test]
fn fresh_array_appends_preserve_nested_utf16_payloads_and_number_bits() {
    assert_trace(
        include_str!("../fixtures/json_canonical_reviver/fresh_array_nested_utf16_and_numbers.js"),
        &["fresh-array-nesting:ok"],
    );
}

#[test]
fn fresh_array_syntax_and_input_coercion_failures_precede_reviver_calls() {
    assert_trace(
        include_str!(
            "../fixtures/json_canonical_reviver/fresh_array_abrupt_parse_precedes_reviver.js"
        ),
        &["fresh-array-abrupt:ok"],
    );
}
