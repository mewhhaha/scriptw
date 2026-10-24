use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

fn assert_modes(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("bounded compiler jobs");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{source}");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
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
            .expect("private Array buffer fixture executes through Wasm AOT");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0))),
            "{source}"
        );
        assert_eq!(
            observed.output_events,
            vec![HostOutputEvent::PrintLine(expected.into())],
            "{source}"
        );
    }
}

#[test]
fn sort_is_stable_without_quadratic_comparisons_and_preserves_gc_identity() {
    assert_modes(
        include_str!("../fixtures/array_private_buffers/sort_values.js"),
        "array-private-sort-values:ok",
    );
}

#[test]
fn sort_snapshots_gets_and_preserves_comparator_abrupt_and_writeback_semantics() {
    assert_modes(
        include_str!("../fixtures/array_private_buffers/sort_effects.js"),
        "array-private-sort-effects:ok",
    );
}

#[test]
fn string_buffer_preserves_live_join_order_reentrancy_and_utf16() {
    assert_modes(
        include_str!("../fixtures/array_private_buffers/join.js"),
        "array-private-join:ok",
    );
}

#[test]
fn locale_buffer_preserves_call_receivers_options_gc_and_typed_array_witnesses() {
    assert_modes(
        include_str!("../fixtures/array_private_buffers/locale.js"),
        "array-private-locale:ok",
    );
}
