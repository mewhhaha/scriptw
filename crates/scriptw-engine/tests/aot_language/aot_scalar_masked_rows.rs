//! Masked compact-row regions publish only completed prefixes before fallback.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn masked_rows_preserve_aliases_live_storage_prefix_bailout_and_tdz() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let fixture = include_str!("../fixtures/scalar_masked_rows.js");
    for (module, directive) in [(true, ""), (false, ""), (false, "\"use strict\";\n")] {
        let source = format!("{directive}{fixture}");
        let engine = Engine::new(RealmBuilder::new().build());
        let compile = CompileOptions {
            host_surface_policy: HostSurfacePolicy::Test262,
            ..CompileOptions::default()
        };
        let run = RunOptions {
            backend: ExecutionBackend::WasmAot,
            timeout_ms: Some(30_000),
            ..RunOptions::default()
        };
        let observed = if module {
            engine.observe_module(&source, compile, run)
        } else {
            engine.observe_script(&source, compile, run)
        }
        .expect("masked row control executes compiled Wasm");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "module={module}; directive={directive:?}; {:?}",
            observed.completion
        );
        assert_eq!(
            observed.output_events,
            [HostOutputEvent::PrintLine("scalar-masked-rows:ok".into())]
        );
    }
}
