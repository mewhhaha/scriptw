//! `language` CLI integration tests: pipeline shape, bindings, scoping,
//! closures, `for-in` and TDZ.
//!
//! # Why this module is three modules
//!
//! Not taste — memory, measured. As one 105-test libtest process this module
//! could not be run at all on the 4-CPU / 15.7 GiB container: three attempts
//! (batch 6, 22:43Z / 23:29Z / 00:30Z) were each SIGKILLed by the OOM killer at
//! t+1200 s after 66, 75 and 75 tests, with `avail` falling MONOTONICALLY from
//! 8.5 GiB at ~7 tests to 3.56 GiB at ~49 and 1.14 GiB two minutes before the
//! kill. That trajectory is cumulative growth across the process, not a
//! three-in-flight plateau, so fewer tests per process was the lever this split
//! reached for. (Do **not** reach for `frontend_test262_subset`'s flat 5.55 GiB
//! as the contrast — that plateau belongs to a child process, see below.)
//!
//!   * per-tier cache limits at 256/64/64 MiB were tried and changed nothing —
//!     they bound bytes on disk, not RSS (`scriptw-engine/src/cache.rs`);
//!   * `SCRIPTW_CPU_PERCENT` is overridden inside `run_chunk`
//!     (`scripts/rung1c-chunks.sh`);
//!   * `--test-threads` below 3 is banned by that script's property 1 — libtest
//!     then names every worker thread `main`, `known_failures::execution_path`
//!     cannot route on the per-test name, and all ~600 tests fall back to
//!     spawning a cold `scriptw` child.
//!
//! # What the growth might be — a hypothesis, and the counterevidence
//!
//! An earlier version of this comment called the split "the only lever left".
//! That list is three *environment knobs*; it never examined in-process
//! retention, and there is a named candidate mechanism there:
//! The predecessor's native-module LRU bounded retained Wasmtime modules only
//! by entry count: 64 entries, no byte ceiling. The in-process Retain path
//! held native modules per distinct fixture, which explains why the three
//! *disk* knobs did not control this particular retention owner. The
//! 2026-10-03 dry replacement in `scriptw-engine/src/memory_module_cache.rs` adds
//! a 512 MiB compilation-image budget while preserving the entry limit. Its
//! acceptance is pending; this does not identify the historical OOM cause.
//!
//! That is where the confidence stops, and the entry-count story does **not**
//! survive the banked chunks. Counted, not estimated, from
//! `target/watched/rung1c-done-counts` and the current sources:
//!
//! | chunk | banked | `ProcessCommand::new` | `.arg("run")` | distinct fixtures | mean fixture bytes |
//! |---|---|---|---|---|---|
//! | `array` | 84 | 0 | 84 | 77 | 1,945 |
//! | `typed_array` | 58 | 0 | 58 | 48 | 2,716 |
//! | `language` (pre-split) | never | 1 | 32 | 33 | 1,285 |
//!
//! `array` is 84 in-process `run` invocations over 77 distinct fixtures, i.e.
//! it pins the 64-entry LRU at its cap with sources half again as large as
//! `language`'s, and it **banked**. `typed_array` sits at 48 retained modules —
//! inside the 53/62/62 band the fatal `language` runs reached — and banked too.
//! So retained-entry count does not explain the `language` OOM, and neither
//! does the linear `~0.118 GiB/test` model below, which would project `array`
//! at ~9.9 GiB. Whatever the real variable is (per-fixture *bytes* of native
//! code is the obvious suspect, and it is exactly what an entry-count LRU does
//! not bound), it is unidentified.
//!
//! Two further corrections to the earlier version of this paragraph, because it
//! reasoned from both:
//!
//! * the 53/62/62 figures are `tests_done - 13`, which silently assumes one
//!   distinct fixture per test. The 105 tests reference 81 distinct fixtures,
//!   so those are over-counts of retained modules, not measurements of them.
//! * `frontend_test262_subset`'s flatness is **not** evidence about this
//!   process's LRU. Its single call is a real child
//!   (`ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"))`,
//!   `frontend_test262_subset.rs:123` — `grep -c 'ProcessCommand::new'` is 1),
//!   so its 5.55 GiB plateau is the *child's* RSS across hundreds of test262
//!   cases and the libtest process's module cache never sees it.
//!
//! Likewise the "cheap" tests kept here do **not** run as child processes: only
//! `build_wasm_succeeds_for_dynamic_fractional_exponentiation_fixture` uses
//! `ProcessCommand`. The other five `build_wasm_*` and all six `inspect_*` call
//! `crate::Command`, whose `output()` (`main.rs:186-215`) dispatches
//! `ExecutionPath::InProcess` to `scriptw_cli::run_cli_capture` on a worker
//! thread of *this* process; the `program` field is unused on that path, so
//! `env!("CARGO_BIN_EXE_scriptw")` there is decoration. They are cheap for a
//! different reason: `build wasm` and `inspect` never reach
//! `WasmModuleMemoryCachePolicy::Retain`, which is on the `run` path only
//! (`scriptw-engine/src/lib.rs`), so they add no cache entry — they do add
//! transient RSS.
//!
//! `SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES` and the new
//! `SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES` configure positive retention limits.
//! The byte limit covers cache-held code/data/debug images, not active clones,
//! compilation or RSS. A fresh resource experiment is still needed before
//! attributing the historical failure or changing the measured chunk sizing.
//!
//! Splitting by libtest FILTER is not available either:
//! `known_failures::rung_1c_chunks` asserts each chunk's second argument is
//! exactly `<name>::` and that anything further is `--skip <other>::`, and
//! `rung_1c_chunks_cover_every_cli_area_module` asserts a bijection between
//! chunk names, `tests/cli/*.rs` stems and `mod` lines in `main.rs`. An
//! `--exact` name list is rejected at rung 0. So the split has to be by module
//! file, which is what [`crate::language_errors`] and [`crate::language_numerics`]
//! are.
//!
//! Sizing rests on ONE direct measurement, and it is deliberately not the
//! extrapolations. The standalone 30-name tail **completed** in a fresh process
//! (30 in 498.2 s, 16.6 s/test) where 75 tests in 1200 s (16.0 s/test) did not,
//! so ~30 heavy tests per process is a size that has actually been observed to
//! finish. Hence three chunks, at 32 / 29 / 31 heavy tests.
//!
//! The `avail` trajectory (8.5 GiB @ ~7 tests, 3.56 @ ~49, 1.14 @ ~67) reads
//! ~0.118 GiB/test and reaches the same answer, but see the counterevidence
//! above: `array` banked 84 in-process runs, which that line projects at
//! ~9.9 GiB. Treat it as consistent, not as support. Per-module *distinct*
//! fixtures — the unit an entry-count LRU would care about — are 32 / 21 / 28,
//! not the test counts; `language_errors` shares 8 of its fixtures across its
//! 29 tests. This module keeps the 13 cheap ones
//! (`in_process_module_reuse_*`, six `inspect_reports_phase_*`, six
//! `build_wasm_succeeds_for_*`) as well, since they cost almost nothing.
//!
//! THE THREE STEMS MUST NOT BECOME `::`-SUFFIXES OF ONE ANOTHER. The overlap
//! rule in `rung_1c_chunks_cover_every_cli_area_module` fires when
//! `format!("{other}::").ends_with(&format!("{chunk}::"))`, and
//! `"language_errors::".ends_with("language::")` is false — which is exactly why
//! these three need no `--skip` while `array` needs `--skip typed_array::`. By
//! the same token libtest's substring filter `language::` does not select
//! `language_errors::…`, so the three chunks run in three SEPARATE processes,
//! which is the entire point: the accumulation is per-process.

