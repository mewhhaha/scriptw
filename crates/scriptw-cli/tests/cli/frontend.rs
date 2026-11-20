//! `frontend` CLI integration tests.

use crate::*;

#[test]
fn help_lists_clean_break_commands() {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("--help")
        .output()
        .expect("help command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("build wasm"));
    assert!(stdout.contains("types [entrypoint]"));
    assert!(stdout.contains("test262 run"));
    assert!(stdout.contains("inspect"));
    assert!(!stdout.contains("build c "));
    assert!(!stdout.contains("build native "));
    assert!(!stdout
        .lines()
        .any(|line| line.trim_start().starts_with("repl ")));
}

#[test]
fn obsolete_product_commands_are_rejected_before_source_loading() {
    for target in ["c", "native"] {
        for profile in [None, Some("minimal"), Some("custom:selected")] {
            let mut command = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"));
            if let Some(profile) = profile {
                command.args(["--intl-profile", profile]);
            }
            let output = command
                .args(["build", target, "missing-retired-target-source.js"])
                .output()
                .expect("unsupported target should report a command error");
            assert!(!output.status.success());
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains(&format!("unknown build target: {target}")));
            assert!(!stderr.contains("missing-retired-target-source.js"));
        }
    }
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("repl")
        .output()
        .expect("retired REPL should report a command error");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown command: repl"));
}

#[test]
fn subprocess_argument_handling_rejects_missing_run_source() {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .output()
        .expect("run command should report a missing source");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("run needs a source file"));
}

#[test]
fn types_generates_from_discovered_jsonc_config_and_entrypoint() {
    let project = unique_project_dir("types-jsonc");
    write_project_file(
        &project,
        "src/index.ts",
        r#"
export default {
  fetch(request, env, ctx) {
    return new Response(env.MESSAGE);
  },
  scheduled() {}
};
"#,
    );
    write_project_file(
        &project,
        "wrangler.jsonc",
        r#"{
  // JSONC comments and trailing commas match common Wrangler config files.
  "main": "src/index.ts",
  "compatibility_date": "2026-06-19",
  "kv_namespaces": [{ "binding": "CACHE", "id": "x" }],
  "r2_buckets": [{ "binding": "ASSETS", "bucket_name": "assets" }],
  "vars": { "MESSAGE": "hello", "COUNT": 3 },
  "env": {
    "prod": {
      "vars": { "MESSAGE": "prod" },
      "queues": { "producers": [{ "binding": "JOBS", "queue": "jobs" }] }
    },
  },
}
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("types")
        .arg("--cwd")
        .arg(&project)
        .arg("--print")
        .output()
        .expect("types command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("// Config: wrangler.jsonc"));
    assert!(stdout.contains("// Entrypoint: src/index.ts"));
    assert!(stdout.contains("// Detected handlers: fetch, scheduled"));
    assert!(stdout.contains("CACHE: KVNamespace;"));
    assert!(stdout.contains("ASSETS: R2Bucket;"));
    assert!(stdout.contains("COUNT: 3;"));
    assert!(stdout.contains("MESSAGE: \"hello\" | \"prod\";"));
    assert!(stdout.contains("JOBS?: Queue;"));
    assert!(stdout.contains("compatibilityDate: \"2026-06-19\";"));
}

#[test]
fn types_uses_explicit_entrypoint_and_check_for_written_output() {
    let project = unique_project_dir("types-explicit");
    write_project_file(
        &project,
        "src/worker.ts",
        r#"
export default {
  fetch() {
    return new Response("ok");
  }
};
"#,
    );
    write_project_file(
        &project,
        "src/config-main.ts",
        r#"
export default {
  scheduled() {}
};
"#,
    );
    write_project_file(
        &project,
        "wrangler.json",
        r#"{"main":"src/config-main.ts","vars":{"FLAG":true},"services":[{"binding":"API","service":"api"}]}"#,
    );

    let output_path = "types/worker-env.d.ts";
    let write = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("typegen")
        .arg("src/worker.ts")
        .arg(output_path)
        .arg("--cwd")
        .arg(&project)
        .arg("--config")
        .arg("wrangler.json")
        .output()
        .expect("typegen command should write");

    assert!(
        write.status.success(),
        "{}",
        String::from_utf8_lossy(&write.stderr)
    );

    let check = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("types")
        .arg("src/worker.ts")
        .arg(output_path)
        .arg("--cwd")
        .arg(&project)
        .arg("--config")
        .arg("wrangler.json")
        .arg("--check")
        .output()
        .expect("types --check should run");

    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let written = fs::read_to_string(project.join(output_path)).expect("types output should exist");
    assert!(written.contains("FLAG: true;"));
    assert!(written.contains("API: Fetcher;"));
    assert!(written.contains("entrypoint: \"src/worker.ts\";"));
    assert!(written.contains("syntax: \"module\";"));
    assert!(written.contains("// Detected handlers: fetch"));
    assert!(!written.contains("// Detected handlers: scheduled"));
}

#[test]
fn types_reads_toml_env_and_can_omit_runtime_declarations() {
    let project = unique_project_dir("types-toml");
    write_project_file(
        &project,
        "src/worker.ts",
        r#"
addEventListener("fetch", event => event.respondWith(new Response("ok")));
"#,
    );
    write_project_file(
        &project,
        "wrangler.toml",
        r#"
main = "src/worker.ts"
compatibility_date = "2026-06-19"

[vars]
MODE = "dev"

[[d1_databases]]
binding = "DB"
database_name = "main"

[env.preview.vars]
MODE = "preview"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("typegen")
        .arg("--cwd")
        .arg(&project)
        .arg("--env")
        .arg("preview")
        .arg("--env-interface")
        .arg("WorkerEnv")
        .arg("--include-runtime=false")
        .arg("--print")
        .output()
        .expect("typegen toml command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("interface WorkerEnv"));
    assert!(stdout.contains("MODE: \"preview\";"));
    assert!(stdout.contains("DB: D1Database;"));
    assert!(stdout.contains("syntax: \"service-worker\";"));
    assert!(!stdout.contains("ExecutionContext"));
}

#[test]
fn inspect_reports_pipeline_invariants() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("hello.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("goal: Script"));
    assert!(stdout.contains("direct-js-to-wasm-only"));
    assert!(stdout.contains("stages: parsed-source, script-ir-built, wasm-ready"));
}

#[test]
fn inspect_reports_phase_five_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_switch.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ir: script statements="));
    assert!(stdout.contains("switches=1"));
    assert!(stdout.contains("labels=1"));
    assert!(stdout.contains("debuggers=1"));
}

#[test]
fn inspect_reports_phase_six_var_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_var.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("vars=3"));
}

