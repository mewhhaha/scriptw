use std::fs;
use std::path::Path;

const ENGINE_SOURCE: &str = include_str!("../../src/lib.rs");
const ENGINE_TEST_SOURCE: &str = include_str!("../../src/tests/control_flow.rs");
const GRAPH_SOURCE: &str = include_str!("../../src/graph_observation.rs");
const EMBEDDING_SOURCE: &str = include_str!("../../src/embedding.rs");
const NATIVE_DIRECT_EVAL_TEST_SOURCE: &str =
    include_str!("../../src/native_direct_eval_compiler_tests.rs");
const NATIVE_DIRECT_EVAL_OWNERSHIP_TEST_SOURCE: &str =
    include_str!("../../src/native_direct_eval_compiler_tests/ownership.rs");
const NATIVE_GLOBAL_SCRIPT_TEST_SOURCE: &str =
    include_str!("../../src/native_global_script_compiler_tests.rs");
const NATIVE_SOURCE_COMPILER_SOURCE: &str =
    include_str!("../../src/wasm_native_source_compiler.rs");
const NATIVE_SOURCE_COMPILER_TEST_SOURCE: &str =
    include_str!("../../src/wasm_native_source_compiler/tests.rs");
const NATIVE_COMPILER_TEST_GATE: &str = "#[cfg(all(test, feature = \"native-source-compiler\"))]";

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn normalized(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn count_in_rust_sources(dir: &Path, needle: &str) -> usize {
    fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .map(|path| {
            if path.is_dir() {
                return count_in_rust_sources(&path, needle);
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return 0;
            }
            fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
                .matches(needle)
                .count()
        })
        .sum()
}

#[test]
fn wasm_execution_mode_is_the_exact_private_no_capability_domain() {
    let fields = bounded(
        ENGINE_SOURCE,
        "pub struct Engine {",
        "\n}\n\nenum WasmExecutionMode {",
    );
    // Test observers require both test isolation and native-source capability.
    // Only this exact conjunction may leave the production-state inventory.
    let mut production_fields = String::new();
    let mut test_field = false;
    for line in fields.lines() {
        if line.trim() == NATIVE_COMPILER_TEST_GATE {
            assert!(
                !test_field,
                "a test field must end before the next attribute"
            );
            test_field = true;
        } else if test_field {
            if line.trim_end().ends_with(',') {
                test_field = false;
            }
        } else {
            production_fields.push_str(line);
        }
    }
    assert!(!test_field, "every test-only field is complete");
    assert_eq!(normalized(&production_fields), "realm:Realm,");
    assert_eq!(
        ENGINE_SOURCE.matches("\nenum WasmExecutionMode {").count(),
        1
    );
    let declaration = bounded(
        ENGINE_SOURCE,
        "enum WasmExecutionMode {",
        "enum WasmExecutionOutcome {",
    );
    assert_eq!(
        normalized(declaration),
        "Legacy,Structured,Graph(SnapshotLimits),}"
    );
    assert!(!declaration.contains("#[derive"));

    let production = ENGINE_SOURCE
        .split_once("\n#[cfg(test)]\nmod tests;")
        .expect("engine unit-test boundary")
        .0;
    assert_eq!(production.matches("WasmExecutionMode").count(), 18);
    assert_eq!(ENGINE_SOURCE.matches("WasmExecutionMode").count(), 18);
    assert_eq!(ENGINE_TEST_SOURCE.matches("WasmExecutionMode").count(), 2);
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for module in [
        "native_direct_eval_compiler_tests",
        "native_global_script_compiler_tests",
    ] {
        let declaration = format!("\n{NATIVE_COMPILER_TEST_GATE}\nmod {module};");
        assert_eq!(ENGINE_SOURCE.matches(declaration.as_str()).count(), 1);
    }
    assert_eq!(
        NATIVE_SOURCE_COMPILER_SOURCE
            .matches("\n#[cfg(test)]\nmod tests;")
            .count(),
        1,
        "the native source compiler regression consumer is test-only"
    );
    let native_test_consumers = [
        (
            NATIVE_DIRECT_EVAL_TEST_SOURCE,
            "assert_trace",
            "fn assert_trace(source: &str, expected: &[&str], unmatched: &[Vec<u16>],
                counts: (usize, usize, usize),) -> NativeDirectEvalCompilationSnapshot",
            "30_000",
            "engine",
        ),
        (
            NATIVE_DIRECT_EVAL_OWNERSHIP_TEST_SOURCE,
            "direct_eval_native_compilation_and_execution_keep_the_original_parent_timeout",
            "fn direct_eval_native_compilation_and_execution_keep_the_original_parent_timeout()",
            "500",
            "engine",
        ),
        (
            NATIVE_GLOBAL_SCRIPT_TEST_SOURCE,
            "assert_native_trace",
            "fn assert_native_trace(source: &str, expected: &[&str],
                unmatched: &[(PreparedScriptKind, Vec<u16>)], iterator: bool,
                counts: NativeGlobalScriptCompilationSnapshot, function_factories: usize,)",
            "30_000",
            "engine",
        ),
        (
            NATIVE_SOURCE_COMPILER_TEST_SOURCE,
            "native_function_and_regexp_eval_resolve_new_exports_from_the_original_r",
            "fn native_function_and_regexp_eval_resolve_new_exports_from_the_original_r()",
            "30_000",
            "compiler",
        ),
    ];
    let mut native_test_mode_uses = 0;
    for (source, name, signature, timeout, receiver) in native_test_consumers {
        let marker = format!("\nfn {name}(");
        assert_eq!(source.matches(marker.as_str()).count(), 1);
        let function = bounded(source, &marker, "\n}");
        let (declaration, _) = function
            .split_once('{')
            .expect("private test function body");
        assert_eq!(
            normalized(&format!("fn {name}({declaration}")),
            normalized(signature)
        );
        assert_eq!(source.matches("WasmExecutionMode").count(), 1);
        assert_eq!(function.matches("WasmExecutionMode").count(), 1);
        let route = format!(
            "{receiver}.execute_with_wasm_bytes_inner(artifact.program(),Some({timeout}),false,\
                WasmModuleMemoryCachePolicy::Retain,&WasmExecutionMode::Structured,)"
        );
        assert_eq!(normalized(function).matches(route.as_str()).count(), 1);
        native_test_mode_uses += 1;
    }
    // The embedding entry fixes structured capture without exposing a mode
    // parameter. Admit this one named consumer, keeping the domain private.
    let embedding_state = bounded(
        EMBEDDING_SOURCE,
        "pub fn new_state<T: Any + Send>(",
        "\n    fn check(",
    );
    assert_eq!(EMBEDDING_SOURCE.matches("WasmExecutionMode").count(), 1);
    assert_eq!(embedding_state.matches("WasmExecutionMode").count(), 1);
    assert!(normalized(embedding_state)
        .contains("output_events:WasmOutputEvents::for_mode(&WasmExecutionMode::Structured),"));
    let (embedding_parameters, _) = embedding_state
        .split_once('{')
        .expect("embedding state constructor body");
    assert!(!embedding_parameters.contains("WasmExecutionMode"));
    // These named cfg(test) consumers and fixed embedding entry add no mode.
    assert_eq!(
        count_in_rust_sources(&source_root, "WasmExecutionMode"),
        25 + native_test_mode_uses + 1
    );
    for forbidden in [
        "pub enum WasmExecutionMode",
        "impl WasmExecutionMode",
        "Default for WasmExecutionMode",
        "Clone for WasmExecutionMode",
        "Copy for WasmExecutionMode",
        "Debug for WasmExecutionMode",
        "PartialEq for WasmExecutionMode",
        "Eq for WasmExecutionMode",
        "== WasmExecutionMode::",
        "!= WasmExecutionMode::",
        "matches!(mode",
    ] {
        assert!(!production.contains(forbidden), "found `{forbidden}`");
    }
}