use crate::*;

#[test]
fn run_wasm_backend_supports_iteration_array_and_arguments_member_references() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg(fixture_path("wasm_iteration_member_references.js"))
        .output()
        .expect("iteration reference fixture should run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"), "{stdout}");
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn in_process_module_reuse_keeps_host_output_in_fresh_realms() {
    let path = fixture_path("wasm_host_output.js");
    let args = ["run", "--execution-backend", "wasm", path.as_str()];
    for _ in 0..2 {
        let output = scriptw_cli::run_cli_capture(args.map(str::to_string));
        assert_eq!(
            output.exit_code,
            0,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.matches("root\n").count(), 1);
        assert_eq!(stdout.matches("alias\n").count(), 1);
        assert_eq!(stdout.matches("method\n").count(), 1);
    }
}

#[test]
fn inspect_reports_phase_nineteen_global_resolution_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_global_resolution.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("global_property_reads="));
    assert!(stdout.contains("global_property_writes="));
    assert!(stdout.contains("implicit_globals="));
}

#[test]
fn inspect_reports_phase_twenty_host_output_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_host_output.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("host_globals=1"));
    assert!(stdout.contains("host_builtin_calls=3"));
}

#[test]
fn inspect_reports_phase_twenty_four_abrupt_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_abrupt_core.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("try_finallys=2"));
    assert!(stdout.contains("deletes=2"));
    // Five `===` comparisons, two `in` (HasProperty) and the two `f.kind` /
    // `f.arrowKind` reads. Since 5eca93e67 `new F()` no longer carries `F`'s
    // instance shape, so those two reads lower to `GetV` spec operations
    // instead of shape-typed property reads (7 -> 9).
    assert!(stdout.contains("spec_operations=9"), "{stdout}");
    assert!(stdout.contains("in_ops=0"));
    assert!(stdout.contains("new_target_uses=3"));
}