#[test]
fn inspect_reports_phase_seven_function_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_functions.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // `functions` and `returns` count every lowered `FunctionIr`, and that
    // includes the exact-context specialization clones lowering adds per
    // distinct canonical argument context (`exact_context_function_specializations`,
    // `lowering/call_candidate_analysis.rs`). Since 8c80a8b8f `inc` is observed
    // in two contexts -- `add2`'s `x` and the inner `inc(x)` result -- so the
    // two source functions lower to `inc`, `inc$exact_helper_context$0`,
    // `inc$exact_helper_context$1` and `add2`. `calls` is unaffected: the three
    // call sites are all in the uncloned `add2` and the script body.
    assert!(stdout.contains(" functions=4 "), "{stdout}");
    assert!(stdout.contains("calls=3"));
    assert!(stdout.contains("returns=4"), "{stdout}");
}

#[test]
fn inspect_reports_phase_eight_object_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_objects.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("objects=1"));
    assert!(stdout.contains("arrays=1"));
    assert!(stdout.contains("property_reads=1"));
    assert!(stdout.contains("property_writes=1"));
}

#[test]
fn inspect_reports_phase_ten_callable_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_callables.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("function_values="));
    assert!(stdout.contains("indirect_calls="));
    assert!(stdout.contains("method_calls="));
    assert!(stdout.contains("this_reads="));
}

#[test]
fn inspect_reports_phase_eleven_closure_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_closures.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("nested_functions=1"));
    assert!(stdout.contains("function_exprs=2"));
    assert!(stdout.contains("closures="));
    assert!(stdout.contains("captures="));
}