#[test]
fn ordinary_and_graph_entry_points_fix_their_exact_execution_mode() {
    let entries = [
        (
            "fn run_source_with_cached_wasm(",
            "fn observe_source_with_cached_wasm(",
            "Legacy",
        ),
        (
            "fn observe_source_with_cached_wasm(",
            "fn run_source_with_cached_wasm_on_current_thread(",
            "Structured",
        ),
        (
            "fn run_source_with_cached_wasm_on_current_thread(",
            "fn execute_source_with_cached_wasm_on_current_thread(",
            "Legacy",
        ),
        (
            "fn run_with_wasm_bytes_inner(",
            "fn execute_with_wasm_bytes_inner(",
            "Legacy",
        ),
        (
            "fn run_with_wasm_bytes_inner_with_agents(",
            "fn execute_with_wasm_bytes_inner_with_agents(",
            "Legacy",
        ),
    ];
    for (start, end, variant) in entries {
        let entry = bounded(ENGINE_SOURCE, start, end);
        assert_eq!(entry.matches("WasmExecutionMode::").count(), 1, "{start}");
        assert!(
            entry.contains(&format!("&WasmExecutionMode::{variant},")),
            "{start} must select {variant}"
        );
    }

    let production = ENGINE_SOURCE
        .split_once("\n#[cfg(test)]\nmod tests;")
        .expect("engine unit-test boundary")
        .0;
    assert_eq!(production.matches("mode: &WasmExecutionMode,").count(), 4);
    let graph_entry = bounded(
        GRAPH_SOURCE,
        "let result = self.execute_with_wasm_bytes_inner(",
        "fn compile_graph_artifact(",
    );
    assert_eq!(
        graph_entry
            .matches("&WasmExecutionMode::Graph(limits),")
            .count(),
        2
    );
    assert_eq!(graph_entry.matches("WasmExecutionMode::").count(), 2);
    assert!(graph_entry.contains("WasmExecutionOutcome::Graph(outcome) => Ok(outcome)"));
    assert!(graph_entry
        .contains("WasmExecutionOutcome::Legacy(_) | WasmExecutionOutcome::Structured(_) =>"));
    assert!(!graph_entry.contains("_ =>"));
}

