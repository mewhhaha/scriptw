use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let result = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("cached input contract executes compiled Wasm");
    assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(result.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        result.completion
    );
    assert_eq!(
        result.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn cached_input_narrowing_preserves_primitive_receivers_keys_and_abrupt_order() {
    for directive in ["", "\"use strict\";\n"] {
        run(
            &format!(
                "{directive}{}",
                include_str!("../fixtures/cached_read_inputs.js")
            ),
            "cached-inputs:ok",
        );
    }
}

#[test]
fn cached_input_fallback_keeps_execution_and_getter_realms() {
    run(
        r#"
function same(actual, expected) {
  if (!Object.is(actual, expected)) throw new Error('unexpected value identity');
}
function throwsConstructor(expected, callback) {
  try { callback(); }
  catch (error) {
    if (error.constructor === expected) return;
    throw new Error('unexpected error constructor');
  }
  throw new Error('expected an exception');
}
var realm = __scriptwCreateRealm(), foreign = realm.global;
var foreignRead = realm.evalScript('(function(value, unused, extra) { return value.field; })');
var foreignGetter = realm.evalScript('(function() { throw new TypeError("foreign getter"); })');
var object = {};
Object.defineProperty(object, 'field', {get: foreignGetter});
function localRead(value, unused, extra) { return value.field; }
throwsConstructor(foreign.TypeError, function() { localRead(object); });
throwsConstructor(TypeError, function() { localRead(null); });
throwsConstructor(foreign.TypeError, function() { foreignRead(null); });
realm.evalScript('Object.defineProperty(Number.prototype, "field", {configurable: true, get: function() { "use strict"; return this; }});');
same(foreignRead(11), 11);
same(localRead(11), undefined);
print('cached-input-realms:ok');
"#,
        "cached-input-realms:ok",
    );
}
