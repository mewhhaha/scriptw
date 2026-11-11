use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion,
    ObservedJsValue, RealmBuilder, RunOptions,
};

fn assert_accessors(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    #[cfg(feature = "spec-exec-oracle")]
    let backends = [ExecutionBackend::WasmAot, ExecutionBackend::SpecExec];
    #[cfg(not(feature = "spec-exec-oracle"))]
    let backends = [ExecutionBackend::WasmAot];
    for backend in backends {
        for directive in ["", "'use strict';\n"] {
            let source = format!(
                "{directive}function check(value) {{ if (!value) throw Error('stack regression'); }}\n{source}\ntrue;"
            );
            let result = Engine::new(RealmBuilder::new().build())
                .observe_script(
                    &source,
                    CompileOptions {
                        host_surface_policy: HostSurfacePolicy::Test262,
                        ..CompileOptions::default()
                    },
                    RunOptions {
                        backend,
                        timeout_ms: Some(30_000),
                        ..RunOptions::default()
                    },
                )
                .expect("Error stack accessors compile and execute");
            assert_eq!(
                result.completion,
                ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
                "{backend:?}: {}\n{source}",
                result.note
            );
        }
    }
}

#[test]
fn error_stack_getters_use_native_brands_and_captured_text() {
    assert_accessors(include_str!("../fixtures/error_stack_accessors/getter.js"));
}

#[test]
fn error_stack_setters_preserve_descriptor_and_proxy_effects() {
    assert_accessors(include_str!("../fixtures/error_stack_accessors/setter.js"));
}

#[test]
fn borrowed_error_stack_accessors_keep_their_realms() {
    assert_accessors(include_str!("../fixtures/error_stack_accessors/realms.js"));
}
