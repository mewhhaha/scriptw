const DIFFERENTIAL_SOURCE: &str = include_str!("../src/differential.rs");
const WORKER_SOURCE: &str = include_str!("../src/differential/worker.rs");
const PROCESS_SOURCE: &str = include_str!("../src/differential/worker_process.rs");
const ROOTED_SOURCE: &str = include_str!("../src/differential/rooted_snapshot.rs");
const WORKER_CONTROLS: &str = include_str!("differential_worker_execution.rs");
const HARNESS_SOURCE: &str = include_str!("../src/lib.rs");
const ORACLE_GATE: &str = "#[cfg(any(test,feature=\"spec-exec-oracle\"))]";

fn normalized(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn replay_and_comparison_machinery_requires_test_or_oracle_capability() {
    let source = normalized(DIFFERENTIAL_SOURCE);
    for declaration in [
        "usescriptw_engine::{CompileOptions,ModuleLoadingPolicy,ObservedCompletion,ObservedJsValue};",
        "enumOutputComparisonPolicy{",
        "implDifferentialBackend{",
        "implExecutionDisposition{",
        "implCompletionKindObservation{",
        "implPrimitiveValueObservation{",
        "fnvalue(&self)->&PrimitiveValueObservation{",
        "implUnsupportedObservedValueType{",
        "implFailurePhase{",
        "implOutputUnavailableReason{",
        "constfnoutput_policy(self)->OutputComparisonPolicy{",
        "#[derive(Debug)]structBackendExecution{",
        "#[derive(Debug)]enumBackendExecutionResult{",
        "constfnexecution_failure_phase(backend:DifferentialBackend)->FailurePhase{",
        "fncompile_options_for_case(case:&DifferentialCase)->CompileOptions{",
        "fncompare_observations(",
        "fnobeys_output_policy(",
        "fnproject_backend_execution(",
        "fnproject_primitive_completion(",
        "constfncompare_v1_dispositions(",
        "fncompare_v2_observations(",
        "fncompare_v3_observations(",
        "fnv2_execution_signature(",
        "fnv3_mismatch_signature(",
        "fnv4_mismatch_signature(",
        "fnv5_mismatch_signature(",
        "fnv3_backend_observation_signature(",
        "fnprimitive_value_signature(",
    ] {
        let offset = source
            .find(declaration)
            .unwrap_or_else(|| panic!("missing `{declaration}`"));
        assert!(
            source[..offset].ends_with(ORACLE_GATE),
            "`{declaration}` is outside the oracle compile boundary"
        );
    }
    assert!(source.contains("#[cfg(feature=\"spec-exec-oracle\")]modworker;"));
    assert!(source
        .contains("#[cfg(feature=\"spec-exec-oracle\")]pubuseworker::run_differential_worker;"));
    assert!(!source.contains("fnexecute_case("));
    assert!(normalized(WORKER_SOURCE).contains("fnexecute_case("));
    let process = normalized(PROCESS_SOURCE);
    assert!(process
        .contains("#[cfg(feature=\"spec-exec-oracle\")]pub(super)structCompletedWorkerAttempt"));
    assert!(process.contains("#[cfg(all(feature=\"spec-exec-oracle\",unix))]structLiveWorker"));
    let rooted = normalized(ROOTED_SOURCE);
    for declaration in [
        "pub(super)fnterminal_limits_match(",
        "pub(super)fncompare(",
        "pub(super)fnsignature(",
    ] {
        let offset = rooted
            .find(declaration)
            .unwrap_or_else(|| panic!("missing `{declaration}`"));
        assert!(
            rooted[..offset].ends_with(ORACLE_GATE),
            "`{declaration}` is outside the oracle compile boundary"
        );
    }
    // Wire/admission is available in product builds; actual graph execution
    // remains only in the feature-gated sole backend worker.
    assert!(source.contains("modrooted_snapshot;"));
    assert!(rooted.contains("pubfnnew_snapshot_script("));
    assert!(rooted.contains("pubfnnew_snapshot_embedded("));
    assert!(!rooted.contains("Engine::"));
    let worker = normalized(WORKER_SOURCE);
    assert!(worker.contains("engine.observe_script_graph("));
    assert!(worker.contains("engine.observe_module_graph("));
    assert!(!source.contains("engine.observe_script_graph("));
    assert!(!source.contains("engine.observe_module_graph("));

    // Native replay identity must be available before a selected worker runs,
    // including in default builds that can only return OracleNotLinked.
    // These helpers format/hash an admitted input; they grant no execution.
    for declaration in [
        "impl DifferentialGoal {",
        "const FNV_OFFSET_BASIS: u64 =",
        "fn fnv_update(",
        "fn fnv_field(",
        "fn input_fingerprint(",
    ] {
        let prefix = DIFFERENTIAL_SOURCE
            .split_once(declaration)
            .unwrap_or_else(|| panic!("missing native identity owner `{declaration}`"))
            .0;
        assert!(!prefix.trim_end().ends_with(']'),
            "native replay identity must not acquire an oracle-only item attribute: `{declaration}`");
    }
    let fingerprint = source
        .split_once("fninput_fingerprint(")
        .unwrap()
        .1
        .split_once("#[cfg(test)]fncase_fingerprint(")
        .unwrap()
        .0;
    for input in [
        "case.goal().as_str()",
        "case.observation_contract().as_str()",
        "case.filename.as_bytes()",
        "case.timeout_ms.get()",
        "case.source().as_bytes()",
        "case.snapshot_limits()",
        "case.module_graph()",
    ] {
        assert!(
            fingerprint.contains(input),
            "missing native identity input `{input}`"
        );
    }
    for forbidden in [
        "Engine::",
        "runner.run(",
        "execute_case(",
        "observe_script(",
        "observe_module(",
    ] {
        assert!(!fingerprint.contains(forbidden));
    }
}

#[test]
fn test_only_mutation_and_feature_only_loader_have_explicit_boundaries() {
    let harness = normalized(HARNESS_SOURCE);
    assert!(harness.contains("#[cfg(test)]fnvalues_mut(&mutself)->Option<&mutVec<T>>{"));
    assert!(!harness.contains("fnskip_template_source("));

    let differential = normalized(DIFFERENTIAL_SOURCE);
    let controls = normalized(WORKER_CONTROLS);
    assert!(controls.contains("#![cfg(feature=\"spec-exec-oracle\")]"));
    assert!(controls.contains("env!(\"CARGO_BIN_EXE_scriptw-differential-worker\")"));
    assert!(controls.contains("modloader_policy{"));
    assert!(controls
        .contains("fnfilesystem_control_and_reject_all_cover_every_spec_exec_host_context("));
    assert!(!controls.contains("fnexecute_case("));
    let feature_off = differential
        .split_once("#[cfg(not(feature=\"spec-exec-oracle\"))]pubfnreplay_case(")
        .unwrap()
        .1
        .split_once(ORACLE_GATE)
        .unwrap()
        .0;
    assert!(feature_off.contains("Err(DifferentialError::OracleNotLinked)"));
    assert!(!feature_off.contains("runner.run("));
    assert!(!feature_off.contains("Engine::"));
}
