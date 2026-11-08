use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, ObservedCompletion, RealmBuilder,
    RunOptions,
};

fn assert_trace(source: &str, expected: &[&str]) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
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
            .expect("JSON preparation must compile and execute through Wasm AOT");
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
                .map(|line| HostOutputEvent::PrintLine((*line).to_owned()))
                .collect::<Vec<_>>(),
            "{source}"
        );
    }
}

#[test]
fn replacer_list_observes_inherited_gets_snapshots_length_and_deduplicates_after_hooks() {
    assert_trace(
        include_str!("../fixtures/json_stringify_preparation/property_list.js"),
        &[
            r#"{"2":2,"x":3,"tail":4}"#,
            "index:0:true,number:string:true,index:1,string:string:true,duplicate:string:true,root:toJSON,value:2,value:x,value:tail",
            r#"{"b":2,"a":1}"#,
            "list:length,length:number,list:0,list:1,list:2,value:b,value:a",
        ],
    );
}

#[test]
fn to_json_get_covers_all_object_kinds_before_replacer_and_boxed_conversion() {
    assert_trace(
        include_str!("../fixtures/json_stringify_preparation/to_json.js"),
        &[
            "array-own:7:get:true,call:true:true,replace:true:true,unbox:number:true",
            "array-inherited:7:get:true,call:true:true,replace:true:true,unbox:number:true",
            "function:7:get:true,call:true:true,replace:true:true,unbox:number:true",
            "arguments:7:get:true,call:true:true,replace:true:true,unbox:number:true",
            r#"{"child":9,"items":[9]}"#,
            "replace:,get:true,call:child:true,replace:child,replace:items,get:true,call:0:true,replace:0",
        ],
    );
}

#[test]
fn preparation_abrupt_paths_preserve_original_values_and_skip_later_hooks() {
    assert_trace(
        include_str!("../fixtures/json_stringify_preparation/abrupt_order.js"),
        &[
            "callable-first:true:toJSON",
            "noncallable-array:true:",
            "index:true:index:0",
            "wrapper:true:index:0,convert:string",
            "toJSON-get:true:get",
            "toJSON-call:true:get,call:true:true",
            "unbox:true:toJSON,replacer,coerce:number",
            "replacer:true:toJSON,replacer",
        ],
    );
}

#[test]
fn enumerable_key_snapshot_preserves_proxy_order_mutations_and_abrupt_identity() {
    assert_trace(
        r#"
var events = [], symbol = Symbol('skip'), target = {first: 1, hidden: 2, later: 3}, proxy;
Object.defineProperty(target, 'hidden', {enumerable: false});
target[symbol] = 9;
Object.defineProperty(target, 'first', {get: function() {
  events.push('getter:' + (this === proxy));
  Object.defineProperty(target, 'hidden', {enumerable: false});
  target.added = 4;
  return 1;
}, enumerable: true, configurable: true});
proxy = new Proxy(target, {
  ownKeys: function() { events.push('keys'); return ['first', 'hidden', 'later', symbol]; },
  getOwnPropertyDescriptor: function(object, key) {
    events.push('desc:' + String(key));
    if (key === 'first') { delete object.later; Object.defineProperty(object, 'hidden', {enumerable: true}); }
    return Reflect.getOwnPropertyDescriptor(object, key);
  },
  get: function(object, key, receiver) { events.push('get:' + String(key)); return Reflect.get(object, key, receiver); }
});
print(JSON.stringify(proxy, function(key, value) { events.push('replace:' + key); return value; }));
print(events.join('|'));
var marker = {}, checks = [];
var abrupt = new Proxy({first: 1, stop: 2, tail: 3}, {
  ownKeys: function() { checks.push('keys'); return ['first', 'stop', 'tail']; },
  getOwnPropertyDescriptor: function(object, key) {
    checks.push('desc:' + key);
    if (key === 'stop') throw marker;
    return Reflect.getOwnPropertyDescriptor(object, key);
  },
  get: function(object, key, receiver) { checks.push('get:' + key); return Reflect.get(object, key, receiver); }
});
try { JSON.stringify(abrupt); throw 'missing descriptor failure'; } catch (error) {
  print(error === marker, checks.join('|'));
}
"#,
        &[
            r#"{"first":1,"hidden":2}"#,
            "get:toJSON|replace:|keys|desc:first|desc:hidden|desc:later|get:first|getter:true|replace:first|get:hidden|replace:hidden",
            "true get:toJSON|keys|desc:first|desc:stop",
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
fn gc_serialization_retains_context_path_lists_callbacks_and_abrupt_values() {
    assert_json_gc_source(
        include_str!("../fixtures/json_stringify_preparation/gc_roots_and_abrupt.js"),
        "json-gc-stringify:ok",
    );
}
