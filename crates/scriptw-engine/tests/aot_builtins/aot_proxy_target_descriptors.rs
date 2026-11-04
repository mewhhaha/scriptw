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
            .expect("Proxy target descriptor controls execute through Wasm AOT");
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
fn four_trap_invariants_observe_real_target_descriptors_before_constraints() {
    assert_modes(
        include_str!("../fixtures/proxy_target_descriptors/target_operations.js"),
        "proxy-target-descriptors:ok",
    );
}

#[test]
fn recursive_descriptor_errors_use_the_executing_operation_realm() {
    assert_modes(
        include_str!("../fixtures/proxy_target_descriptors/operation_realms.js"),
        "proxy-target-descriptor-realms:ok",
    );
}

#[test]
fn ordinary_descriptor_snapshots_survive_reentrant_redefinition_and_refresh_later_reads() {
    assert_modes(
        include_str!("../fixtures/proxy_target_descriptors/ordinary_snapshot_mutation.js"),
        "ordinary-descriptor-snapshot-mutation:ok",
    );
}

#[test]
fn exotic_descriptors_preserve_snapshots_and_materialize_independent_public_results() {
    assert_modes(
        include_str!("../fixtures/proxy_target_descriptors/exotic_snapshot_mutation.js"),
        "exotic-descriptor-snapshot-mutation:ok",
    );
}