#[test]
fn inspect_reports_phase_fourteen_param_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_params.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Since c294028e7 `arguments` resolves statically, so the Script keeps
    // static identifiers and exact call targets. The four source functions
    // gain three exact-context specialization clones: `outer` from
    // `outer(3, 4, 5)` and `pick` once per argument context (`{}` and
    // `{ value: 5 }`). The source defaults are `outer`'s `x = 1` and `f`'s
    // `y = this.x` (`pick`'s `{ value = 2 }` is a binding-pattern default, not
    // a parameter default) and the only rest parameter is `outer`'s `...xs`;
    // the `outer` clone repeats both. `f` has no clone: calling `outer` is
    // unknown user code, which erases `o`'s shape before `o.f()`.
    assert!(stdout.contains("functions=7"), "{stdout}");
    assert!(stdout.contains("default_params=3"), "{stdout}");
    assert!(stdout.contains("rest_params=2"), "{stdout}");
    // The arrow reads `outer`'s implicit arguments cell through its capture,
    // not an ExprIr::Arguments object expression and not a runtime Reference.
    assert!(stdout.contains("arguments_uses=0"), "{stdout}");
    assert!(stdout.contains("lexical_arguments_captures=1"), "{stdout}");
    assert!(
        stdout.contains("environment_identifier_reads=0"),
        "{stdout}"
    );
    assert!(
        stdout.contains("environment_identifier_calls=0"),
        "{stdout}"
    );
    // The parameter default `y = this.x` is executable IR as well.
    assert!(stdout.contains("this_reads=1"), "{stdout}");
}

#[test]
fn inspect_reports_phase_sixteen_coercion_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_coercions.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("loose_equalities=1"));
    assert!(stdout.contains("coercive_numeric_ops=1"));
    assert!(stdout.contains("coercive_relational_ops=1"));
    assert!(stdout.contains("void_uses=1"));
    assert!(stdout.contains("comma_ops=1"));
}

#[test]
fn inspect_reports_phase_eighteen_global_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_globals.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The canonical global environment plus the fixture's `x`, `f` and `g`,
    // including the mutable public `globalThis` data property. 40ff4b4ba
    // (Runtime ABI 47) added thirteen Web host globals to every Script, which
    // moved 69 -> 82; `inspect_reports_phase_twenty_five_builtin_ir_shape`
    // lists them.
    assert!(stdout.contains("global_bindings=82"), "{stdout}");
    // The three comparisons and `globalThis.x` retain their actual receiver.
    // This fixture proves the builtin property intact at all four uses.
    assert!(stdout.contains("global_this_uses=4"), "{stdout}");
    // The script's own `this` plus the one inherited by the arrow `g`, which
    // 37ba2cdd3 resolves to the Script root binding and counts; `g`'s
    // exact-context specialization clone lowers that arrow body a second time.
    assert!(stdout.contains("top_level_this_uses=3"), "{stdout}");
    assert!(stdout.contains("global_default_this_calls=2"));
}

#[test]
fn inspect_reports_phase_twenty_one_constructor_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_constructors.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("constructs="));
    assert!(stdout.contains("instanceofs="));
    assert!(stdout.contains("prototype_reads="));
    assert!(stdout.contains("prototype_writes="));
}

#[test]
fn inspect_reports_phase_twenty_three_exception_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_exceptions.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("throws=1"));
    assert!(stdout.contains("try_catches=2"));
}

#[test]
fn build_wasm_dump_replaces_linked_runtime_with_runtime_free_artifact() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    struct DumpTree(std::path::PathBuf);
    impl Drop for DumpTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let tree = DumpTree(unique_project_dir("wasm-dump-lifecycle"));
    let dump = tree.0.join("artifact.wasm");
    let mut runtime_path = dump.as_os_str().to_os_string();
    runtime_path.push(".runtime.wasm");
    let runtime_path = std::path::PathBuf::from(runtime_path);
    let engine = scriptw_engine::Engine::new(scriptw_engine::RealmBuilder::new().build());
    for (filename, source, requires_runtime, builds) in [
        (
            "linked.js",
            "let o = { x: 1 }; o.y = 2; o.x + o.y;",
            true,
            1,
        ),
        ("scalar.js", "40 + 2;", false, 2),
    ] {
        let script = tree.0.join(filename);
        fs::write(&script, source).unwrap();
        let unit = engine
            .compile_script(
                source,
                scriptw_engine::CompileOptions {
                    filename: Some(script.to_string_lossy().into_owned()),
                    host_surface_policy: scriptw_engine::HostSurfacePolicy::Product,
                    ..scriptw_engine::CompileOptions::default()
                },
            )
            .unwrap();
        let expected = engine.emit_wasm(&unit).unwrap();
        assert_eq!(
            expected.runtime.is_some(),
            requires_runtime,
            "{filename}: the fixture must exercise its declared runtime requirement"
        );
        // The repeated scalar build also verifies that a missing sidecar is
        // accepted when replacing an already runtime-free dump.
        for _ in 0..builds {
            let built = Command::new(env!("CARGO_BIN_EXE_scriptw"))
                .args(["--jobs", "1", "--host-surface", "product", "build", "wasm"])
                .arg(&script)
                .env("SCRIPTW_WASM_DUMP", &dump)
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{filename}: {}",
                String::from_utf8_lossy(&built.stderr)
            );
            assert_eq!(
                fs::read(&dump).unwrap(),
                expected.bytes,
                "{filename}: the dumped program must match the exact SDK artifact"
            );
            if let Some(runtime) = &expected.runtime {
                let emitted =
                    fs::read(&runtime_path).expect("linked dump must publish its runtime");
                assert!(
                    emitted.as_slice() == runtime.bytes(),
                    "{filename}: the dumped runtime must match the exact SDK artifact"
                );
            } else {
                assert_eq!(
                    fs::symlink_metadata(&runtime_path).unwrap_err().kind(),
                    io::ErrorKind::NotFound,
                    "{filename}: a runtime-free dump must leave no previous sidecar"
                );
            }
        }
    }
}