#[test]
fn inspect_reports_phase_twenty_five_builtin_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_builtin_globals.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // `builtin_globals` counts the program's global bindings that resolve to a
    // builtin, i.e. it is a function of the GLOBAL ENVIRONMENT and not of this
    // fixture's text. So every batch that adds an intrinsic root moves it and
    // this assertion goes red without anything in the fixture changing.
    // Batch 8: 51 -> 52 for `AsyncDisposableStack`; the constructor-only
    // `%DisposableStack%` shell then moves 52 -> 53. `%Float16Array%`
    // (ES2025, catalog `GlobalOrdinal(53)`, 7a7610705) moves 53 -> 54.
    // `ShadowRealm` at `GlobalOrdinal(54)` moves 54 -> 55. Runtime ABI 47
    // (40ff4b4ba) adds the Web host globals `TextEncoder`, `TextDecoder`,
    // `AbortController`, `AbortSignal`, `EventTarget`, `Event`,
    // `DOMException`, `ReadableStream`, `ReadableStreamDefaultReader`,
    // `ReadableByteStreamController`, `queueMicrotask`, `setTimeout` and
    // `clearTimeout` at `GlobalOrdinal(55..=67)`, moving 55 -> 68. Recount
    // with `scriptw inspect crates/scriptw-cli/tests/fixtures/wasm_builtin_globals.js`
    // rather than guessing the delta; do not weaken this to a prefix match,
    // because the exact number is the only thing that makes an accidental
    // global-environment change visible at rung 1b.
    assert!(stdout.contains(" builtin_globals=68 "), "{stdout}");
    assert!(stdout.contains("builtin_ctor_calls="));
    assert!(stdout.contains("builtin_static_calls="));
    assert!(stdout.contains("error_builtin_calls="));
}

#[test]
fn inspect_reports_phase_twenty_nine_delete_global_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_delete_globals.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("deletes="));
    assert!(stdout.contains("identifier_deletes="));
    assert!(stdout.contains("global_deletes="));
}

#[test]
fn inspect_reports_phase_thirty_null_heritage_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_null_heritage.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("null_heritage_classes="));
}

#[test]
fn build_wasm_succeeds_for_dynamic_fractional_exponentiation_fixture() {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_exponentiation_dynamic_fractional_core.js",
        ))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_global_resolution_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_global_resolution.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_abrupt_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_abrupt_core.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_builtin_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_builtin_globals.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_delete_global_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_delete_globals.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_null_heritage_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_null_heritage.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn run_wasm_backend_preserves_var_parameter_bindings() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_var_parameter_bindings.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_outer_bindings_during_recursion() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_recursive_function_outer_binding.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_supports_annex_b_block_functions() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_annexb_block_functions.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

/// B.3.2.1's evaluation of an Annex B function declaration writes the
/// function's VariableEnvironment directly (`fenv.SetMutableBinding`); it must
/// not resolve through a same-named catch parameter that B.3.4 lets coexist
/// with the var-scoped name, while a plain `var` initializer in that catch
/// block still assigns the parameter.
#[test]
fn run_wasm_backend_keeps_catch_parameters_under_annex_b_function_copies() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_annexb_catch_parameter_copy.js"))
        .output()
        .expect("run command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("backend_used: WasmAot"), "{stdout}");
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_captures_annex_b_block_function_bindings() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_annexb_block_capture_aliases.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_preserves_function_values_after_for_lexical_initializers() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_for_lexical_function_value.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(9)"));
}