#[test]
fn both_consumers_exhaustively_project_output_ownership_and_result_shape() {
    let output_projection = bounded(
        ENGINE_SOURCE,
        "fn for_mode(mode: &WasmExecutionMode) -> Self {",
        "fn record(&self, text: &str)",
    );
    assert_eq!(
        normalized(output_projection),
        "matchmode{WasmExecutionMode::Legacy=>Self::DelegateOnly,WasmExecutionMode::Structured|WasmExecutionMode::Graph(_)=>{Self::Capture(Arc::new(Mutex::new(Vec::new())))}}}"
    );

    let execution = bounded(
        ENGINE_SOURCE,
        "fn execute_with_wasm_bytes_inner_with_agents(",
        "fn read_module_entry_status(",
    );
    assert!(execution.contains("output_events: WasmOutputEvents::for_mode(mode),"));
    let graph_projection = bounded(
        execution,
        "if let WasmExecutionMode::Graph(limits) = mode {",
        "let (result_tag, completion_kind, value) =",
    );
    assert!(graph_projection.contains(
        "wasm_rooted_snapshot::observe(&mut roots, &observed_instance, completion, *limits)?"
    ));
    assert_eq!(
        graph_projection
            .matches("return Ok(WasmExecutionOutcome::Graph(")
            .count(),
        1
    );
    assert!(
        graph_projection.contains("output_events: roots.as_context().data().output_events.take()")
    );
    assert!(
        execution.find("wasm_rooted_snapshot::observe(")
            < execution.find("wasm_gc_completion::observe(")
    );
    let result_projection = bounded(execution, "match mode {", "\n    }\n}");
    assert_eq!(
        result_projection
            .matches("WasmExecutionMode::Legacy =>")
            .count(),
        1
    );
    assert_eq!(
        result_projection
            .matches("WasmExecutionMode::Structured =>")
            .count(),
        1
    );
    assert_eq!(
        result_projection
            .matches("Ok(WasmExecutionOutcome::Legacy(")
            .count(),
        1
    );
    assert_eq!(
        result_projection
            .matches("Ok(WasmExecutionOutcome::Structured(")
            .count(),
        1
    );
    let legacy_arm = bounded(
        result_projection,
        "WasmExecutionMode::Legacy => {",
        "WasmExecutionMode::Structured => {",
    );
    assert_eq!(
        legacy_arm
            .matches("Ok(WasmExecutionOutcome::Legacy(")
            .count(),
        1
    );
    assert!(!legacy_arm.contains("WasmExecutionOutcome::Structured"));
    let structured_arm = result_projection
        .split_once("WasmExecutionMode::Structured => {")
        .expect("structured result arm")
        .1;
    assert_eq!(
        structured_arm
            .matches("Ok(WasmExecutionOutcome::Structured(")
            .count(),
        1
    );
    assert!(!structured_arm.contains("WasmExecutionOutcome::Legacy"));
    assert_eq!(result_projection.matches("WasmExecutionMode::Graph(_) => unreachable!(\"graph mode returned before scalar decoding\")").count(), 1);
    assert!(!result_projection.contains("_ =>"));
    assert!(execution.find("WasmOutputEvents::for_mode(mode)") < execution.find("match mode {"));
}