#[test]
fn build_wasm_succeeds_for_supported_fixture() {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_var.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_function_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_functions.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_object_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_objects.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_callable_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_callables.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_closure_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_closures.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_param_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_params.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_coercion_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_coercions.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_global_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_globals.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_host_output_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("hello.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_constructor_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_constructors.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_exception_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_exceptions.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_fixture() {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_var.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(6"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_function_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_functions.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(3"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_object_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_objects.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(2"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_callable_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_callables.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(18"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_closure_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_closures.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(5"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_param_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_params.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(2"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_coercion_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_coercions.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(2"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_global_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_globals.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true"));
}

#[test]
fn run_wasm_backend_succeeds_for_host_output_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("hello.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("grug"));
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("undefined"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_constructor_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_constructors.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(19"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_exception_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_exceptions.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("string(ReferenceError)"));
}

#[test]
fn run_wasm_backend_handles_many_choice_free_regexp_captures() {
    let root = unique_project_dir("wasm-regexp-many-captures");
    let capture_count = 500;
    let source = format!(
        "var match = /{}/.exec(\"{}\"); if (match.length !== {} || match[1] !== \"a\" || match[{}] !== \"a\") throw \"capture endpoints\"; true;",
        "(a)".repeat(capture_count),
        "a".repeat(capture_count),
        capture_count + 1,
        capture_count,
    );
    write_project_file(&root, "many-captures.js", &source);
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(root.join("many-captures.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_handles_many_one_shot_regexp_choices() {
    let root = unique_project_dir("wasm-regexp-many-one-shot-choices");
    let atom_count = 2_000;
    let source = format!(
        "var match = /{}/.exec(\"{}\"); if (match === null || match[0].length !== {}) throw \"one-shot choice match\"; true;",
        "a?".repeat(atom_count),
        "a".repeat(100_000),
        atom_count,
    );
    write_project_file(&root, "many-one-shot-choices.js", &source);
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(root.join("many-one-shot-choices.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("boolean(true)"));
}

#[test]
fn test262_list_works_with_fixture_suite() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("list")
        .arg("--suite-root")
        .arg(suite_root())
        .output()
        .expect("test262 list should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("count: 191"));
    assert!(stdout.contains("sloppy-script:language/fail/parse-negative.js"));
    assert!(stdout.contains("strict-script:language/fail/parse-negative.js"));
}

#[cfg(feature = "spec-exec-oracle")]
#[test]
fn test262_run_writes_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("run")
        .arg("--execution-backend")
        .arg("spec-exec")
        .arg("--suite-root")
        .arg(suite_root())
        .arg("--snapshot-dir")
        .arg(snapshot_dir())
        .arg("--snapshot-name")
        .arg("cli-fixture")
        .output()
        .expect("test262 run should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("execution_backend: spec-exec"));
    assert!(stdout.contains("total: 191"));
    assert!(stdout.contains("passed: 191"));
    assert!(stdout.contains("Unsupported: 0"));
}

fn failing_test262_suite_root(name: &str) -> std::path::PathBuf {
    let suite_root = unique_project_dir(name);
    let test_dir = suite_root.join("test/language/fail");
    fs::create_dir_all(&test_dir).expect("failing test262 directory should be created");
    fs::write(
        test_dir.join("throws.js"),
        "/*---\nflags: [raw]\n---*/\nthrow new Error('intentional failure');\n",
    )
    .expect("failing test262 case should write");
    suite_root
}

fn unsupported_test262_suite_root(name: &str) -> std::path::PathBuf {
    let suite_root = unique_project_dir(name);
    // Exercise the native caller's explicit ExternalCallerImport boundary.
    // Build the eval text at runtime so it takes the native compilation path.
    let units = "import('./unavailable.js');"
        .encode_utf16()
        .map(|unit| unit.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let source = format!("/*---\nflags: [raw]\n---*/\neval(String.fromCharCode({units}));\n");
    write_project_file(&suite_root, "test/language/unsupported/feature.js", &source);
    suite_root
}

#[test]
fn test262_feature_metadata_does_not_reject_a_supported_raw_execution() {
    let suite_root = unique_project_dir("test262-supported-feature-metadata");
    write_project_file(
        &suite_root,
        "test/language/supported/feature.js",
        "/*---\nfeatures: [immutable-arraybuffer]\nflags: [raw]\n---*/\ntrue;\n",
    );
    // Preserve the nested output directory and exercise both worker protocols
    // against the same source after a previous process has written snapshots.
    for reuse in [false, true] {
        let mut command = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"));
        command
            .arg("test262")
            .arg("run")
            .arg("--suite-root")
            .arg(&suite_root)
            .arg("--execution-backend")
            .arg("wasm-aot")
            .arg("--threads")
            .arg("1")
            .arg("--snapshot-dir")
            .arg(suite_root.join("snapshots"))
            .arg("--snapshot-name")
            .arg("supported-feature-metadata");
        if reuse {
            command.arg("--reuse-case-workers");
        }
        let output = command
            .output()
            .expect("metadata-only selection should complete");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("total: 1") && stdout.contains("passed: 1"),
            "{stdout}"
        );
        assert!(stdout.contains("Unsupported: 0"), "{stdout}");
    }
}

#[test]
fn test262_run_exits_unsuccessfully_when_a_case_fails() {
    let suite_root = failing_test262_suite_root("test262-failing-run");

    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("run")
        .arg("--suite-root")
        .arg(&suite_root)
        .arg("--snapshot-dir")
        .arg(unique_snapshot_dir("failing-run"))
        .arg("--snapshot-name")
        .arg("cli-failing-run")
        .output()
        .expect("failing test262 run should complete");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("total: 1"));
    assert!(stdout.contains("passed: 0"));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("test262 run failed: 1 of 1 cases did not pass"));
}

#[test]
fn test262_shard_exits_unsuccessfully_and_keeps_failure_snapshot() {
    let suite_root = failing_test262_suite_root("test262-failing-shard");
    let snapshot_dir = unique_snapshot_dir("failing-shard");
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("shard")
        .arg("--shard")
        .arg("1/1")
        .arg("--suite-root")
        .arg(&suite_root)
        .arg("--snapshot-dir")
        .arg(&snapshot_dir)
        .arg("--snapshot-name")
        .arg("cli-failing-shard")
        .output()
        .expect("failing test262 shard should complete");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("shard: 1/1"));
    assert!(stdout.contains("total: 1"));
    assert!(stdout.contains("passed: 0"));
    assert!(stdout.contains("failed: 1"));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("test262 shard failed: 1 of 1 cases did not pass"));

    let snapshot_paths = fs::read_dir(&snapshot_dir)
        .expect("failed shard snapshot directory should exist")
        .map(|entry| {
            entry
                .expect("failed shard snapshot entry should read")
                .path()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        snapshot_paths.len(),
        2,
        "failed shard should retain exactly its JSON and text snapshots: {snapshot_paths:?}"
    );
    let json_path = snapshot_paths
        .iter()
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .expect("failed shard JSON snapshot should exist");
    let json = fs::read_to_string(json_path).expect("failed shard JSON snapshot should read");
    assert!(json.contains("\"total\": 1"));
    assert!(json.contains("\"passed\": 0"));
    assert!(json.contains("language/fail/throws.js"));
}

#[test]
fn test262_run_exits_unsuccessfully_when_selection_is_empty() {
    let suite_root = failing_test262_suite_root("test262-empty-selection");
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("run")
        .arg("language/not-present")
        .arg("--suite-root")
        .arg(&suite_root)
        .arg("--snapshot-dir")
        .arg(unique_snapshot_dir("empty-selection"))
        .arg("--snapshot-name")
        .arg("cli-empty-selection")
        .output()
        .expect("empty test262 selection should complete");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("total: 0"));
    assert!(stdout.contains("passed: 0"));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("test262 run produced no verdict: zero cases were selected"));
}

#[test]
fn test262_run_and_shard_reject_unsupported_and_keep_failure_snapshots() {
    let suite_root = unsupported_test262_suite_root("test262-unsupported-verdict");

    for (command, shard_selector) in [("run", None), ("shard", Some("1/1"))] {
        let snapshot_dir = unique_snapshot_dir(&format!("unsupported-{command}"));
        let snapshot_name = format!("cli-unsupported-{command}");
        let mut process = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"));
        process.arg("test262").arg(command);
        if let Some(shard_selector) = shard_selector {
            process.arg("--shard").arg(shard_selector);
        }
        let output = process
            .arg("--suite-root")
            .arg(&suite_root)
            .arg("--snapshot-dir")
            .arg(&snapshot_dir)
            .arg("--snapshot-name")
            .arg(&snapshot_name)
            .output()
            .expect("unsupported test262 selection should complete");

        assert!(!output.status.success(), "test262 {command} must be red");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("total: 1"), "test262 {command}: {stdout}");
        assert!(stdout.contains("passed: 0"), "test262 {command}: {stdout}");
        assert!(
            stdout.contains("Unsupported: 1"),
            "test262 {command}: {stdout}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!(
                "test262 {command} failed: 1 of 1 cases did not pass"
            )),
            "test262 {command} should explain its red verdict"
        );

        let snapshot_paths = fs::read_dir(&snapshot_dir)
            .expect("unsupported failure snapshot directory should exist")
            .map(|entry| {
                entry
                    .expect("unsupported failure snapshot entry should read")
                    .path()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            snapshot_paths.len(),
            2,
            "test262 {command} should retain its JSON and text snapshots"
        );
        let json_path = snapshot_paths
            .iter()
            .find(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .expect("unsupported failure JSON snapshot should exist");
        let json =
            fs::read_to_string(json_path).expect("unsupported failure JSON snapshot should read");
        assert!(json.contains("\"Unsupported\": 1"));
        assert!(json.contains("\"kind\": \"Unsupported\""));
        assert!(json.contains("language/unsupported/feature.js"));
    }
}

#[cfg(feature = "spec-exec-oracle")]
#[test]
fn test262_report_groups_failures_by_bucket() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("report")
        .arg("--execution-backend")
        .arg("spec-exec")
        .arg("--suite-root")
        .arg(suite_root())
        .arg("--snapshot-dir")
        .arg(snapshot_dir())
        .arg("--snapshot-name")
        .arg("cli-report")
        .output()
        .expect("test262 report should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("execution_backend: spec-exec"));
    assert!(stdout.contains("passed: 191"));
    assert!(stdout.contains("failed: 0"));
}

#[cfg(feature = "spec-exec-oracle")]
#[test]
fn test262_report_all_aggregates_fixture_suite() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("report-all")
        .arg("--execution-backend")
        .arg("spec-exec")
        .arg("--suite-root")
        .arg(suite_root())
        .arg("--snapshot-dir")
        .arg(snapshot_dir())
        .arg("--snapshot-name")
        .arg("cli-report-all")
        .output()
        .expect("test262 report-all should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("execution_backend: spec-exec"));
    assert!(stdout.contains("total: 191"));
    assert!(stdout.contains("passed: 191"));
    assert!(stdout.contains("targets:"));
}

#[cfg(feature = "spec-exec-oracle")]
#[test]
fn test262_backlog_and_snapshot_compare_read_completed_matrix_snapshots() {
    let snapshot_dir = unique_snapshot_dir("backlog-compare");
    let suite_root = copied_suite_root("backlog-compare");
    let base_snapshot_name = "cli-backlog-compare-base";
    let candidate_snapshot_name = "cli-backlog-compare-candidate";
    for snapshot_name in [base_snapshot_name, candidate_snapshot_name] {
        let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
            .arg("test262")
            .arg("report-all")
            .arg("--execution-backend")
            .arg("spec-exec")
            .arg("--suite-root")
            .arg(&suite_root)
            .arg("--snapshot-dir")
            .arg(&snapshot_dir)
            .arg("--snapshot-name")
            .arg(snapshot_name)
            .output()
            .expect("test262 report-all should run");

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let backlog = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("generate-backlog")
        .arg("--execution-backend")
        .arg("spec-exec")
        .arg("--suite-root")
        .arg(&suite_root)
        .arg("--snapshot-dir")
        .arg(&snapshot_dir)
        .arg("--snapshot-name")
        .arg(candidate_snapshot_name)
        .output()
        .expect("test262 generate-backlog should run");

    assert!(
        backlog.status.success(),
        "{}",
        String::from_utf8_lossy(&backlog.stderr)
    );
    let stdout = String::from_utf8_lossy(&backlog.stdout);
    assert!(stdout.contains("execution_backend: spec-exec"));
    assert!(stdout.contains("total: 191"));
    assert!(stdout.contains("records: 0"));
    assert!(stdout.contains("backlog_json:"));
    assert!(stdout.contains("backlog_txt:"));

    let compare = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("compare-snapshots")
        .arg(base_snapshot_name)
        .arg("--execution-backend")
        .arg("spec-exec")
        .arg("--suite-root")
        .arg(&suite_root)
        .arg("--snapshot-dir")
        .arg(&snapshot_dir)
        .arg("--snapshot-name")
        .arg(candidate_snapshot_name)
        .output()
        .expect("test262 compare-snapshots should run");

    assert!(
        compare.status.success(),
        "{}",
        String::from_utf8_lossy(&compare.stderr)
    );
    let stdout = String::from_utf8_lossy(&compare.stdout);
    assert!(stdout.contains("base_snapshot: cli-backlog-compare-base"));
    assert!(stdout.contains("candidate_snapshot: cli-backlog-compare-candidate"));
    assert!(stdout.contains("added_passes: 0"));
    assert!(stdout.contains("regressions: 0"));
    assert!(stdout.contains("changed_failure_hashes: 0"));
}

#[cfg(feature = "spec-exec-oracle")]
#[test]
fn test262_publish_status_rejects_oracle_backend_before_writing() {
    let readme_path = temp_readme_path("publish-status-rejects-spec");
    let before = std::fs::read_to_string(&readme_path).expect("fixture readme should read");
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("publish-status")
        .arg("--execution-backend")
        .arg("spec-exec")
        .arg("--suite-root")
        .arg(suite_root())
        .arg("--snapshot-dir")
        .arg(snapshot_dir())
        .arg("--snapshot-name")
        .arg("cli-publish-status-rejects-spec")
        .arg("--readme-path")
        .arg(&readme_path)
        .output()
        .expect("test262 publish-status should run");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("spec-exec is an oracle-only backend and cannot publish"));
    assert_eq!(
        std::fs::read_to_string(&readme_path).expect("readme should remain readable"),
        before,
        "rejecting oracle publication must not mutate the README"
    );
}

#[test]
fn test262_publish_status_rejects_max_matrix_nodes() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("publish-status")
        .arg("--suite-root")
        .arg(suite_root())
        .arg("--snapshot-dir")
        .arg(snapshot_dir())
        .arg("--snapshot-name")
        .arg("cli-publish-status-reject-limit")
        .arg("--max-matrix-nodes")
        .arg("1")
        .output()
        .expect("test262 publish-status should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("publish-status does not allow --max-matrix-nodes"));
}

#[cfg(feature = "spec-exec-oracle")]
#[test]
fn test262_progress_status_reports_incomplete_aggregate_without_publishing() {
    let snapshot_dir = std::env::temp_dir()
        .join(format!(
            "scriptw-cli-test262-progress-status-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
        .display()
        .to_string();
    let snapshot_name = "cli-progress-status";
    let seed = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("report-all")
        .arg("--execution-backend")
        .arg("spec-exec")
        .arg("--suite-root")
        .arg(suite_root())
        .arg("--snapshot-dir")
        .arg(&snapshot_dir)
        .arg("--snapshot-name")
        .arg(snapshot_name)
        .arg("--max-matrix-nodes")
        .arg("1")
        .output()
        .expect("partial report-all should run");

    assert!(seed.status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("progress-status")
        .arg("--execution-backend")
        .arg("spec-exec")
        .arg("--suite-root")
        .arg(suite_root())
        .arg("--snapshot-dir")
        .arg(&snapshot_dir)
        .arg("--snapshot-name")
        .arg(snapshot_name)
        .output()
        .expect("test262 progress-status should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("execution_backend: spec-exec"));
    assert!(stdout.contains("complete=false"));
    assert!(stdout.contains("matrix_nodes_completed: 1"));
    assert!(stdout.contains("matrix_nodes_total:"));
    assert!(stdout.contains("observed_total:"));
    assert!(stdout.contains("target_total: 191"));
    assert!(stdout.contains("unobserved_total:"));
    assert!(stdout.contains("current_success:"));
    assert!(stdout.contains("current_success_full:"));
    assert!(stdout.contains("remaining_observed_failures:"));
    assert!(stdout.contains("remaining_to_green:"));
    assert!(stdout.contains("outcomes:"));
    assert!(stdout.contains("  Success:"));
    assert!(stdout.contains("  NotImplemented:"));
    assert!(stdout.contains("  Crash:"));
    assert!(stdout.contains("  Bug:"));
    assert!(stdout.contains("burn_down: NotImplemented="));
    assert!(stdout.contains("not_run:"));

    assert!(!std::path::Path::new(&snapshot_dir)
        .join("published-status-spec-exec.json")
        .exists());
    assert!(!std::path::Path::new(&snapshot_dir)
        .join("published-status-spec-exec.txt")
        .exists());
}

#[cfg(feature = "spec-exec-oracle")]
#[test]
fn test262_triage_and_failure_details_read_completed_matrix_snapshots() {
    let snapshot_dir = std::env::temp_dir()
        .join(format!(
            "scriptw-cli-test262-triage-status-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
        .display()
        .to_string();
    let snapshot_name = "cli-triage-status";
    let seed = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("report-all")
        .arg("--execution-backend")
        .arg("spec-exec")
        .arg("--suite-root")
        .arg(suite_root())
        .arg("--snapshot-dir")
        .arg(&snapshot_dir)
        .arg("--snapshot-name")
        .arg(snapshot_name)
        .output()
        .expect("report-all should run");

    assert!(seed.status.success());

    let triage = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("triage-status")
        .arg("--execution-backend")
        .arg("spec-exec")
        .arg("--suite-root")
        .arg(suite_root())
        .arg("--snapshot-dir")
        .arg(&snapshot_dir)
        .arg("--snapshot-name")
        .arg(snapshot_name)
        .output()
        .expect("triage-status should run");

    assert!(triage.status.success());
    let stdout = String::from_utf8_lossy(&triage.stdout);
    assert!(stdout.contains("execution_backend: spec-exec"));
    assert!(stdout.contains("failing_nodes:"));
    assert!(stdout.contains("ranking: Crash,Bug,NotImplemented,failed"));

    let details = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("test262")
        .arg("failure-details")
        .arg("language/wasm")
        .arg("--execution-backend")
        .arg("spec-exec")
        .arg("--suite-root")
        .arg(suite_root())
        .arg("--snapshot-dir")
        .arg(&snapshot_dir)
        .arg("--snapshot-name")
        .arg(snapshot_name)
        .output()
        .expect("failure-details should run");

    assert!(details.status.success());
    let stdout = String::from_utf8_lossy(&details.stdout);
    assert!(stdout.contains("execution_backend: spec-exec"));
    assert!(stdout.contains("node_id: language/wasm"));
    assert!(stdout.contains("filter: language/wasm"));
    assert!(stdout.contains("detail_groups: 0"));
}

// `test262_wasm_backend_runs_supported_fixture_subset` used to live here. It now
// lives in `frontend_test262_subset.rs`, moved verbatim apart from a `--threads 2`
// on its child, so that `scripts/rung1c-chunks.sh` can give it a chunk of its
// own: it is the single most memory-expensive test in rung 1c (8.4-8.7 GiB in
// one child process, measured), and this chunk failed to produce a verdict in
// three consecutive container windows with it inside. Those three failures were
// three DIFFERENT failures — one OOM SIGKILL, one container restart, one stall-
// guard kill — and an earlier version of this comment called all three OOM
// kills, which is why only the memory half was ever addressed. That module's
// header carries the corrected record, the measurement and the naming
// constraint. Do not move it back.