#[test]
fn run_wasm_backend_preserves_captured_block_lexical_environments() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_block_lexical_environments.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_for_in_array_key_capture_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_for_in_array_key_capture.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_script_global_var_nested_update_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_script_global_var_nested_update.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_optional_property_chain_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_optional_property_chain.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_optional_private_property_chain_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_optional_private_property_chain.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_supported_strict_this_calls_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_strict_this_calls.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_lexical_super_home_object_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_lexical_super_home_object.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_object_literal_home_object_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_object_literal_home_object.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_object_method_arrow_super_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_object_method_arrow_super.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_super_property_reference_mutation_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_super_property_reference_mutation.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_delete_super_reference_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_delete_super_reference.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_with_object_environment_putvalue_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_with_object_environment_putvalue.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_with_object_environment_getvalue_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_with_object_environment_getvalue.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_with_environment_identifier_call_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_with_environment_identifier_call.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_with_environment_numeric_update_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_with_environment_numeric_update.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_global_object_environment_numeric_update_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_global_object_environment_numeric_update.js",
        ))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_object_environment_logical_assignment_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_object_environment_logical_assignment.js",
        ))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_with_environment_compound_assignment_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_with_environment_compound_assignment.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_global_object_environment_compound_assignment_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_global_object_environment_compound_assignment.js",
        ))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_block_function_declaration_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_block_function_declaration.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_lexical_shadowing_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_lexical_shadowing.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_script_lexical_capture_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_script_lexical_capture.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_for_in_let_closure_capture_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_for_in_let_closure_capture.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_shadowed_for_in_let_closure_capture_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_for_in_shadowed_let_closure_capture.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_preserves_supported_binding_pattern_capture_storage_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_binding_pattern_capture_storage_contract.js",
        ))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_for_in_order_simple_object_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_for_in_order_simple_object.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_for_in_prototype_order_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_for_in_prototype_order.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_for_in_array_define_property_order_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_for_in_array_define_property_order.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_for_in_head_tdz_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_for_in_head_tdz.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_enforces_runtime_lexical_tdz_for_pattern_initializers() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_runtime_lexical_tdz_patterns.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_uses_iterators_for_call_argument_spread() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_call_argument_spread_iterators.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_depth_two_const_array_capture_immutability() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_depth_two_const_array_capture.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_missing_arguments_shadowing_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_missing_arguments_shadowing.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_global_resolution_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_global_resolution.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(4"));
}

#[test]
fn run_wasm_backend_succeeds_for_global_constant_descriptors_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_global_constant_descriptors.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true"));
}

#[test]
fn run_wasm_backend_applies_one_global_declaration_instantiation_plan() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_global_declaration_instantiation.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_builtin_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_builtin_globals.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_delete_global_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_delete_globals.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_null_heritage_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_null_heritage.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

/// A hoisted function's `const` capture must not be typed from the hoist-time
/// TDZ placeholder.
///
/// Function declarations are lowered before the statement list, so when
/// `function fb() { return B; }` captures the top-level `const B`, `B` is still
/// the uninitialized placeholder whose kind is `Undefined`. Publishing that as
/// the capture's proven value propagates into `signature.return_kind`, and then
/// `typeof fb()` constant-folds to `"undefined"` without ever calling `fb`.
///
/// Both fields are plain observable JavaScript, so this stays a black-box
/// check rather than an assertion about inferred kinds. The fixture documents
/// the wider const-capture operator-selection defect that this test
/// deliberately does not cover.
#[test]
fn run_wasm_backend_types_a_hoisted_functions_const_capture_from_its_initializer() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_const_capture_return_kind.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(
        stdout.contains("const-capture-return-kind:object:1"),
        "{stdout}"
    );
}

/// A top-level global write is visible to intrinsic folds inside closures.
///
/// Function bodies are lowered before the root-statement pass replays
/// top-level writes, so without the prepass write summary a closure reading
/// `Symbol.iterator` after `Symbol = undefined` folded to the stale intrinsic
/// instead of throwing TypeError.
#[test]
fn run_wasm_backend_observes_global_clobber_inside_closures() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_global_symbol_clobber_in_closure.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}
