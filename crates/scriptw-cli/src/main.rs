use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use scriptw_engine::{
    cache_status, configure_compilation_jobs, prune_caches, CompileOptions, CustomIntlProfile,
    CustomProfileId, Engine, ExecutionBackend, HostHooks, HostSurfacePolicy,
    IntlCompilationProfile, RealmBuilder, RunOptions,
};
use scriptw_test262::{
    differential::{
        replay_case, run_generated_arithmetic_campaign, ArithmeticCheckCount,
        ArithmeticExpressionDepth, ArithmeticGenerationPlan, ArithmeticGenerationSeed,
        ArithmeticGrammar, ArithmeticReductionLimit, DifferentialReplayInput, DifferentialReport,
        DifferentialVerdict, DifferentialWorkerRunner, GeneratedArithmeticCampaignOutcome,
        SpecExecOracle,
    },
    ArtifactProducer, CompilerProvenance, ConformanceRunVerdict, ConformanceRunner, FailureKind,
    FailureOrigin, LocalHarnessSource, OutcomeKind, PublicationBackend, PublishedStatusPaths,
    RunConfig, SuiteConfig, VerifiedAggregateSummary,
};
use serde::Serialize;
use serde_json::{Map, Value};

mod argument_cursor;
mod test262_args;
mod test262_output;
use argument_cursor::ArgumentCursor;
use test262_args::{AdmittedTest262Command, Test262Command};
#[cfg(test)]
mod argument_tests;
use test262_output::{
    print_kind_counts, print_origin_counts, print_outcome_counts, OutcomeCoverage, OutcomeTotals,
};

mod differential_campaign;
mod differential_corpus;
mod differential_robustness;
mod differential_scenario_pair;
mod differential_test262;
mod fake_suite_publication;
mod intl_data;
mod jsonc;
mod performance;
mod status_publication;
use fake_suite_publication::VerifiedFakeSuiteCounts;

#[derive(Debug)]
struct StdoutHostHooks {
    output: OutputBuffer,
}

impl HostHooks for StdoutHostHooks {
    fn report_exception(&self, text: &str) {
        eprintln!("{text}");
    }
    fn print_line(&self, text: &str) {
        let mut output = self
            .output
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        writeln!(output, "{text}").expect("writing host output should succeed");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedTest262Args {
    provided_options: BTreeSet<String>,
    config: SuiteConfig,
    filter: Option<String>,
    matrix_node: Option<String>,
    run_config: RunConfig,
    readme_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedDifferentialReplayArgs {
    case_path: PathBuf,
    oracle: SpecExecOracle,
    worker_bin: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedDifferentialGenerateArithmeticArgs {
    output_path: PathBuf,
    plan: ArithmeticGenerationPlan,
    reduction_limit: ArithmeticReductionLimit,
    oracle: SpecExecOracle,
    worker_bin: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PublishedStatusCount {
    passed: usize,
    total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PublishedCountEntry {
    label: String,
    count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PublishedTargetEntry {
    filter: String,
    passed: usize,
    total: usize,
    failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PublishedSnapshotPaths {
    json: String,
    txt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PublishedPinnedRevisions {
    ecma262: String,
    test262: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PublishedRealSuiteGoal {
    name: String,
    denominator: String,
    target_total: usize,
    current_success: usize,
    remaining_to_green: usize,
    pass_rate: String,
    outcome_targets: Vec<PublishedCountEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PublishedRealSuiteStatus {
    backend: String,
    refresh_date: String,
    manifest_hash: u64,
    compiler_identity: CompilerProvenance,
    passed: usize,
    total: usize,
    failed: usize,
    pinned_revisions: PublishedPinnedRevisions,
    counts_per_outcome: Vec<PublishedCountEntry>,
    counts_per_kind: Vec<PublishedCountEntry>,
    counts_per_origin: Vec<PublishedCountEntry>,
    top_targets: Vec<PublishedTargetEntry>,
    snapshot_paths: PublishedSnapshotPaths,
    goal: PublishedRealSuiteGoal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PublishedStatusArtifact {
    producer: ArtifactProducer,
    fake_wasm_safe: PublishedStatusCount,
    fake_full: PublishedStatusCount,
    real_suite: PublishedRealSuiteStatus,
}

fn usage() -> &'static str {
    "scriptw <command> [args]

Commands:
  intl export --output PATH             export the selected admitted twelve-frame Intl bundle
  intl inspect --input PATH             re-admit a bundle and print its actual provider identity
  compiler-identity                     print the embedded build-source and
                                        executing-image identity as JSON
  performance report --output-dir PATH --samples 3..100
      --machine-label LABEL --idle-machine [--gate all|warm-exact|warm-chunk|cold-exact]
      [--fixture-root PATH] [--baseline report.json]
                                        retain repeated whole-CLI AOT timings
                                        for the existing opt-in benchmark corpus
  performance compile --output-dir PATH --samples 3..100
      --machine-label LABEL --idle-machine [--fixture-root PATH] [--baseline report.json]
                                        measure actual prepare/lower/emit/validate stages,
                                        retain Wasm footprints and per-stage comparisons
  performance runtime --output-dir PATH --samples 3..100
      --machine-label LABEL --idle-machine [--fixture-root PATH] [--baseline report.json]
                                        execute the fixed emitted corpus in fresh Stores,
                                        retain runtime spans and GC capacity snapshots
  run [--execution-backend wasm|spec] [--module] <file>
                                        compile and run a script through Rust engine path
                                        (default backend: wasm; wasm-aot is the only
                                        supported product execution path). A `.mjs` file
                                        is parsed as an ES module; `--module` forces the
                                        module goal for any other extension
  build wasm <file>                     compile JavaScript directly to Wasm
  cache status                          report bounded Scriptw caches and the
                                        legacy global Wasmtime cache
  cache prune [--legacy-wasmtime]       delete Scriptw caches; optionally delete
                                        the reported legacy Wasmtime cache
  differential replay <case.json> --oracle spec-exec [--worker-bin PATH]
                                        replay one versioned v1 self-checking,
                                        v2 primitive-completion, or v3
                                        primitive-plus-print-transcript case,
                                        or v4 embedded graph, in separate
                                        bounded workers through Wasm-AOT and the
                                        explicitly enabled spec-exec oracle;
                                        emit one JSON observation report
  differential replay-corpus --output-dir PATH --oracle spec-exec
      [--corpus-root PATH] [--worker-bin PATH]
                                        replay the entire compiled or selected
                                        corpus, retain every red/incomplete case
                                        and write a new aggregate/report directory
  differential replay-scenario-pair <pair.json> --oracle spec-exec
      [--worker-bin PATH]                replay both checked metamorphic variants,
                                        report both actual backend observations
                                        and their within-backend relation
  differential seed-test262 --base NAME --candidate NAME
      --selection failures|newly-green --max-seeds N --output-dir PATH
  differential replay-test262 --seed PATH --output-report PATH --oracle spec-exec
      [--worker-bin PATH]                retain exact Test262 harness and mode
  differential robustness --input PATH --target NAME --seed N --cases N
                          --timeout-ms N --output-dir PATH --oracle spec-exec
      targets include encode-uri, encode-uri-component, decode-uri,
      decode-uri-component, prelude and filesystem-resolver (bounded native JSON)
  differential replay-robustness <input.json> --oracle spec-exec
  differential minimize-robustness <input.json> --replays N --output-dir PATH
                                  --oracle spec-exec [--worker-bin PATH]
  performance conformance --snapshot NAME --output-dir PATH
                          [--suite-root PATH] [--snapshot-dir PATH]
                          [--execution-backend wasm-aot|spec-exec]
  performance data --bundle PATH [--bundle PATH ...] --output-dir PATH
  performance check --mode compiler|runtime --baseline PATH --candidate PATH
                    --policy PATH --output-dir PATH
  differential generate-arithmetic <output.json>
      --seed N --checks N --depth 1|2|3|4 --max-replays N --oracle spec-exec
      [--grammar integer-bitwise-v2|integer-arithmetic-v1|integer-product-v3]
      [--worker-bin PATH]
                                        generate one deterministic schema-v1
                                        arithmetic case, replay it, and reduce
                                        a mismatch before writing the case;
                                        default grammar: integer-bitwise-v2
                                        worker defaults to this executable;
                                        embedding callers select a worker binary
  differential campaign --output-dir PATH --seed N --cases 1..128
      --max-replays N --oracle spec-exec [--worker-bin PATH]
      [--grammar integer-product-v3|integer-bitwise-v2|integer-arithmetic-v1
        --checks N --depth 1|2|3|4]
      [--grammar object-probe-v1 --nodes 1..16 --properties 1..16]
      [--grammar object-mutations-v2 --nodes 1..15 --properties 1..16 --steps 1..64]
      [--grammar module-graph-v1|module-graph-v2 --modules 1..16 --edges N]
      [--grammar control-flow-v1 --steps 1..32 --depth 1|2|3|4]
      [--grammar negative-source-v1 --steps 1..32 --depth 1|2|3|4]
      [--grammar builtin-stateful-v1|metamorphic-stateful-v1 --steps 1..32]
      [--grammar builtin-stateful-v2|metamorphic-stateful-v2 --steps 1..32]
                                        replay deterministic consecutive seeds,
                                        journal every initial/reduced candidate,
                                        retain red/incomplete aggregate truth;
                                        default grammar: integer-product-v3
  types [entrypoint] [output] [options] generate Worker-style TypeScript types
  typegen [entrypoint] [output] [options]
                                        alias for types
  test262 close-release [--snapshot-dir PATH] [--threads N] [--reuse-case-workers]
                                        gate two fresh full pinned Wasm-AOT runs
  test262 sync [--suite-root PATH]
  test262 list [filter] [--suite-root PATH]
  test262 list-all [filter] [--suite-root PATH]
                                        print the complete discovered ID inventory
  test262 run [filter] [options]
  test262 run-list <execution-list> [options]
                                        exact mode:path cohort; normal supervision
  test262 shard --shard <index>/<total> [filter] [options]
  test262 report [filter] [options]
  test262 report-all [options]
  test262 publish-status [options]      publish verified wasm-aot status only
  test262 progress-status [options]
  test262 triage-status [options]
  test262 failure-details <matrix-node> [options]
  test262 generate-backlog [options]
  test262 compare-snapshots <base-snapshot-name> [options]
  inspect <file>                        show compile pipeline summary

test262 options (accepted only by commands that use them):
  --shard INDEX/TOTAL                  shard only; one-based, 1 <= INDEX <= TOTAL
  --suite-root PATH
  --snapshot-dir PATH
  --threads N
  --reuse-case-workers                 bounded process reuse; fresh Realm/Store per case
  --timeout-ms N
  --execution-backend wasm|spec         default: wasm (wasm-aot). `spec` selects
                                         spec-exec, an INTERNAL/DEBUG-ONLY interpreter
                                         (Boa) differential oracle for triage/T25 use
                                         only; it is never a product conformance
                                         backend and requires a `spec-exec-oracle`
                                         feature build of scriptw-engine/scriptw-cli
  --resume
  --snapshot-name NAME
  --matrix-node NODE                    run/report one exact matrix leaf by node id or unique filter
  --max-matrix-nodes N                report-all: process at most N new matrix
                                        nodes in this invocation
  --readme-path PATH

global options:
  --jobs N                              Cranelift compiler threads (SCRIPTW_JOBS,
                                        otherwise half the logical CPUs; independent of
                                        test262 --threads)
  --host-surface product|test262        host-global authority (default: product;
                                        test262 is for conformance harnesses)
  --intl-profile minimal|conformance|custom:ID  pinned Intl image selection for build wasm
  --intl-manifest PATH                 strict Custom locale/data/service manifest (v1–v6)
                                        and Wasm run (default: minimal)
  --intl-list-locales es,he             real List data projection within custom:ID
  --intl-relative-time-locales fr,pl    real RelativeTime projection within custom:ID
  --intl-duration-locales fr,sr         real Duration projection within custom:ID
  --intl-displaynames-locales fr,ja     real DisplayNames projection within custom:ID
  --intl-number-locales es,pl           coupled NumberFormat/PluralRules projection
                                        within custom:ID
  --intl-datetime-locales ar-EG,zh      real DateTimeFormat projection within custom:ID
  --intl-collator-locales de-CH,sv      real Collator projection within custom:ID
  --intl-segmenter-locales fi,el        real Segmenter projection within custom:ID

types options:
  --config PATH, -c PATH
  --entrypoint PATH
  --env NAME
  --env-interface NAME
  --include-runtime=false
  --include-env=false
  --strict-vars=false
  --check
  --print
  --cwd PATH
"
}

fn command_main(mut args: Vec<String>, stdout: OutputBuffer) -> Result<(), String> {
    let GlobalOptions {
        jobs,
        host_surface_policy,
        intl_profile,
    } = take_global_options(&mut args)?;
    if let Some(jobs) = jobs {
        configure_compilation_jobs(jobs)?;
    }
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        if intl_profile.is_some() {
            return Err("--intl-profile requires build wasm, a Wasm run or intl export".into());
        }
        print!("{}", usage());
        return Ok(());
    };

    if intl_profile.is_some() && !matches!(command.as_str(), "run" | "build" | "intl") {
        return Err(format!(
            "--intl-profile is unsupported by {command}; use build wasm, a Wasm run or intl export"
        ));
    }

    if matches!(command.as_str(), "--help" | "-h" | "help") {
        print!("{}", usage());
        return Ok(());
    }

    if command == "compiler-identity" {
        if args.next().is_some() {
            return Err("compiler-identity does not accept arguments".into());
        }
        println!(
            "{}",
            serde_json::to_string(&CompilerProvenance::current()?)
                .map_err(|error| format!("cannot render compiler identity: {error}"))?
        );
        return Ok(());
    }

    if command == "performance" {
        if host_surface_policy != HostSurfacePolicy::default() {
            return Err("performance host surfaces are fixed by the benchmark corpus".into());
        }
        if intl_profile.is_some() {
            return Err("performance reporting uses the corpus's default Intl profile".into());
        }
        let path = performance::command(args.collect())?;
        println!("performance report: {}", path.display());
        return Ok(());
    }

    if command == "differential" {
        return handle_differential_command(args.collect());
    }
    if command == "intl" {
        return intl_data::command(args.collect(), intl_profile);
    }

    let engine = Engine::new(
        RealmBuilder::new()
            .with_host_hooks(Box::new(StdoutHostHooks { output: stdout }))
            .build(),
    );
    match command.as_str() {
        "run" => {
            let ParsedRunArgs {
                backend,
                path,
                force_module,
            } = parse_run_args(&args.collect::<Vec<_>>())?;
            if intl_profile.is_some() && backend != ExecutionBackend::WasmAot {
                return Err("--intl-profile requires the Wasm execution backend".into());
            }
            let path = path.ok_or_else(|| "run needs a source file".to_string())?;
            let source = read_source(&path)?;
            // Goal selection is the product's only module entry point: `.mjs`
            // by extension, or `--module` for a module written to any other
            // name. Everything else stays a Script.
            let is_module = force_module || is_module_path(&path);
            let options = CompileOptions {
                filename: Some(path),
                host_surface_policy,
                intl_profile: intl_profile.unwrap_or_default(),
                ..CompileOptions::default()
            };
            let run_options = RunOptions {
                backend,
                ..RunOptions::default()
            };
            if is_module {
                engine.run_module(&source, options, run_options)
            } else {
                engine.run_script(&source, options, run_options)
            }
            .map(|outcome| println!("run outcome: {:?}", outcome))
            .map_err(|err| err.to_string())
        }
        "cache" => handle_cache_command(args.collect()),
        "build" => {
            let format = args
                .next()
                .ok_or_else(|| "build needs target: wasm".to_string())?;
            if format != "wasm" {
                return Err(format!("unknown build target: {format}"));
            }
            let path = args
                .next()
                .ok_or_else(|| "build needs a source file".to_string())?;
            let source = read_source(&path)?;
            let unit = engine
                .compile_script(
                    &source,
                    CompileOptions {
                        filename: Some(path.clone()),
                        host_surface_policy,
                        intl_profile: intl_profile.unwrap_or_default(),
                        ..CompileOptions::default()
                    },
                )
                .map_err(|err| err.to_string())?;
            engine
                .emit_wasm(&unit)
                .map(|artifact| {
                    // `SCRIPTW_WASM_DUMP` receives the program module; a linked
                    // program's runtime module goes beside it. Runtime-free
                    // replacements remove any prior runtime sidecar.
                    if let Some(path) = std::env::var_os("SCRIPTW_WASM_DUMP") {
                        fs::write(&path, &artifact.bytes).unwrap_or_else(|err| {
                            panic!("failed to write SCRIPTW_WASM_DUMP artifact: {err}");
                        });
                        let mut runtime_path = path.clone();
                        runtime_path.push(".runtime.wasm");
                        if let Some(runtime) = &artifact.runtime {
                            fs::write(&runtime_path, runtime.bytes()).unwrap_or_else(|err| {
                                panic!("failed to write SCRIPTW_WASM_DUMP runtime artifact: {err}");
                            });
                        } else if let Err(err) = fs::remove_file(&runtime_path) {
                            if err.kind() != std::io::ErrorKind::NotFound {
                                panic!("failed to remove stale SCRIPTW_WASM_DUMP runtime artifact: {err}");
                            }
                        }
                    }
                    if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                        eprintln!("scriptw wasm trace: artifact bytes: {}", artifact.bytes.len());
                        if let Some(runtime) = &artifact.runtime {
                            eprintln!(
                                "scriptw wasm trace: runtime bytes: {}",
                                runtime.bytes().len()
                            );
                        }
                    }
                    println!("built Wasm artifact: {}", artifact.description)
                })
                .map_err(|err| err.to_string())
        }
        "types" | "typegen" => handle_types_command(args.collect()),
        "test262" => handle_test262_command(args.collect()),
        "inspect" => {
            let path = args
                .next()
                .ok_or_else(|| "inspect needs a source file".to_string())?;
            let source = read_source(&path)?;
            let goal = if is_module_path(&path) {
                "module"
            } else {
                "script"
            };
            let unit = if goal == "module" {
                engine.compile_module(
                    &source,
                    CompileOptions {
                        filename: Some(path),
                        host_surface_policy,
                        ..CompileOptions::default()
                    },
                )
            } else {
                engine.compile_script(
                    &source,
                    CompileOptions {
                        filename: Some(path),
                        host_surface_policy,
                        ..CompileOptions::default()
                    },
                )
            }
            .map_err(|err| err.to_string())?;
            let report = engine.inspect(&unit);
            println!("goal: {:?}", report.goal);
            println!("source_len: {}", report.source_len);
            println!("stages: {}", report.stages.join(", "));
            println!("invariants: {}", report.invariants.join(", "));
            println!("ir: {}", report.ir_summary);
            if !report.diagnostics.is_empty() {
                println!("diagnostics:");
                for diagnostic in report.diagnostics {
                    println!("  {diagnostic}");
                }
            }
            Ok(())
        }
        _ => Err(format!("unknown command: {command}\n\n{}", usage())),
    }
}

fn handle_differential_command(args: Vec<String>) -> Result<(), String> {
    let Some(subcommand) = args.first() else {
        return Err(format!(
            "differential needs a `replay`, `replay-corpus`, `replay-scenario-pair`, `seed-test262`, `replay-test262`, `generate-arithmetic`, `campaign`, `robustness`, `replay-robustness` or `minimize-robustness` subcommand\n\n{}",
            usage()
        ));
    };
    match subcommand.as_str() {
        "replay" => {
            let parsed = parse_differential_replay_args(&args[1..])?;
            let input = DifferentialReplayInput::load(&parsed.case_path)
                .map_err(|error| error.to_string())?;
            let runner = differential_worker_runner(parsed.worker_bin)?;
            let report =
                replay_case(&input, parsed.oracle, &runner).map_err(|error| error.to_string())?;
            finish_differential_report(report)
        }
        "replay-corpus" => differential_corpus::run(&args[1..]),
        "replay-scenario-pair" => differential_scenario_pair::run(&args[1..]),
        "seed-test262" => differential_test262::seed(&args[1..]),
        "replay-test262" => differential_test262::replay(&args[1..]),
        "robustness" => differential_robustness::run(&args[1..]),
        "replay-robustness" => differential_robustness::replay(&args[1..]),
        "minimize-robustness" => differential_robustness::minimize(&args[1..]),
        "generate-arithmetic" => handle_differential_generate_arithmetic(&args[1..]),
        "campaign" => differential_campaign::run(&args[1..]),
        "__worker" => handle_differential_worker(&args[1..]),
        _ => Err(format!(
            "unknown differential subcommand: {subcommand}\n\n{}",
            usage()
        )),
    }
}

fn differential_worker_runner(
    worker_bin: Option<PathBuf>,
) -> Result<DifferentialWorkerRunner, String> {
    let executable = match worker_bin {
        Some(executable) => executable,
        None => std::env::current_exe().map_err(|error| {
            format!("cannot locate the differential worker executable: {error}")
        })?,
    };
    match scriptw_engine::configured_compilation_jobs() {
        Some(jobs) => DifferentialWorkerRunner::new_with_compilation_jobs(executable, jobs),
        None => DifferentialWorkerRunner::new(executable),
    }
    .map_err(|error| error.to_string())
}

#[cfg(feature = "spec-exec-oracle")]
fn handle_differential_worker(args: &[String]) -> Result<(), String> {
    let mut request = None;
    let mut journal = None;
    let mut oracle = None;
    let mut index = 0;
    while index < args.len() {
        let option = &args[index];
        let value = args
            .get(index + 1)
            .filter(|value| !value.is_empty() && !value.starts_with('-'))
            .ok_or_else(|| format!("{option} needs a value"))?;
        match option.as_str() {
            "--request" if request.is_none() => request = Some(PathBuf::from(value)),
            "--journal" if journal.is_none() => journal = Some(PathBuf::from(value)),
            "--oracle" if oracle.is_none() => oracle = Some(parse_differential_oracle(value)?),
            "--request" | "--journal" | "--oracle" => {
                return Err(format!("{option} may only be specified once"));
            }
            _ => return Err(format!("unknown differential worker option: {option}")),
        }
        index += 2;
    }
    let request = request.ok_or_else(|| "differential worker needs --request".to_string())?;
    let journal = journal.ok_or_else(|| "differential worker needs --journal".to_string())?;
    let oracle = oracle
        .ok_or_else(|| "differential worker requires explicit --oracle spec-exec".to_string())?;
    scriptw_test262::differential::run_differential_worker(
        &request,
        &journal,
        oracle,
        scriptw_engine::configured_compilation_jobs().unwrap_or(1),
    )
    .map_err(|error| error.to_string())
}

#[cfg(not(feature = "spec-exec-oracle"))]
fn handle_differential_worker(_args: &[String]) -> Result<(), String> {
    Err(scriptw_test262::differential::DifferentialError::OracleNotLinked.to_string())
}

fn finish_differential_report(report: DifferentialReport) -> Result<(), String> {
    let report_json = report.to_pretty_json().map_err(|error| error.to_string())?;
    println!("{report_json}");

    match report.verdict() {
        DifferentialVerdict::BothCompleted
        | DifferentialVerdict::PrimitiveCompletionsMatch
        | DifferentialVerdict::PrimitiveCompletionAndPrintTranscriptMatch
        | DifferentialVerdict::SelectedObjectProbeAndPrintTranscriptMatch
        | DifferentialVerdict::RootedCompletionGraphAndPrintTranscriptMatch => Ok(()),
        DifferentialVerdict::BothFailed => Err(format!(
            "differential case {} failed in both backends; see the JSON observations above",
            report.case_id().as_str()
        )),
        DifferentialVerdict::WorkerFailure => Err(format!(
            "differential case {} had a worker failure; see the JSON observations above",
            report.case_id().as_str()
        )),
        DifferentialVerdict::Mismatch => {
            let signature = report
                .mismatch_signature()
                .map(|signature| signature.as_str())
                .unwrap_or("missing-signature-runner-invariant");
            Err(format!("differential mismatch: {signature}"))
        }
        DifferentialVerdict::ObservationContractViolated => Err(format!(
            "differential case {} violated its {} contract; see the JSON observations above",
            report.case_id().as_str(),
            report.protocol().observation_contract().as_str(),
        )),
    }
}

fn handle_differential_generate_arithmetic(args: &[String]) -> Result<(), String> {
    let parsed = parse_differential_generate_arithmetic_args(args)?;
    if parsed.output_path.exists() {
        return Err(format!(
            "differential generated case output already exists: {}",
            parsed.output_path.display()
        ));
    }
    if let Some(parent) = parsed
        .output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if !parent.is_dir() {
            return Err(format!(
                "differential generated case output directory does not exist: {}",
                parent.display()
            ));
        }
    }

    let runner = differential_worker_runner(parsed.worker_bin)?;
    let outcome = run_generated_arithmetic_campaign(
        parsed.plan,
        parsed.reduction_limit,
        parsed.oracle,
        &runner,
    )
    .map_err(|error| error.to_string())?;
    let (case, report, persist) = match outcome {
        GeneratedArithmeticCampaignOutcome::Verified { case, report }
        | GeneratedArithmeticCampaignOutcome::ReducedMismatch { case, report, .. } => {
            (case, report, true)
        }
        GeneratedArithmeticCampaignOutcome::Rejected { case, report } => (case, report, false),
    };

    if persist {
        let case_json = case.to_pretty_json().map_err(|error| error.to_string())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&parsed.output_path)
            .map_err(|error| {
                format!(
                    "failed to create differential generated case {}: {error}",
                    parsed.output_path.display()
                )
            })?;
        std::io::Write::write_all(&mut output, case_json.as_bytes()).map_err(|error| {
            format!(
                "failed to write differential generated case {}: {error}",
                parsed.output_path.display()
            )
        })?;
    }

    finish_differential_report(report)
}

fn parse_differential_replay_args(args: &[String]) -> Result<ParsedDifferentialReplayArgs, String> {
    let mut case_path = None;
    let mut oracle = None;
    let mut worker_bin = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--worker-bin" => {
                if worker_bin.is_some() {
                    return Err("--worker-bin may only be specified once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.is_empty() && !value.starts_with('-'))
                    .ok_or_else(|| "--worker-bin needs an executable path".to_string())?;
                worker_bin = Some(PathBuf::from(value));
                index += 2;
            }
            "--oracle" => {
                if oracle.is_some() {
                    return Err("--oracle may only be specified once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--oracle needs a value".to_string())?;
                oracle = Some(parse_differential_oracle(value)?);
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown differential replay option: {value}"));
            }
            value => {
                if case_path.is_some() {
                    return Err("differential replay accepts exactly one case path".to_string());
                }
                case_path = Some(PathBuf::from(value));
                index += 1;
            }
        }
    }
    let case_path = case_path
        .ok_or_else(|| "differential replay needs a versioned case JSON path".to_string())?;
    let oracle = oracle.ok_or_else(|| {
        "differential replay requires explicit `--oracle spec-exec`; the oracle is never a default"
            .to_string()
    })?;
    Ok(ParsedDifferentialReplayArgs {
        case_path,
        oracle,
        worker_bin,
    })
}

fn parse_differential_generate_arithmetic_args(
    args: &[String],
) -> Result<ParsedDifferentialGenerateArithmeticArgs, String> {
    let mut output_path = None;
    let mut grammar = None;
    let mut seed = None;
    let mut checks = None;
    let mut depth = None;
    let mut reduction_limit = None;
    let mut oracle = None;
    let mut worker_bin = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--grammar" => {
                if grammar.is_some() {
                    return Err("--grammar may only be specified once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--grammar needs a value".to_string())?;
                grammar =
                    Some(ArithmeticGrammar::from_name(value).map_err(|error| error.to_string())?);
                index += 2;
            }
            "--seed" => {
                if seed.is_some() {
                    return Err("--seed may only be specified once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--seed needs a value".to_string())?;
                seed = Some(ArithmeticGenerationSeed::new(
                    value
                        .parse::<u64>()
                        .map_err(|_| "--seed must be an unsigned 64-bit integer".to_string())?,
                ));
                index += 2;
            }
            "--checks" => {
                if checks.is_some() {
                    return Err("--checks may only be specified once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--checks needs a value".to_string())?;
                checks = Some(
                    ArithmeticCheckCount::new(
                        value
                            .parse::<usize>()
                            .map_err(|_| "--checks must be an unsigned integer".to_string())?,
                    )
                    .map_err(|error| error.to_string())?,
                );
                index += 2;
            }
            "--depth" => {
                if depth.is_some() {
                    return Err("--depth may only be specified once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--depth needs a value".to_string())?;
                depth = Some(
                    ArithmeticExpressionDepth::new(
                        value
                            .parse::<usize>()
                            .map_err(|_| "--depth must be an unsigned integer".to_string())?,
                    )
                    .map_err(|error| error.to_string())?,
                );
                index += 2;
            }
            "--max-replays" => {
                if reduction_limit.is_some() {
                    return Err("--max-replays may only be specified once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--max-replays needs a value".to_string())?;
                reduction_limit =
                    Some(
                        ArithmeticReductionLimit::new(value.parse::<usize>().map_err(|_| {
                            "--max-replays must be an unsigned integer".to_string()
                        })?)
                        .map_err(|error| error.to_string())?,
                    );
                index += 2;
            }
            "--worker-bin" => {
                if worker_bin.is_some() {
                    return Err("--worker-bin may only be specified once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.is_empty() && !value.starts_with('-'))
                    .ok_or_else(|| "--worker-bin needs an executable path".to_string())?;
                worker_bin = Some(PathBuf::from(value));
                index += 2;
            }
            "--oracle" => {
                if oracle.is_some() {
                    return Err("--oracle may only be specified once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--oracle needs a value".to_string())?;
                oracle = Some(parse_differential_oracle(value)?);
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(format!(
                    "unknown differential generate-arithmetic option: {value}"
                ));
            }
            value => {
                if output_path.is_some() {
                    return Err(
                        "differential generate-arithmetic accepts exactly one output path"
                            .to_string(),
                    );
                }
                output_path = Some(PathBuf::from(value));
                index += 1;
            }
        }
    }

    let output_path = output_path.ok_or_else(|| {
        "differential generate-arithmetic needs an output case JSON path".to_string()
    })?;
    let seed = seed.ok_or_else(|| "differential generate-arithmetic needs --seed N".to_string())?;
    let checks =
        checks.ok_or_else(|| "differential generate-arithmetic needs --checks N".to_string())?;
    let depth =
        depth.ok_or_else(|| "differential generate-arithmetic needs --depth N".to_string())?;
    let reduction_limit = reduction_limit
        .ok_or_else(|| "differential generate-arithmetic needs --max-replays N".to_string())?;
    let oracle = oracle.ok_or_else(|| {
        "differential generate-arithmetic requires explicit `--oracle spec-exec`; the oracle is never a default"
            .to_string()
    })?;

    Ok(ParsedDifferentialGenerateArithmeticArgs {
        output_path,
        plan: ArithmeticGenerationPlan::new(
            grammar.unwrap_or(ArithmeticGrammar::IntegerBitwiseV2),
            seed,
            checks,
            depth,
        ),
        reduction_limit,
        oracle,
        worker_bin,
    })
}

fn parse_differential_oracle(value: &str) -> Result<SpecExecOracle, String> {
    match value {
        "spec-exec" => Ok(SpecExecOracle::explicitly_enabled()),
        value => Err(format!(
            "unknown differential oracle: {value} (expected spec-exec)"
        )),
    }
}

fn handle_cache_command(args: Vec<String>) -> Result<(), String> {
    let command = args.first().map(String::as_str).unwrap_or("status");
    match command {
        "status" if args.len() == 1 || args.is_empty() => {
            let status = cache_status().map_err(|err| format!("cache status failed: {err}"))?;
            print_cache_directory("function-cache", &status.function_cache);
            print_cache_directory("module-cache", &status.module_cache);
            print_cache_directory("program-cache", &status.program_cache);
            print_cache_directory("legacy-wasmtime-cache", &status.legacy_wasmtime_cache);
            println!(
                "scriptw-cache-total-bytes: {}",
                status
                    .function_cache
                    .bytes
                    .saturating_add(status.module_cache.bytes)
                    .saturating_add(status.program_cache.bytes)
            );
            // Derived from the reported per-tier limits rather than hardcoded,
            // so `SCRIPTW_CACHE_LIMIT_BYTES` is reflected here instead of the
            // line claiming 2 GiB while the tiers are honouring an override.
            println!(
                "scriptw-cache-total-limit-bytes: {}",
                [
                    &status.function_cache,
                    &status.module_cache,
                    &status.program_cache,
                ]
                .into_iter()
                .filter_map(|directory| directory.limit_bytes)
                .fold(0_u64, u64::saturating_add)
            );
            Ok(())
        }
        "prune" => {
            let include_legacy = match args.get(1).map(String::as_str) {
                None => false,
                Some("--legacy-wasmtime") if args.len() == 2 => true,
                Some(value) => return Err(format!("unknown cache prune arg: {value}")),
            };
            let report =
                prune_caches(include_legacy).map_err(|err| format!("cache prune failed: {err}"))?;
            println!("scriptw-files-removed: {}", report.scriptw_files_removed);
            println!("scriptw-bytes-removed: {}", report.scriptw_bytes_removed);
            println!("legacy-files-removed: {}", report.legacy_files_removed);
            println!("legacy-bytes-removed: {}", report.legacy_bytes_removed);
            if !include_legacy {
                println!(
                    "legacy-wasmtime-cache: retained (pass --legacy-wasmtime to remove explicitly)"
                );
            }
            Ok(())
        }
        value => Err(format!(
            "unknown cache command: {value} (expected status or prune)"
        )),
    }
}

fn print_cache_directory(label: &str, status: &scriptw_engine::CacheDirectoryStatus) {
    println!("{label}-path: {}", status.path.display());
    println!("{label}-exists: {}", status.exists);
    println!("{label}-files: {}", status.files);
    println!("{label}-bytes: {}", status.bytes);
    if let Some(limit) = status.limit_bytes {
        println!("{label}-limit-bytes: {limit}");
    }
}

struct GlobalOptions {
    jobs: Option<usize>,
    host_surface_policy: HostSurfacePolicy,
    intl_profile: Option<IntlCompilationProfile>,
}

#[cfg(test)]
fn take_global_jobs(args: &mut Vec<String>) -> Result<Option<usize>, String> {
    take_global_options(args).map(|options| options.jobs)
}

#[cfg(test)]
fn take_global_host_surface_policy(args: &mut Vec<String>) -> Result<HostSurfacePolicy, String> {
    take_global_options(args).map(|options| options.host_surface_policy)
}

#[cfg(test)]
fn take_global_intl_profile(
    args: &mut Vec<String>,
) -> Result<Option<IntlCompilationProfile>, String> {
    take_global_options(args).map(|options| options.intl_profile)
}

enum IntlLocaleProjectionComponent {
    List,
    RelativeTime,
    DisplayNames,
    Duration,
    Number,
    DateTime,
    Collator,
    Segmenter,
}

impl IntlLocaleProjectionComponent {
    fn parse(argument: &str) -> Option<Self> {
        match argument {
            "--intl-list-locales" => Some(Self::List),
            "--intl-relative-time-locales" => Some(Self::RelativeTime),
            "--intl-displaynames-locales" => Some(Self::DisplayNames),
            "--intl-duration-locales" => Some(Self::Duration),
            "--intl-number-locales" => Some(Self::Number),
            "--intl-datetime-locales" => Some(Self::DateTime),
            "--intl-collator-locales" => Some(Self::Collator),
            "--intl-segmenter-locales" => Some(Self::Segmenter),
            _ => None,
        }
    }
}

fn take_global_options(args: &mut Vec<String>) -> Result<GlobalOptions, String> {
    let mut jobs = None;
    let mut policy = None;
    let mut remaining = Vec::with_capacity(args.len());
    let mut profile = None;
    let mut manifest = None;
    let mut list_locales = None;
    let mut relative_time_locales = None;
    let mut display_names_locales = None;
    let mut duration_locales = None;
    let mut number_locales = None;
    let mut date_time_locales = None;
    let mut collator_locales = None;
    let mut segmenter_locales = None;
    let mut cursor = ArgumentCursor::new(args);
    while let Some(argument) = cursor.next() {
        if argument == "--jobs" {
            if jobs.is_some() {
                return Err("--jobs may only be specified once".into());
            }
            let value = cursor
                .value("--jobs needs a value")?
                .parse::<usize>()
                .map_err(|error| format!("invalid --jobs value: {error}"))?;
            if value == 0 {
                return Err("--jobs must be a positive integer".into());
            }
            jobs = Some(value);
            continue;
        }
        if argument == "--host-surface" {
            if policy.is_some() {
                return Err("--host-surface may only be specified once".into());
            }
            policy = Some(match cursor.value("--host-surface needs a value")? {
                "product" => HostSurfacePolicy::Product,
                "test262" => HostSurfacePolicy::Test262,
                value => return Err(format!("unknown --host-surface value: {value}")),
            });
            continue;
        }
        if argument == "--intl-manifest" {
            if manifest.is_some() {
                return Err("--intl-manifest may only be specified once".into());
            }
            let value = cursor.value("--intl-manifest needs a path")?;
            if value.is_empty() || value.starts_with("--") {
                return Err("--intl-manifest needs a path".into());
            }
            manifest = Some(PathBuf::from(value));
            continue;
        }
        if let Some(component) = IntlLocaleProjectionComponent::parse(argument) {
            let flag = argument;
            let locales = match component {
                IntlLocaleProjectionComponent::List => &mut list_locales,
                IntlLocaleProjectionComponent::RelativeTime => &mut relative_time_locales,
                IntlLocaleProjectionComponent::DisplayNames => &mut display_names_locales,
                IntlLocaleProjectionComponent::Duration => &mut duration_locales,
                IntlLocaleProjectionComponent::Number => &mut number_locales,
                IntlLocaleProjectionComponent::DateTime => &mut date_time_locales,
                IntlLocaleProjectionComponent::Collator => &mut collator_locales,
                IntlLocaleProjectionComponent::Segmenter => &mut segmenter_locales,
            };
            if locales.is_some() {
                return Err(format!("{flag} may only be specified once"));
            }
            let value = cursor.value(&format!("{flag} needs a value"))?;
            if value.starts_with("--") {
                return Err(format!("{flag} needs a value"));
            }
            *locales = Some(value.split(',').map(str::to_owned).collect::<Vec<_>>());
            continue;
        }
        if argument != "--intl-profile" {
            remaining.push(argument.to_owned());
            continue;
        }
        if profile.is_some() {
            return Err("--intl-profile may only be specified once".into());
        }
        let value = cursor.value("--intl-profile needs a value")?;
        profile = Some(if value == "minimal" {
            IntlCompilationProfile::Minimal
        } else if let Some(id) = value.strip_prefix("custom:") {
            IntlCompilationProfile::Custom(
                CustomProfileId::parse(id)
                    .map_err(|error| format!("invalid --intl-profile Custom identity: {error}"))?,
            )
        } else if value == "conformance" {
            IntlCompilationProfile::Conformance
        } else {
            return Err(format!(
                "unknown --intl-profile value: {value} (expected minimal, conformance or custom:ID)"
            ));
        });
    }
    if jobs.is_none() {
        jobs = match std::env::var("SCRIPTW_JOBS") {
            Ok(value) => Some(
                value
                    .parse::<std::num::NonZeroUsize>()
                    .map_err(|_| "SCRIPTW_JOBS must be a positive integer".to_string())?
                    .get(),
            ),
            Err(std::env::VarError::NotPresent) => None,
            Err(error) => return Err(error.to_string()),
        };
    }
    *args = remaining;
    let intl_profile = (|| -> Result<Option<IntlCompilationProfile>, String> {
        if let Some(path) = manifest {
            if profile.is_some()
                || list_locales.is_some()
                || relative_time_locales.is_some()
                || display_names_locales.is_some()
                || duration_locales.is_some()
                || number_locales.is_some()
                || date_time_locales.is_some()
                || collator_locales.is_some()
                || segmenter_locales.is_some()
            {
                return Err(
                "--intl-manifest cannot be combined with --intl-profile or component locale flags"
                    .into(),
            );
            }
            use std::io::Read as _;
            let file = std::fs::File::open(&path).map_err(|error| {
                format!("cannot read --intl-manifest {}: {error}", path.display())
            })?;
            let mut bytes = Vec::new();
            file.take(CustomIntlProfile::MANIFEST_MAX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| {
                    format!("cannot read --intl-manifest {}: {error}", path.display())
                })?;
            if bytes.len() > CustomIntlProfile::MANIFEST_MAX_BYTES {
                return Err("--intl-manifest exceeds 256 KiB".into());
            }
            let json = std::str::from_utf8(&bytes)
                .map_err(|error| format!("--intl-manifest must be UTF-8: {error}"))?;
            let selection = CustomIntlProfile::from_manifest_json(json)
                .map_err(|error| format!("invalid --intl-manifest {}: {error}", path.display()))?;
            return Ok(Some(IntlCompilationProfile::CustomProjection(selection)));
        }
        match (profile, list_locales, relative_time_locales, display_names_locales, duration_locales, number_locales, date_time_locales, collator_locales, segmenter_locales) {
        (profile, None, None, None, None, None, None, None, None) => Ok(profile),
        (Some(IntlCompilationProfile::Custom(id)), lists, relative_times, display_names, durations, numbers, date_times, collators, segmenters) => {
            let lists = lists
                .as_ref()
                .map(|locales| locales.iter().map(String::as_str).collect::<Vec<_>>());
            let relative_times = relative_times
                .as_ref()
                .map(|locales| locales.iter().map(String::as_str).collect::<Vec<_>>());
            let display_names = display_names
                .as_ref()
                .map(|locales| locales.iter().map(String::as_str).collect::<Vec<_>>());
            let durations = durations
                .as_ref()
                .map(|locales| locales.iter().map(String::as_str).collect::<Vec<_>>());
            let numbers = numbers
                .as_ref()
                .map(|locales| locales.iter().map(String::as_str).collect::<Vec<_>>());
            let date_times = date_times
                .as_ref()
                .map(|locales| locales.iter().map(String::as_str).collect::<Vec<_>>());
            let collators = collators.as_ref()
                .map(|locales| locales.iter().map(String::as_str).collect::<Vec<_>>());
            let segmenters = segmenters.as_ref()
                .map(|locales| locales.iter().map(String::as_str).collect::<Vec<_>>());
            let selection = CustomIntlProfile::new(
                id, lists.as_deref(), relative_times.as_deref(), display_names.as_deref(), durations.as_deref(),
                numbers.as_deref(), date_times.as_deref(), collators.as_deref(),
            segmenters.as_deref(),
)
            .map_err(|error| format!("invalid --intl-list-locales/--intl-relative-time-locales/--intl-displaynames-locales/--intl-duration-locales/--intl-number-locales/--intl-datetime-locales/--intl-collator-locales/--intl-segmenter-locales selection: {error}"))?;
            Ok(Some(IntlCompilationProfile::CustomProjection(selection)))
        }
        _ => Err(
            "--intl-list-locales, --intl-relative-time-locales, --intl-displaynames-locales, --intl-duration-locales, --intl-number-locales, --intl-datetime-locales, --intl-collator-locales and --intl-segmenter-locales require --intl-profile custom:ID"
                .into(),
        ),
    }
    })()?;
    Ok(GlobalOptions {
        jobs,
        host_surface_policy: policy.unwrap_or_default(),
        intl_profile,
    })
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_readme_path() -> PathBuf {
    repo_root().join("README.md")
}

fn fake_suite_config() -> Result<SuiteConfig, String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("scriptw-test262")
        .join("tests")
        .join("fixtures")
        .join("fake_test262");
    let mut config = SuiteConfig::default();
    config.suite_root = root.join("vendor/test262");
    config.local_harness = LocalHarnessSource::EmbeddedWasmAot;
    config.snapshot_dir = repo_root().join("target/test262-scratch/publication-fake");
    config.case_runner_bin = Some(
        std::env::current_exe()
            .map_err(|error| format!("cannot select the current Test262 case worker: {error}"))?,
    );
    Ok(config)
}

fn current_utc_date_string() -> Result<String, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("system clock before unix epoch: {err}"))?;
    let days = (now.as_secs() / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}

fn sorted_kind_counts(counts: &BTreeMap<FailureKind, usize>) -> Vec<PublishedCountEntry> {
    let mut entries = FailureKind::ordered_counts(counts)
        .map(|(kind, count)| PublishedCountEntry {
            label: kind.as_str().to_string(),
            count,
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.cmp(&right.label))
    });
    entries
}

fn sorted_outcome_counts(counts: &BTreeMap<OutcomeKind, usize>) -> Vec<PublishedCountEntry> {
    OutcomeKind::ordered_counts(counts)
        .map(|(outcome, count)| PublishedCountEntry {
            label: outcome.as_str().to_string(),
            count,
        })
        .collect()
}

fn sorted_origin_counts(counts: &BTreeMap<FailureOrigin, usize>) -> Vec<PublishedCountEntry> {
    let mut entries = FailureOrigin::ordered_counts(counts)
        .map(|(origin, count)| PublishedCountEntry {
            label: origin.as_str().to_string(),
            count,
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.cmp(&right.label))
    });
    entries
}

fn top_target_entries(summary: &VerifiedAggregateSummary) -> Vec<PublishedTargetEntry> {
    let mut entries = summary
        .summary
        .entries
        .iter()
        .filter(|entry| entry.failed > 0)
        .map(|entry| PublishedTargetEntry {
            filter: entry.filter.clone(),
            passed: entry.passed,
            total: entry.total,
            failed: entry.failed,
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .failed
            .cmp(&left.failed)
            .then_with(|| left.filter.cmp(&right.filter))
    });
    entries.truncate(5);
    entries
}

fn build_published_status_artifact(
    fake_counts: &VerifiedFakeSuiteCounts,
    summary: &VerifiedAggregateSummary,
    publication_backend: PublicationBackend,
    refresh_date: &str,
) -> PublishedStatusArtifact {
    let totals = OutcomeTotals::new(
        &summary.summary.counts_per_outcome,
        OutcomeCoverage::Complete,
    );
    PublishedStatusArtifact {
        producer: ArtifactProducer::CURRENT,
        fake_wasm_safe: fake_counts.wasm_safe().clone(),
        fake_full: fake_counts.full().clone(),
        real_suite: PublishedRealSuiteStatus {
            backend: publication_backend.as_str().to_string(),
            refresh_date: refresh_date.to_string(),
            manifest_hash: summary.manifest_hash,
            compiler_identity: summary.compiler_identity.clone(),
            passed: summary.summary.passed,
            total: summary.summary.total,
            failed: summary.summary.failed,
            pinned_revisions: PublishedPinnedRevisions {
                ecma262: summary.pinned_revisions.ecma262.clone(),
                test262: summary.pinned_revisions.test262.clone(),
            },
            counts_per_outcome: sorted_outcome_counts(&summary.summary.counts_per_outcome),
            counts_per_kind: sorted_kind_counts(&summary.summary.counts_per_kind),
            counts_per_origin: sorted_origin_counts(&summary.summary.counts_per_origin),
            top_targets: top_target_entries(summary),
            snapshot_paths: PublishedSnapshotPaths {
                json: summary.snapshot_paths.json_path.display().to_string(),
                txt: summary.snapshot_paths.txt_path.display().to_string(),
            },
            goal: PublishedRealSuiteGoal {
                name: "Full pinned Test262 green".to_string(),
                denominator: "pinned-suite-total".to_string(),
                target_total: summary.summary.total,
                current_success: totals.success,
                remaining_to_green: totals.remaining_to_green(),
                pass_rate: percent_string(totals.success, summary.summary.total),
                outcome_targets: vec![
                    PublishedCountEntry {
                        label: OutcomeKind::NotImplemented.as_str().to_string(),
                        count: 0,
                    },
                    PublishedCountEntry {
                        label: OutcomeKind::Crash.as_str().to_string(),
                        count: 0,
                    },
                    PublishedCountEntry {
                        label: OutcomeKind::Bug.as_str().to_string(),
                        count: 0,
                    },
                ],
            },
        },
    }
}

fn render_published_status_text(artifact: &PublishedStatusArtifact) -> String {
    let mut out = String::new();
    let real = &artifact.real_suite;
    out.push_str("published real Test262 status\n");
    out.push_str(&format!("producer={}\n", artifact.producer.as_str()));
    out.push_str(&format!("refresh_date={}\n", real.refresh_date));
    out.push_str(&format!("execution_backend={}\n", real.backend));
    out.push_str(&format!("manifest_hash={}\n", real.manifest_hash));
    out.push_str(&format!(
        "compiler_identity={}\n",
        serde_json::to_string(&real.compiler_identity)
            .expect("checked compiler identity serializes to JSON")
    ));
    out.push_str(&format!(
        "pinned: ecma262={} test262={}\n",
        real.pinned_revisions.ecma262, real.pinned_revisions.test262
    ));
    out.push_str(&format!(
        "fake_wasm_safe={}/{}\n",
        artifact.fake_wasm_safe.passed, artifact.fake_wasm_safe.total
    ));
    out.push_str(&format!(
        "fake_full={}/{}\n",
        artifact.fake_full.passed, artifact.fake_full.total
    ));
    out.push_str(&format!("real_total={}\n", real.total));
    out.push_str(&format!("real_passed={}\n", real.passed));
    out.push_str(&format!("real_failed={}\n", real.failed));
    out.push_str(&format!("goal={}\n", real.goal.name));
    out.push_str(&format!(
        "progress={}/{}\n",
        real.goal.current_success, real.goal.target_total
    ));
    out.push_str(&format!(
        "remaining_to_green={}\n",
        real.goal.remaining_to_green
    ));
    out.push_str(&format!(
        "burn_down: NotImplemented={} Crash={} Bug={}\n",
        outcome_count(&real.counts_per_outcome, OutcomeKind::NotImplemented),
        outcome_count(&real.counts_per_outcome, OutcomeKind::Crash),
        outcome_count(&real.counts_per_outcome, OutcomeKind::Bug)
    ));
    out.push_str(&format!("snapshot_json={}\n", real.snapshot_paths.json));
    out.push_str(&format!("snapshot_txt={}\n", real.snapshot_paths.txt));
    out.push_str("outcomes:\n");
    for entry in &real.counts_per_outcome {
        out.push_str(&format!("  {}={}\n", entry.label, entry.count));
    }
    out.push_str("failure_kinds:\n");
    for entry in &real.counts_per_kind {
        out.push_str(&format!("  {}={}\n", entry.label, entry.count));
    }
    out.push_str("failure_origins:\n");
    for entry in &real.counts_per_origin {
        out.push_str(&format!("  {}={}\n", entry.label, entry.count));
    }
    out.push_str("top_targets:\n");
    if real.top_targets.is_empty() {
        out.push_str("  none\n");
    } else {
        for entry in &real.top_targets {
            out.push_str(&format!(
                "  {}: {}/{} passed (failed {})\n",
                entry.filter, entry.passed, entry.total, entry.failed
            ));
        }
    }
    out
}

fn write_published_status_artifact(
    snapshot_dir: &Path,
    publication_backend: PublicationBackend,
    artifact: &PublishedStatusArtifact,
) -> Result<PublishedStatusPaths, String> {
    let json = serde_json::to_vec_pretty(artifact)
        .map_err(|err| format!("failed to encode published status json: {err}"))?;
    let text = render_published_status_text(artifact);
    scriptw_test262::publish_status_generation(
        snapshot_dir,
        publication_backend,
        &json,
        text.as_bytes(),
    )
    .map_err(|err| {
        format!(
            "failed to publish status generation in {}: {err}",
            snapshot_dir.display()
        )
    })
}

fn percent_string(passed: usize, total: usize) -> String {
    if total == 0 {
        return "0.0%".to_string();
    }
    format!("{:.1}%", (passed as f64 * 100.0) / total as f64)
}

fn outcome_count(entries: &[PublishedCountEntry], outcome: OutcomeKind) -> usize {
    entries
        .iter()
        .find(|entry| entry.label == outcome.as_str())
        .map(|entry| entry.count)
        .unwrap_or(0)
}

fn render_current_status_block(real: &PublishedRealSuiteStatus) -> String {
    format!(
        "## Current Status\n<!-- scriptw-status:start -->\nPublished Test262 (`{}`, `{}`): `{}/{}` passed; `{}` failed.\nNative-host conformance includes cases unsupported by standalone Wasm; it does not establish current HEAD coverage.\nPinned revisions: `ecma262={}` `test262={}`\nCompiler build fingerprint: `{}`; executing image SHA-256: `{}`\nPublished status artifacts: `{}` and `{}`\n[Full test details](docs/rust-rewrite/test262-current.md) · [Refresh workflow](CONTRIBUTING.md#test262).\n<!-- scriptw-status:end -->",
        real.backend,
        real.refresh_date,
        real.passed,
        real.total,
        real.failed,
        real.pinned_revisions.ecma262,
        real.pinned_revisions.test262,
        real.compiler_identity.identity().source_fingerprint(),
        real.compiler_identity.identity().executable_sha256(),
        real.snapshot_paths.json,
        real.snapshot_paths.txt,
    )
}

fn rewrite_current_status_block(
    readme_path: &Path,
    artifact: &PublishedStatusArtifact,
) -> Result<(), String> {
    let raw = fs::read_to_string(readme_path)
        .map_err(|err| format!("failed to read {}: {err}", readme_path.display()))?;
    let replacement = render_current_status_block(&artifact.real_suite);
    let updated =
        status_publication::replace_status_section(&raw, &replacement).map_err(|error| {
            format!(
                "invalid status section in {}: {error}",
                readme_path.display()
            )
        })?;
    scriptw_test262::atomic_write(readme_path, updated.as_bytes())
        .map_err(|err| format!("failed to write {}: {err}", readme_path.display()))
}

const DEFAULT_TYPE_OUTPUT: &str = "worker-configuration.d.ts";
const DEFAULT_CONFIG_NAMES: &[&str] = &[
    "wrangler.jsonc",
    "wrangler.json",
    "wrangler.toml",
    "scriptw.jsonc",
    "scriptw.json",
    "scriptw.toml",
];
const TYPEGEN_BINDING_SPECS: &[(&[&str], &str)] = &[
    (&["kv_namespaces"], "KVNamespace"),
    (&["r2_buckets"], "R2Bucket"),
    (&["d1_databases"], "D1Database"),
    (&["durable_objects", "bindings"], "DurableObjectNamespace"),
    (&["services"], "Fetcher"),
    (&["queues", "producers"], "Queue"),
    (&["analytics_engine_datasets"], "AnalyticsEngineDataset"),
    (&["vectorize"], "VectorizeIndex"),
    (&["ai_search"], "AiSearch"),
    (&["ai_search_namespaces"], "AiSearchNamespace"),
    (&["mtls_certificates"], "Fetcher"),
    (&["browser"], "BrowserRendering"),
    (&["images"], "ImagesBinding"),
    (&["hyperdrive"], "Hyperdrive"),
    (&["workflows"], "Workflow"),
    (&["pipelines"], "Pipeline"),
    (&["dispatch_namespaces"], "DispatchNamespace"),
    (&["send_email"], "SendEmail"),
];
const TYPEGEN_SINGLETON_BINDING_SPECS: &[(&[&str], &str)] = &[
    (&["ai"], "Ai"),
    (&["version_metadata"], "WorkerVersionMetadata"),
    (&["assets"], "Fetcher"),
];
const TYPEGEN_KNOWN_HANDLERS: &[&str] = &[
    "fetch",
    "scheduled",
    "queue",
    "email",
    "tail",
    "trace",
    "alarm",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypegenOptions {
    config_paths: Vec<String>,
    cwd: PathBuf,
    env: Option<String>,
    env_interface: String,
    include_runtime: bool,
    include_env: bool,
    strict_vars: bool,
    check: bool,
    print: bool,
    entrypoint: Option<String>,
    entrypoint_explicit: bool,
    output: String,
    help: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypegenEntrypointInfo {
    path: String,
    syntax: String,
    handlers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypegenBinding {
    name: String,
    types: BTreeSet<String>,
    optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypegenInputs {
    configs: Vec<Value>,
    config_paths: Vec<PathBuf>,
    entrypoint_info: Option<TypegenEntrypointInfo>,
    bindings: Vec<TypegenBinding>,
}

fn typegen_usage() -> &'static str {
    "Usage: scriptw types [entrypoint] [worker-configuration.d.ts] [options]

Generate Wrangler-style TypeScript runtime and Env declarations from a worker config.

Options:
  --config, -c <path>          Path to wrangler/scriptw config; can be repeated
  --entrypoint <path>          Worker entrypoint when not set by config main
  --env, -e <name>             Generate only one named environment
  --env-interface <name>       Global env interface name (default: Env)
  --include-runtime=<bool>     Include minimal runtime declarations (default: true)
  --include-env=<bool>         Include env declarations (default: true)
  --strict-vars=<bool>         Preserve literal var types (default: true)
  --check                      Exit non-zero if the output file is stale
  --print                      Print generated declarations instead of writing
  --cwd <path>                 Resolve config and output paths from this directory
"
}

fn handle_types_command(args: Vec<String>) -> Result<(), String> {
    let options = parse_typegen_args(&args)?;
    if options.help {
        print!("{}", typegen_usage());
        return Ok(());
    }

    let inputs = load_typegen_inputs(&options)?;
    let output = render_typegen_declarations(&inputs, &options);
    let output_path = resolve_from(&options.cwd, &options.output);

    if options.print {
        print!("{output}");
        return Ok(());
    }

    if options.check {
        let current = fs::read_to_string(&output_path).unwrap_or_default();
        if current != output {
            return Err(format!(
                "{} is out of date; run scriptw types",
                options.output
            ));
        }
        println!("Types are up to date at {}", options.output);
        return Ok(());
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs::write(&output_path, output)
        .map_err(|err| format!("failed to write {}: {err}", output_path.display()))?;
    println!("Types written to {}", options.output);
    Ok(())
}

fn parse_typegen_args(args: &[String]) -> Result<TypegenOptions, String> {
    let mut options = TypegenOptions {
        config_paths: Vec::new(),
        cwd: std::env::current_dir().map_err(|err| format!("failed to read cwd: {err}"))?,
        env: None,
        env_interface: "Env".to_string(),
        include_runtime: true,
        include_env: true,
        strict_vars: true,
        check: false,
        print: false,
        entrypoint: None,
        entrypoint_explicit: false,
        output: String::new(),
        help: false,
    };
    let mut positionals = Vec::new();

    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if !is_typegen_flag(arg) {
            positionals.push(arg.clone());
            index += 1;
            continue;
        }

        let (flag_name, inline_value) = split_flag_value(arg);
        match flag_name {
            "--help" | "-h" => options.help = true,
            "--config" | "-c" => {
                let (value, next) = read_typegen_option_value(args, index, arg, flag_name)?;
                options.config_paths.push(value);
                index = next;
            }
            "--cwd" => {
                let (value, next) = read_typegen_option_value(args, index, arg, flag_name)?;
                options.cwd = PathBuf::from(value);
                index = next;
            }
            "--env" | "-e" => {
                let (value, next) = read_typegen_option_value(args, index, arg, flag_name)?;
                options.env = Some(value);
                index = next;
            }
            "--env-interface" => {
                let (value, next) = read_typegen_option_value(args, index, arg, flag_name)?;
                if !is_ts_identifier(&value) {
                    return Err(format!("Invalid TypeScript interface name: {value}"));
                }
                options.env_interface = value;
                index = next;
            }
            "--entrypoint" | "--entry" | "--main" => {
                let (value, next) = read_typegen_option_value(args, index, arg, flag_name)?;
                options.entrypoint = Some(value);
                options.entrypoint_explicit = true;
                index = next;
            }
            "--out" | "--output" | "-o" => {
                let (value, next) = read_typegen_option_value(args, index, arg, flag_name)?;
                options.output = value;
                index = next;
            }
            "--include-runtime" => {
                options.include_runtime = parse_typegen_bool(inline_value, true)?;
            }
            "--no-include-runtime" => options.include_runtime = false,
            "--include-env" => {
                options.include_env = parse_typegen_bool(inline_value, true)?;
            }
            "--no-include-env" => options.include_env = false,
            "--strict-vars" => {
                options.strict_vars = parse_typegen_bool(inline_value, true)?;
            }
            "--no-strict-vars" => options.strict_vars = false,
            "--check" => {
                options.check = parse_typegen_bool(inline_value, true)?;
            }
            "--print" => {
                options.print = parse_typegen_bool(inline_value, true)?;
            }
            _ => return Err(format!("Unknown types option: {flag_name}")),
        }
        index += 1;
    }

    for positional in positionals {
        if positional.ends_with(".d.ts") {
            if !options.output.is_empty() {
                return Err(format!(
                    "Multiple output paths were provided: {}, {}",
                    options.output, positional
                ));
            }
            options.output = positional;
            continue;
        }

        if options.entrypoint.is_none() {
            options.entrypoint = Some(positional);
            options.entrypoint_explicit = true;
            continue;
        }

        if options.output.is_empty() {
            options.output = positional;
            continue;
        }

        return Err(format!("Unexpected positional argument: {positional}"));
    }

    if options.output.is_empty() {
        options.output = DEFAULT_TYPE_OUTPUT.to_string();
    }
    if !options.output.ends_with(".d.ts") {
        return Err(format!(
            "Type output path must end in .d.ts: {}",
            options.output
        ));
    }

    Ok(options)
}

fn is_typegen_flag(value: &str) -> bool {
    value.starts_with('-')
}

fn split_flag_value(arg: &str) -> (&str, Option<&str>) {
    arg.split_once('=')
        .map(|(flag, value)| (flag, Some(value)))
        .unwrap_or((arg, None))
}

fn read_typegen_option_value(
    args: &[String],
    index: usize,
    raw_arg: &str,
    flag_name: &str,
) -> Result<(String, usize), String> {
    if let Some((_, value)) = raw_arg.split_once('=') {
        return Ok((value.to_string(), index));
    }
    let next_index = index + 1;
    let value = args
        .get(next_index)
        .ok_or_else(|| format!("{flag_name} expects a value"))?;
    if is_typegen_flag(value) {
        return Err(format!("{flag_name} expects a value"));
    }
    Ok((value.clone(), next_index))
}

fn parse_typegen_bool(value: Option<&str>, fallback: bool) -> Result<bool, String> {
    let Some(value) = value else {
        return Ok(fallback);
    };
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!("Expected a boolean value, got {value}")),
    }
}

fn resolve_from(base: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn discover_typegen_config(cwd: &Path) -> Vec<PathBuf> {
    DEFAULT_CONFIG_NAMES
        .iter()
        .map(|name| cwd.join(name))
        .find(|candidate| candidate.exists())
        .into_iter()
        .collect()
}

fn load_typegen_inputs(options: &TypegenOptions) -> Result<TypegenInputs, String> {
    let config_paths = if options.config_paths.is_empty() {
        discover_typegen_config(&options.cwd)
    } else {
        options
            .config_paths
            .iter()
            .map(|config_path| resolve_from(&options.cwd, config_path))
            .collect()
    };
    let configs = config_paths
        .iter()
        .map(|config_path| read_typegen_config(config_path))
        .collect::<Result<Vec<_>, _>>()?;
    let config_dir = config_paths
        .first()
        .and_then(|path| path.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| options.cwd.clone());
    let config_main = configs
        .iter()
        .find_map(|config| config.get("main").and_then(Value::as_str))
        .map(ToOwned::to_owned);
    let entrypoint = options.entrypoint.clone().or(config_main);
    let entrypoint_base = if options.entrypoint.is_some() {
        &options.cwd
    } else {
        &config_dir
    };
    let entrypoint_info = read_typegen_entrypoint(
        entrypoint.as_deref(),
        entrypoint_base,
        options.entrypoint_explicit,
    )?;
    let bindings = collect_typegen_bindings(&configs, options);

    Ok(TypegenInputs {
        configs,
        config_paths,
        entrypoint_info,
        bindings,
    })
}

fn read_typegen_config(path: &Path) -> Result<Value, String> {
    let source = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        parse_typegen_toml(&source, path)
    } else {
        parse_typegen_json_like(&source, path)
    }
}

fn parse_typegen_json_like(source: &str, path: &Path) -> Result<Value, String> {
    serde_json::from_str(
        &jsonc::normalize(source)
            .map_err(|err| format!("Failed to parse {}: {err}", path.display()))?,
    )
    .map_err(|err| format!("Failed to parse {}: {err}", path.display()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TomlTarget {
    ObjectPath(Vec<String>),
    ArrayLast(Vec<String>),
}

fn parse_typegen_toml(source: &str, path: &Path) -> Result<Value, String> {
    let mut root = Value::Object(Map::new());
    let mut current = TomlTarget::ObjectPath(Vec::new());

    for raw_line in source.lines() {
        let line = strip_toml_comment(raw_line).trim().to_string();
        if line.is_empty() {
            continue;
        }

        if line.starts_with("[[") && line.ends_with("]]") {
            let parts = split_toml_path(line[2..line.len() - 2].trim());
            push_toml_array_table(&mut root, &parts)
                .map_err(|err| format!("Failed to parse {}: {err}", path.display()))?;
            current = TomlTarget::ArrayLast(parts);
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let parts = split_toml_path(line[1..line.len() - 1].trim());
            ensure_value_object_path(&mut root, &parts)
                .map_err(|err| format!("Failed to parse {}: {err}", path.display()))?;
            current = TomlTarget::ObjectPath(parts);
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(format!(
                "Failed to parse {}: Invalid TOML line: {line}",
                path.display()
            ));
        };
        let key_parts = split_toml_path(key.trim());
        let value = parse_toml_value(value.trim())
            .map_err(|err| format!("Failed to parse {}: {err}", path.display()))?;
        set_toml_current_value(&mut root, &current, &key_parts, value)
            .map_err(|err| format!("Failed to parse {}: {err}", path.display()))?;
    }

    Ok(root)
}

fn strip_toml_comment(line: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut quote = '\0';
    let mut escaped = false;
    for ch in line.chars() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = false;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = true;
            quote = ch;
            out.push(ch);
            continue;
        }
        if ch == '#' {
            break;
        }
        out.push(ch);
    }
    out
}

fn split_toml(value: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut quote = '\0';
    let mut escaped = false;

    for ch in value.chars() {
        if in_string {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = false;
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_string = true;
            quote = ch;
            current.push(ch);
            continue;
        }

        if ch == '[' || ch == '{' {
            depth += 1;
        }
        if ch == ']' || ch == '}' {
            depth -= 1;
        }

        if ch == delimiter && depth == 0 {
            parts.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }

    if !current.trim().is_empty() || value.ends_with(delimiter) {
        parts.push(current.trim().to_string());
    }
    parts
}

fn split_toml_path(value: &str) -> Vec<String> {
    split_toml(value, '.')
        .into_iter()
        .map(|part| part.trim().trim_matches('"').trim_matches('\'').to_string())
        .collect()
}

fn parse_toml_value(raw: &str) -> Result<Value, String> {
    let value = raw.trim();
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        if value.starts_with('"') {
            return serde_json::from_str(value)
                .map_err(|err| format!("Invalid TOML string {value}: {err}"));
        }
        return Ok(Value::String(value[1..value.len() - 1].to_string()));
    }
    if value == "true" {
        return Ok(Value::Bool(true));
    }
    if value == "false" {
        return Ok(Value::Bool(false));
    }
    if let Ok(number) = value.parse::<i64>() {
        return Ok(Value::Number(number.into()));
    }
    if let Ok(number) = value.parse::<f64>() {
        if let Some(number) = serde_json::Number::from_f64(number) {
            return Ok(Value::Number(number));
        }
    }
    if value.starts_with('[') && value.ends_with(']') {
        let inner = value[1..value.len() - 1].trim();
        if inner.is_empty() {
            return Ok(Value::Array(Vec::new()));
        }
        return split_toml(inner, ',')
            .into_iter()
            .map(|part| parse_toml_value(&part))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array);
    }
    if value.starts_with('{') && value.ends_with('}') {
        let mut out = Value::Object(Map::new());
        let inner = value[1..value.len() - 1].trim();
        if inner.is_empty() {
            return Ok(out);
        }
        for part in split_toml(inner, ',') {
            let Some((key, item_value)) = part.split_once('=') else {
                return Err(format!("Invalid inline TOML table entry: {part}"));
            };
            let key_parts = split_toml_path(key.trim());
            let item_value = parse_toml_value(item_value.trim())?;
            set_nested_value(&mut out, &key_parts, item_value)?;
        }
        return Ok(out);
    }
    Ok(Value::String(value.to_string()))
}

fn ensure_value_object_path<'a>(
    root: &'a mut Value,
    parts: &[String],
) -> Result<&'a mut Map<String, Value>, String> {
    if !root.is_object() {
        *root = Value::Object(Map::new());
    }

    let mut current = root;
    for part in parts {
        if !current.is_object() {
            *current = Value::Object(Map::new());
        }
        let object = current.as_object_mut().expect("value should be object");
        current = object
            .entry(part.clone())
            .or_insert_with(|| Value::Object(Map::new()));
    }

    if !current.is_object() {
        *current = Value::Object(Map::new());
    }
    current
        .as_object_mut()
        .ok_or_else(|| "expected TOML object".to_string())
}

fn set_nested_value(root: &mut Value, parts: &[String], value: Value) -> Result<(), String> {
    let Some((last, parent_parts)) = parts.split_last() else {
        return Err("empty TOML key".to_string());
    };
    let parent = ensure_value_object_path(root, parent_parts)?;
    parent.insert(last.clone(), value);
    Ok(())
}

fn push_toml_array_table(root: &mut Value, parts: &[String]) -> Result<(), String> {
    let Some((last, parent_parts)) = parts.split_last() else {
        return Err("empty TOML array table".to_string());
    };
    let parent = ensure_value_object_path(root, parent_parts)?;
    let entry = parent
        .entry(last.clone())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !entry.is_array() {
        *entry = Value::Array(Vec::new());
    }
    entry
        .as_array_mut()
        .ok_or_else(|| "expected TOML array".to_string())?
        .push(Value::Object(Map::new()));
    Ok(())
}

fn get_nested_array_mut<'a>(root: &'a mut Value, parts: &[String]) -> Option<&'a mut Vec<Value>> {
    let mut current = root;
    for part in parts {
        current = current.as_object_mut()?.get_mut(part)?;
    }
    current.as_array_mut()
}

fn set_toml_current_value(
    root: &mut Value,
    current: &TomlTarget,
    key_parts: &[String],
    value: Value,
) -> Result<(), String> {
    match current {
        TomlTarget::ObjectPath(path) => {
            let mut parts = path.clone();
            parts.extend_from_slice(key_parts);
            set_nested_value(root, &parts, value)
        }
        TomlTarget::ArrayLast(path) => {
            let array = get_nested_array_mut(root, path)
                .ok_or_else(|| "current TOML array table is missing".to_string())?;
            let last = array
                .last_mut()
                .ok_or_else(|| "current TOML array table is empty".to_string())?;
            set_nested_value(last, key_parts, value)
        }
    }
}

fn get_nested_value<'a>(value: &'a Value, parts: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for part in parts {
        current = current.get(*part)?;
    }
    Some(current)
}

fn clone_without_env(config: &Value) -> Value {
    let Some(object) = config.as_object() else {
        return Value::Object(Map::new());
    };
    Value::Object(
        object
            .iter()
            .filter(|(key, _)| key.as_str() != "env")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

fn merge_config_value(base: &Value, override_value: Option<&Value>) -> Value {
    let Some(override_value) = override_value else {
        return base.clone();
    };
    match (base.as_object(), override_value.as_object()) {
        (Some(base_object), Some(override_object)) => {
            let mut out = base_object.clone();
            for (key, value) in override_object {
                let merged = out
                    .get(key)
                    .map(|base_value| merge_config_value(base_value, Some(value)))
                    .unwrap_or_else(|| value.clone());
                out.insert(key.clone(), merged);
            }
            Value::Object(out)
        }
        _ => override_value.clone(),
    }
}

fn arrayish_values(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(Value::Null) | None => Vec::new(),
        Some(value) => vec![value],
    }
}

fn add_typegen_binding(
    bindings: &mut BTreeMap<String, TypegenBinding>,
    name: Option<&str>,
    type_name: &str,
    optional: bool,
) {
    let Some(name) = name.filter(|name| !name.is_empty()) else {
        return;
    };
    let entry = bindings
        .entry(name.to_string())
        .or_insert_with(|| TypegenBinding {
            name: name.to_string(),
            types: BTreeSet::new(),
            optional: true,
        });
    entry.types.insert(type_name.to_string());
    entry.optional &= optional;
}

fn extract_typegen_bindings_from_config(
    config: &Value,
    bindings: &mut BTreeMap<String, TypegenBinding>,
    optional: bool,
    strict_vars: bool,
) {
    if let Some(vars) = config.get("vars").and_then(Value::as_object) {
        for (name, value) in vars {
            add_typegen_binding(
                bindings,
                Some(name),
                &if strict_vars {
                    typegen_literal_type(value)
                } else {
                    typegen_loose_var_type(value)
                },
                optional,
            );
        }
    }

    for name in arrayish_values(get_nested_value(config, &["secrets", "required"])) {
        add_typegen_binding(bindings, name.as_str(), "string", optional);
    }

    for (parts, type_name) in TYPEGEN_BINDING_SPECS {
        for item in arrayish_values(get_nested_value(config, parts)) {
            let binding = item
                .get("binding")
                .and_then(Value::as_str)
                .or_else(|| item.get("name").and_then(Value::as_str));
            add_typegen_binding(bindings, binding, type_name, optional);
        }
    }

    for (parts, type_name) in TYPEGEN_SINGLETON_BINDING_SPECS {
        let binding = get_nested_value(config, parts).and_then(|item| {
            item.get("binding")
                .and_then(Value::as_str)
                .or_else(|| item.get("name").and_then(Value::as_str))
        });
        add_typegen_binding(bindings, binding, type_name, optional);
    }

    for item in arrayish_values(get_nested_value(config, &["unsafe", "bindings"])) {
        let binding = item
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| item.get("binding").and_then(Value::as_str));
        add_typegen_binding(bindings, binding, "unknown", optional);
    }
}

fn collect_typegen_bindings(configs: &[Value], options: &TypegenOptions) -> Vec<TypegenBinding> {
    let mut bindings = BTreeMap::new();
    for config in configs {
        let base = clone_without_env(config);
        if let Some(env_name) = &options.env {
            let env_config = config.get("env").and_then(|env| env.get(env_name));
            let merged = merge_config_value(&base, env_config);
            extract_typegen_bindings_from_config(
                &merged,
                &mut bindings,
                false,
                options.strict_vars,
            );
            continue;
        }

        extract_typegen_bindings_from_config(&base, &mut bindings, false, options.strict_vars);
        if let Some(envs) = config.get("env").and_then(Value::as_object) {
            for env_config in envs.values() {
                extract_typegen_bindings_from_config(
                    env_config,
                    &mut bindings,
                    true,
                    options.strict_vars,
                );
            }
        }
    }
    bindings.into_values().collect()
}

fn read_typegen_entrypoint(
    entrypoint: Option<&str>,
    base_dir: &Path,
    explicit: bool,
) -> Result<Option<TypegenEntrypointInfo>, String> {
    let Some(entrypoint) = entrypoint else {
        return Ok(None);
    };
    let resolved = resolve_from(base_dir, entrypoint);
    if !resolved.exists() {
        if explicit {
            return Err(format!("Entrypoint does not exist: {entrypoint}"));
        }
        return Ok(Some(TypegenEntrypointInfo {
            path: entrypoint.to_string(),
            syntax: "unknown".to_string(),
            handlers: Vec::new(),
        }));
    }

    let source = fs::read_to_string(&resolved)
        .map_err(|err| format!("failed to read entrypoint {}: {err}", resolved.display()))?;
    let service_worker = TYPEGEN_KNOWN_HANDLERS
        .iter()
        .take(5)
        .any(|handler| source_has_add_event_listener(&source, handler));
    let has_default_export = source.contains("export default");
    let handlers = TYPEGEN_KNOWN_HANDLERS
        .iter()
        .copied()
        .filter(|handler| source_has_handler(&source, handler))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let syntax = if service_worker {
        "service-worker"
    } else if has_default_export {
        "module"
    } else {
        "unknown"
    };

    Ok(Some(TypegenEntrypointInfo {
        path: entrypoint.to_string(),
        syntax: syntax.to_string(),
        handlers,
    }))
}

fn source_has_add_event_listener(source: &str, event: &str) -> bool {
    [
        format!("addEventListener(\"{event}\""),
        format!("addEventListener('{event}'"),
        format!("addEventListener (\"{event}\""),
        format!("addEventListener ('{event}'"),
    ]
    .iter()
    .any(|needle| source.contains(needle))
}

fn source_has_handler(source: &str, handler: &str) -> bool {
    for (index, _) in source.match_indices(handler) {
        let before = source[..index].chars().next_back();
        if before.is_some_and(is_identifier_continue) {
            continue;
        }
        let after_index = index + handler.len();
        let mut chars = source[after_index..].chars();
        let first = chars.next();
        if first.is_some_and(is_identifier_continue) {
            continue;
        }
        let next = first
            .into_iter()
            .chain(chars)
            .find(|ch| !ch.is_whitespace());
        if matches!(next, Some('(' | ':')) {
            return true;
        }
    }
    false
}

fn render_typegen_declarations(inputs: &TypegenInputs, options: &TypegenOptions) -> String {
    let mut lines = Vec::new();
    let config_list = if inputs.config_paths.is_empty() {
        "none".to_string()
    } else {
        inputs
            .config_paths
            .iter()
            .map(|path| relative_for_comment(&options.cwd, path))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let entrypoint_label = inputs
        .entrypoint_info
        .as_ref()
        .map(|entrypoint| entrypoint.path.as_str())
        .unwrap_or("none");

    lines.push(
        "// Generated by Scriptw. Regenerate with `scriptw types` after config changes."
            .to_string(),
    );
    lines.push(format!("// Config: {config_list}"));
    lines.push(format!("// Entrypoint: {entrypoint_label}"));
    if let Some(env) = &options.env {
        lines.push(format!("// Environment: {env}"));
    }
    if let Some(entrypoint) = &inputs.entrypoint_info {
        lines.push(format!("// Entrypoint syntax: {}", entrypoint.syntax));
        if !entrypoint.handlers.is_empty() {
            lines.push(format!(
                "// Detected handlers: {}",
                entrypoint.handlers.join(", ")
            ));
        }
    }
    lines.push(String::new());

    if options.include_runtime {
        lines.push(typegen_runtime_declarations().trim_end().to_string());
        lines.push(String::new());
    }

    if options.include_env {
        lines.push("declare namespace Scriptw {".to_string());
        lines.push(format!("  interface {} {{", options.env_interface));
        if inputs.bindings.is_empty() {
            lines.push("    // No bindings were found in the provided config.".to_string());
        } else {
            for binding in &inputs.bindings {
                lines.push(format!(
                    "    {}{}: {};",
                    typegen_prop_name(&binding.name),
                    if binding.optional { "?" } else { "" },
                    format_typegen_binding_type(binding)
                ));
            }
        }
        lines.push("  }".to_string());

        let compatibility_date = inputs
            .configs
            .iter()
            .find_map(|config| config.get("compatibility_date"));
        let compatibility_flags = inputs
            .configs
            .iter()
            .flat_map(|config| arrayish_values(config.get("compatibility_flags")))
            .collect::<Vec<_>>();
        if compatibility_date.is_some()
            || !compatibility_flags.is_empty()
            || inputs.entrypoint_info.is_some()
        {
            lines.push(String::new());
            lines.push("  interface WorkerConfiguration {".to_string());
            if let Some(compatibility_date) = compatibility_date {
                lines.push(format!(
                    "    compatibilityDate: {};",
                    typegen_literal_type(compatibility_date)
                ));
            }
            if !compatibility_flags.is_empty() {
                let flag_types = compatibility_flags
                    .iter()
                    .map(|flag| typegen_literal_type(flag))
                    .collect::<Vec<_>>();
                lines.push(format!(
                    "    compatibilityFlags: readonly ({})[];",
                    flag_types.join(" | ")
                ));
            }
            if let Some(entrypoint) = &inputs.entrypoint_info {
                lines.push(format!(
                    "    entrypoint: {};",
                    typegen_ts_string(&entrypoint.path)
                ));
                lines.push(format!(
                    "    syntax: {};",
                    typegen_ts_string(&entrypoint.syntax)
                ));
            }
            lines.push("  }".to_string());
        }

        lines.push("}".to_string());
        lines.push(String::new());
        lines.push(format!(
            "interface {} extends Scriptw.{} {{}}",
            options.env_interface, options.env_interface
        ));
        lines.push(String::new());
    }

    format!("{}\n", lines.join("\n").trim_end())
}

fn typegen_runtime_declarations() -> &'static str {
    r#"declare interface ExecutionContext {
  waitUntil(promise: Promise<unknown>): void;
  passThroughOnException(): void;
}

declare interface ScheduledController {
  readonly scheduledTime: number;
  readonly cron: string;
  noRetry(): void;
}

declare interface Message<Body = unknown> {
  readonly id: string;
  readonly timestamp: Date;
  readonly body: Body;
  ack(): void;
  retry(options?: QueueRetryOptions): void;
}

declare interface MessageBatch<Body = unknown> {
  readonly queue: string;
  readonly messages: readonly Message<Body>[];
  ackAll(): void;
  retryAll(options?: QueueRetryOptions): void;
}

declare interface QueueRetryOptions {
  delaySeconds?: number;
}

declare interface ExportedHandler<Env = unknown> {
  fetch?(request: Request, env: Env, ctx: ExecutionContext): Response | Promise<Response>;
  scheduled?(controller: ScheduledController, env: Env, ctx: ExecutionContext): void | Promise<void>;
  queue?(batch: MessageBatch, env: Env, ctx: ExecutionContext): void | Promise<void>;
  email?(message: unknown, env: Env, ctx: ExecutionContext): void | Promise<void>;
  tail?(events: readonly unknown[], env: Env, ctx: ExecutionContext): void | Promise<void>;
  trace?(traces: readonly unknown[], env: Env, ctx: ExecutionContext): void | Promise<void>;
  alarm?(controller: unknown, env: Env, ctx: ExecutionContext): void | Promise<void>;
}

declare interface Fetcher {
  fetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response>;
}

declare interface KVNamespace {
  get(key: string, options?: unknown): Promise<unknown>;
  put(key: string, value: unknown, options?: unknown): Promise<void>;
  delete(key: string): Promise<void>;
  list(options?: unknown): Promise<unknown>;
}

declare interface R2Bucket {
  get(key: string, options?: unknown): Promise<unknown>;
  put(key: string, value: unknown, options?: unknown): Promise<unknown>;
  delete(keys: string | readonly string[]): Promise<void>;
  list(options?: unknown): Promise<unknown>;
}

declare interface D1PreparedStatement {
  bind(...values: unknown[]): D1PreparedStatement;
  first<T = unknown>(columnName?: string): Promise<T | null>;
  run<T = unknown>(): Promise<T>;
  all<T = unknown>(): Promise<T>;
  raw<T = unknown>(): Promise<T[]>;
}

declare interface D1Database {
  prepare(query: string): D1PreparedStatement;
  batch<T = unknown>(statements: readonly D1PreparedStatement[]): Promise<T[]>;
  exec(query: string): Promise<unknown>;
  dump(): Promise<ArrayBuffer>;
}

declare interface DurableObjectId {
  readonly name?: string;
  toString(): string;
}

declare interface DurableObjectStub {
  fetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response>;
}

declare interface DurableObjectNamespace {
  newUniqueId(options?: unknown): DurableObjectId;
  idFromName(name: string): DurableObjectId;
  idFromString(id: string): DurableObjectId;
  get(id: DurableObjectId): DurableObjectStub;
}

declare interface Queue<Body = unknown> {
  send(message: Body, options?: unknown): Promise<void>;
  sendBatch(messages: readonly { body: Body; options?: unknown }[]): Promise<void>;
}

declare interface AnalyticsEngineDataset {
  writeDataPoint(event: unknown): void;
}

declare interface VectorizeIndex {
  query(vector: readonly number[], options?: unknown): Promise<unknown>;
  insert(vectors: readonly unknown[]): Promise<unknown>;
  upsert(vectors: readonly unknown[]): Promise<unknown>;
  deleteByIds(ids: readonly string[]): Promise<unknown>;
}

declare interface Ai {
  run(model: string, inputs: unknown, options?: unknown): Promise<unknown>;
}

declare interface AiSearch {
  search(query: unknown, options?: unknown): Promise<unknown>;
}

declare interface AiSearchNamespace {
  search(query: unknown, options?: unknown): Promise<unknown>;
}

declare interface BrowserRendering {
  launch(options?: unknown): Promise<unknown>;
}

declare interface ImagesBinding {
  input(value: unknown): unknown;
}

declare interface Hyperdrive {
  readonly connectionString: string;
}

declare interface Workflow {
  create(options?: unknown): Promise<unknown>;
  get(id: string): Promise<unknown>;
}

declare interface Pipeline {
  send(records: readonly unknown[]): Promise<void>;
}

declare interface DispatchNamespace {
  get(name: string, args?: unknown): Fetcher;
}

declare interface SendEmail {
  send(message: unknown): Promise<void>;
}
"#
}

fn relative_for_comment(cwd: &Path, path: &Path) -> String {
    path.strip_prefix(cwd)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn typegen_prop_name(name: &str) -> String {
    if is_ts_identifier(name) {
        name.to_string()
    } else {
        typegen_ts_string(name)
    }
}

fn typegen_literal_type(value: &Value) -> String {
    match value {
        Value::String(value) => typegen_ts_string(value),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(values) => {
            let types = values
                .iter()
                .map(typegen_literal_type)
                .collect::<BTreeSet<_>>();
            if types.is_empty() {
                "readonly unknown[]".to_string()
            } else {
                format!(
                    "readonly ({})[]",
                    types.into_iter().collect::<Vec<_>>().join(" | ")
                )
            }
        }
        Value::Object(_) => "unknown".to_string(),
    }
}

fn typegen_loose_var_type(value: &Value) -> String {
    match value {
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
        Value::Array(_) => "readonly unknown[]",
        Value::Object(_) => "Record<string, unknown>",
    }
    .to_string()
}

fn format_typegen_binding_type(binding: &TypegenBinding) -> String {
    if binding.types.len() == 1 {
        binding.types.iter().next().cloned().unwrap_or_default()
    } else {
        binding
            .types
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn typegen_ts_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("\"{}\"", value.replace('"', "\\\"")))
}

fn is_ts_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    is_identifier_start(first) && chars.all(is_identifier_continue)
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit()
}

enum Test262VerdictCommand {
    Run,
    RunList,
    Shard,
}

impl Test262VerdictCommand {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::RunList => "run-list",
            Self::Shard => "shard",
        }
    }
}

fn require_passing_test262_verdict(
    command: Test262VerdictCommand,
    verdict: ConformanceRunVerdict,
) -> Result<(), String> {
    let command = command.as_str();
    match verdict {
        ConformanceRunVerdict::NoEvidence => Err(format!(
            "test262 {command} produced no verdict: zero cases were selected"
        )),
        ConformanceRunVerdict::Passed { .. } => Ok(()),
        ConformanceRunVerdict::Failed { total, failed } => Err(format!(
            "test262 {command} failed: {} of {} cases did not pass",
            failed.get(),
            total.get()
        )),
    }
}

mod conformance_closure;

fn handle_test262_command(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err(format!("test262 needs a subcommand\n\n{}", usage()));
    }

    if args[0] == "close-release" {
        return conformance_closure::run(&args[1..]);
    }

    if args[0] == "__case-worker-server-v1" {
        let parsed = parse_test262_case_worker_args(&args[1..])?;
        return scriptw_test262::run_case_worker_server(parsed.config, parsed.run_config);
    }

    if args[0] == "__case-worker" {
        let parsed = parse_test262_case_worker_args(&args[1..])?;
        let summary = scriptw_test262::run_case_worker(parsed.config, parsed.run_config)?;
        return require_passing_test262_verdict(Test262VerdictCommand::Run, summary.verdict()?);
    }

    let subcommand = Test262Command::parse(&args[0])?;
    let parsed = parse_test262_args(&args[1..])?;
    execute_test262_command(subcommand.admit(parsed)?)
}

fn execute_test262_command(input: AdmittedTest262Command) -> Result<(), String> {
    let (subcommand, mut parsed) = input.into_parts();
    let execution_list = if subcommand == Test262Command::RunList {
        let path = parsed
            .filter
            .take()
            .ok_or("test262 run-list requires an execution-list file")?;
        parsed.run_config.filter = None;
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read execution list {path}: {error}"))?;
        Some(scriptw_test262::TestExecutionList::parse(&source)?)
    } else {
        None
    };
    let runner = ConformanceRunner::with_config(parsed.config);
    let execution_backend = parsed.run_config.execution_backend;

    match subcommand {
        Test262Command::Sync => {
            let pinned = runner.pinned_revisions();
            if !runner.config().suite_root.join("test").exists() {
                return Err(format!(
                    "vendored suite missing at {}",
                    runner.config().suite_root.display()
                ));
            }
            println!("suite_root: {}", runner.config().suite_root.display());
            println!("test262 revision: {}", pinned.test262);
            println!("execution_backend: {}", execution_backend.as_str());
            println!("local harness: {}", runner.config().local_harness);
            Ok(())
        }
        Test262Command::List | Test262Command::ListAll => {
            let manifest = runner.discover_suite(parsed.filter.as_deref())?;
            println!("count: {}", manifest.cases.len());
            let limit = if subcommand == Test262Command::ListAll {
                manifest.cases.len()
            } else {
                50
            };
            for case in manifest.cases.iter().take(limit) {
                println!("{}", case.execution_id().wire_key());
            }
            if manifest.cases.len() > limit {
                println!("... {} more", manifest.cases.len() - 50);
            }
            Ok(())
        }
        Test262Command::Run | Test262Command::RunList => {
            let verdict_command = if execution_list.is_some() {
                Test262VerdictCommand::RunList
            } else {
                Test262VerdictCommand::Run
            };
            let summary = match execution_list {
                Some(executions) => runner.run_execution_list(parsed.run_config, executions)?,
                None => match parsed.matrix_node.as_deref() {
                    Some(node_selector) => {
                        runner.run_matrix_node(node_selector, parsed.run_config)?
                    }
                    None => runner.run_full(parsed.run_config)?,
                },
            };
            println!("execution_backend: {}", execution_backend.as_str());
            println!("total: {}", summary.total);
            println!("passed: {}", summary.passed);
            print_kind_counts(&summary.counts_per_kind);
            print_outcome_counts(&summary.counts_per_outcome);
            for failure in summary.failures.iter().take(10) {
                println!(
                    "failure: {} [{}:{}] {}",
                    failure.test_path,
                    failure.outcome.as_str(),
                    failure.kind.as_str(),
                    failure.detail
                );
            }
            require_passing_test262_verdict(verdict_command, summary.verdict()?)
        }
        Test262Command::Report => {
            let summary = match parsed.matrix_node.as_deref() {
                Some(node_selector) => runner.run_matrix_node(node_selector, parsed.run_config)?,
                None => runner.run_full(parsed.run_config)?,
            };
            let report = runner.baseline_report(&summary);
            println!("execution_backend: {}", execution_backend.as_str());
            println!("total: {}", report.total);
            println!("passed: {}", report.passed);
            println!("failed: {}", report.failed);
            print_outcome_counts(&summary.counts_per_outcome);
            for bucket in report.buckets {
                println!("bucket: {} ({})", bucket.kind.as_str(), bucket.total);
                for (subtree, count) in bucket.top_subtrees.iter().take(5) {
                    println!("  {subtree}: {count}");
                }
            }
            Ok(())
        }
        Test262Command::ReportAll => {
            let report =
                runner.aggregate_baseline_report(&runner.run_top_level_matrix(parsed.run_config)?);
            println!("execution_backend: {}", execution_backend.as_str());
            println!("total: {}", report.total);
            println!("passed: {}", report.passed);
            println!("failed: {}", report.failed);
            print_kind_counts(&report.counts_per_kind);
            print_outcome_counts(&report.counts_per_outcome);
            print_origin_counts(&report.counts_per_origin);
            println!("targets:");
            for entry in report.entries {
                println!(
                    "  {}: {}/{} passed",
                    entry.filter, entry.passed, entry.total
                );
            }
            Ok(())
        }
        Test262Command::PublishStatus => {
            let publication_backend = PublicationBackend::try_from(execution_backend)?;
            let verified = match runner.load_publishable_aggregate_summary(
                &parsed.run_config.snapshot_name,
                publication_backend,
            ) {
                Ok(verified) => verified,
                Err(err) if err.may_resume() => {
                    let mut resume_run_config = parsed.run_config.clone();
                    resume_run_config.resume = true;
                    runner.run_top_level_matrix(resume_run_config)?;
                    runner.load_publishable_aggregate_summary(
                        &parsed.run_config.snapshot_name,
                        publication_backend,
                    )?
                }
                Err(err) => return Err(err.into()),
            };
            let refresh_date = current_utc_date_string()?;
            let fake_counts = VerifiedFakeSuiteCounts::measure(fake_suite_config()?)?;
            let artifact = build_published_status_artifact(
                &fake_counts,
                &verified,
                publication_backend,
                &refresh_date,
            );
            let status_paths = write_published_status_artifact(
                &runner.config().snapshot_dir,
                publication_backend,
                &artifact,
            )?;
            let readme_path = parsed.readme_path.unwrap_or_else(default_readme_path);
            rewrite_current_status_block(&readme_path, &artifact)?;

            println!("execution_backend: {}", execution_backend.as_str());
            println!("refresh_date: {}", refresh_date);
            println!("total: {}", verified.summary.total);
            println!("passed: {}", verified.summary.passed);
            println!("failed: {}", verified.summary.failed);
            println!("manifest_hash: {}", verified.manifest_hash);
            println!(
                "compiler_identity: {}",
                serde_json::to_string(&verified.compiler_identity)
                    .map_err(|error| format!("cannot render compiler identity: {error}"))?
            );
            println!("pinned_ecma262: {}", verified.pinned_revisions.ecma262);
            println!("pinned_test262: {}", verified.pinned_revisions.test262);
            for entry in &artifact.real_suite.counts_per_kind {
                println!("kind_{}: {}", entry.label, entry.count);
            }
            for entry in &artifact.real_suite.counts_per_outcome {
                println!("outcome_{}: {}", entry.label, entry.count);
            }
            for entry in &artifact.real_suite.counts_per_origin {
                println!("origin_{}: {}", entry.label, entry.count);
            }
            println!(
                "snapshot_json: {}",
                verified.snapshot_paths.json_path.display()
            );
            println!(
                "snapshot_txt: {}",
                verified.snapshot_paths.txt_path.display()
            );
            println!("status_manifest: {}", status_paths.manifest_path.display());
            println!("status_json: {}", status_paths.json_path.display());
            println!("status_txt: {}", status_paths.txt_path.display());
            println!("readme_path: {}", readme_path.display());
            println!("top_targets:");
            if artifact.real_suite.top_targets.is_empty() {
                println!("  none");
            } else {
                for entry in &artifact.real_suite.top_targets {
                    println!(
                        "  {}: {}/{} passed",
                        entry.filter, entry.passed, entry.total
                    );
                }
            }
            Ok(())
        }
        Test262Command::ProgressStatus => {
            let progress = runner.load_aggregate_progress_summary(
                &parsed.run_config.snapshot_name,
                execution_backend,
            )?;
            let totals = OutcomeTotals::new(
                &progress.summary.counts_per_outcome,
                OutcomeCoverage::Partial {
                    observed: progress.summary.total,
                    target: progress.target_total(),
                },
            );
            println!("execution_backend: {}", execution_backend.as_str());
            println!("complete={}", progress.is_complete());
            println!(
                "matrix_nodes_completed: {}",
                progress.matrix_nodes_completed()
            );
            println!("matrix_nodes_total: {}", progress.matrix_nodes_total());
            println!("observed_total: {}", progress.summary.total);
            println!("target_total: {}", progress.target_total());
            println!("unobserved_total: {}", totals.unobserved);
            println!("current_success: {}", totals.success);
            println!(
                "current_success_full: {}/{}",
                totals.success,
                progress.target_total()
            );
            println!(
                "remaining_observed_failures: {}",
                totals.observed_failures()
            );
            println!("remaining_to_green: {}", totals.remaining_to_green());
            println!("manifest_hash: {}", progress.manifest_hash);
            println!(
                "compiler_identity: {}",
                serde_json::to_string(&progress.compiler_identity)
                    .map_err(|error| format!("cannot render compiler identity: {error}"))?
            );
            println!("pinned_ecma262: {}", progress.pinned_revisions.ecma262);
            println!("pinned_test262: {}", progress.pinned_revisions.test262);
            println!(
                "snapshot_json: {}",
                progress.snapshot_paths.json_path.display()
            );
            print_outcome_counts(&progress.summary.counts_per_outcome);
            println!(
                "burn_down: NotImplemented={} Crash={} Bug={}",
                totals.not_implemented, totals.crash, totals.bug
            );
            println!("not_run: {}", totals.unobserved);
            Ok(())
        }
        Test262Command::TriageStatus => {
            let entries = runner
                .load_matrix_triage_entries(&parsed.run_config.snapshot_name, execution_backend)?;
            println!("execution_backend: {}", execution_backend.as_str());
            println!("failing_nodes: {}", entries.len());
            println!("ranking: Crash,Bug,NotImplemented,failed");
            if entries.is_empty() {
                println!("  none");
            } else {
                for entry in entries.iter().take(25) {
                    println!(
                        "node: {} filter={} passed={}/{} failed={} Crash={} Bug={} NotImplemented={}",
                        entry.node_id,
                        entry.filter,
                        entry.passed,
                        entry.total,
                        entry.failed,
                        entry.crash,
                        entry.bug,
                        entry.not_implemented
                    );
                }
            }
            Ok(())
        }
        Test262Command::FailureDetails => {
            let node_selector = parsed.filter.as_deref().ok_or_else(|| {
                "failure-details needs a matrix node id or exact filter".to_string()
            })?;
            let details = runner.load_matrix_failure_details(
                &parsed.run_config.snapshot_name,
                execution_backend,
                node_selector,
            )?;
            println!("execution_backend: {}", execution_backend.as_str());
            println!("node_id: {}", details.node_id);
            println!("filter: {}", details.filter);
            println!("matrix_path: {}", details.matrix_path.join("/"));
            println!(
                "passed: {}/{} failed={}",
                details.passed, details.total, details.failed
            );
            println!("detail_groups: {}", details.groups.len());
            for group in details.groups.iter().take(25) {
                println!(
                    "detail: count={} outcome={} kind={} origin={} hash={}",
                    group.count,
                    group.outcome.as_str(),
                    group.kind.as_str(),
                    group.origin.as_str(),
                    group.detail_hash
                );
                println!("  {}", group.detail);
                for test in &group.representative_tests {
                    println!("  test: {}", test);
                }
            }
            Ok(())
        }
        Test262Command::GenerateBacklog => {
            let (artifact, paths) =
                runner.generate_backlog(&parsed.run_config.snapshot_name, execution_backend)?;
            println!("execution_backend: {}", execution_backend.as_str());
            println!("snapshot_name: {}", artifact.snapshot_name);
            println!("pinned_ecma262: {}", artifact.pinned_revisions.ecma262);
            println!("pinned_test262: {}", artifact.pinned_revisions.test262);
            println!("total: {}", artifact.total);
            println!("passed: {}", artifact.passed);
            println!("failed: {}", artifact.failed);
            println!("records: {}", artifact.records.len());
            println!("backlog_json: {}", paths.json_path.display());
            println!("backlog_txt: {}", paths.txt_path.display());
            println!("by_task:");
            for (task, count) in &artifact.summary_by_task {
                println!("  {task}: {count}");
            }
            Ok(())
        }
        Test262Command::CompareSnapshots => {
            let base_snapshot_name = parsed.filter.as_deref().ok_or_else(|| {
                "compare-snapshots needs a base snapshot name; candidate uses --snapshot-name"
                    .to_string()
            })?;
            let comparison = runner.compare_snapshots(
                base_snapshot_name,
                &parsed.run_config.snapshot_name,
                execution_backend,
            )?;
            println!("execution_backend: {}", execution_backend.as_str());
            println!("base_snapshot: {}", comparison.base_snapshot_name);
            println!("candidate_snapshot: {}", comparison.candidate_snapshot_name);
            println!(
                "base_compiler_identity: {}",
                serde_json::to_string(&comparison.base_compiler_identity)
                    .map_err(|error| format!("cannot render base compiler identity: {error}"))?
            );
            println!(
                "candidate_compiler_identity: {}",
                serde_json::to_string(&comparison.candidate_compiler_identity).map_err(
                    |error| format!("cannot render candidate compiler identity: {error}")
                )?
            );
            println!("pinned_ecma262: {}", comparison.pinned_revisions.ecma262);
            println!("pinned_test262: {}", comparison.pinned_revisions.test262);
            println!("base_total: {}", comparison.base_total);
            println!("candidate_total: {}", comparison.candidate_total);
            println!("added_passes: {}", comparison.added_passes.len());
            for path in comparison.added_passes.iter().take(25) {
                println!("  pass: {path}");
            }
            println!("regressions: {}", comparison.regressions.len());
            for path in comparison.regressions.iter().take(25) {
                println!("  regression: {path}");
            }
            println!(
                "changed_failure_hashes: {}",
                comparison.changed_failure_hashes.len()
            );
            for change in comparison.changed_failure_hashes.iter().take(25) {
                println!(
                    "  hash: {} {:016x}->{:016x}",
                    change.test_id, change.base_hash, change.candidate_hash
                );
            }
            Ok(())
        }
        Test262Command::Shard => {
            let summary = runner.run_shard(parsed.run_config)?;
            println!("execution_backend: {}", execution_backend.as_str());
            println!(
                "shard: {}/{}",
                summary.one_based_shard_index()?,
                summary.shard_count
            );
            println!("total: {}", summary.total);
            println!("passed: {}", summary.passed);
            println!("failed: {}", summary.failures.len());
            for kind in FailureKind::ALL {
                let count = summary
                    .failures
                    .iter()
                    .filter(|failure| failure.kind == kind)
                    .count();
                println!("{}: {}", kind.as_str(), count);
            }
            require_passing_test262_verdict(Test262VerdictCommand::Shard, summary.verdict()?)
        }
    }
}

fn parse_test262_case_worker_args(args: &[String]) -> Result<ParsedTest262Args, String> {
    let mut normal = Vec::new();
    let mut harness = None;
    let mut suite_output = None;
    let mut cursor = ArgumentCursor::new(args);
    while let Some(argument) = cursor.next() {
        match argument {
            "--case-suite-output" => {
                if suite_output.is_some() {
                    return Err("case worker suite output may be specified once".into());
                }
                suite_output = Some(PathBuf::from(
                    cursor.value("case worker suite output requires a path")?,
                ));
            }
            "--case-harness" | "--case-harness-file" => {
                if harness.is_some() {
                    return Err("case worker harness may be specified once".into());
                }
                let selector = argument;
                let value = cursor.value("case worker harness requires a value")?;
                harness = Some(if selector == "--case-harness-file" {
                    LocalHarnessSource::File(PathBuf::from(value))
                } else {
                    match value {
                        "none" => LocalHarnessSource::None,
                        "spec-exec" => LocalHarnessSource::EmbeddedSpecExec,
                        "wasm-aot" => LocalHarnessSource::EmbeddedWasmAot,
                        "wasm-aot-host-only" => LocalHarnessSource::EmbeddedWasmAotHostOnly,
                        _ => return Err(format!("unknown case worker harness: {value}")),
                    }
                });
            }
            _ => {
                normal.push(argument.to_owned());
                if argument.starts_with("--")
                    && !matches!(argument, "--resume" | "--reuse-case-workers")
                {
                    normal.push(
                        cursor
                            .value(&format!("{argument} needs a value"))?
                            .to_owned(),
                    );
                }
            }
        }
    }
    let mut parsed = parse_test262_args(&normal)?;
    if parsed.matrix_node.is_some() || parsed.readme_path.is_some() {
        return Err("case worker accepts only one execution, never a matrix or publication".into());
    }
    parsed.config.local_harness =
        harness.ok_or("case worker requires its exact harness selection")?;
    if let Some(output) = suite_output {
        parsed.config.inherit_case_output_root(output)?;
    }
    Ok(parsed)
}

fn parse_test262_args(args: &[String]) -> Result<ParsedTest262Args, String> {
    let mut config = SuiteConfig::default();
    let mut filter = None::<String>;
    let mut matrix_node = None::<String>;
    let mut run_config = RunConfig::default();
    // CLI-level product default: Wasm-AOT is the only backend whose results
    // count as conformance (AGENTS.md). `scriptw-test262::RunConfig`'s own
    // struct default stays out of this lane's scope; the CLI entry point
    // overrides it here so `scriptw test262 ...` is Wasm-AOT flag-free.
    // `--execution-backend spec-exec` remains available as an explicit,
    // internal/debug-only differential-oracle override (see usage()).
    run_config.execution_backend = ExecutionBackend::WasmAot;
    let mut readme_path = None::<PathBuf>;

    let mut provided_options = BTreeSet::new();
    let mut cursor = ArgumentCursor::new(args);
    while let Some(argument) = cursor.next() {
        if argument.starts_with("--") {
            if !provided_options.insert(argument.to_owned()) {
                return Err(format!("{argument} may only be specified once"));
            }
        }
        match argument {
            "--suite-root" => {
                let value = cursor.value("--suite-root needs a value")?;
                config.suite_root = PathBuf::from(value);
            }
            "--snapshot-dir" => {
                let value = cursor.value("--snapshot-dir needs a value")?;
                config.snapshot_dir = PathBuf::from(value);
            }
            "--threads" => {
                let value = cursor.value("--threads needs a value")?;
                config.worker_count = value
                    .parse::<usize>()
                    .map_err(|err| format!("invalid --threads value {value}: {err}"))?;
            }
            "--timeout-ms" => {
                let value = cursor.value("--timeout-ms needs a value")?;
                config.timeout_ms = value
                    .parse::<u64>()
                    .map_err(|err| format!("invalid --timeout-ms value {value}: {err}"))?;
            }
            "--execution-backend" => {
                let value = cursor.value("--execution-backend needs a value")?;
                run_config.execution_backend = parse_execution_backend(value)?;
            }
            "--resume" => {
                run_config.resume = true;
            }
            "--reuse-case-workers" => {
                config.reuse_case_workers = true;
            }
            "--snapshot-name" => {
                let value = cursor.value("--snapshot-name needs a value")?;
                run_config.snapshot_name = value.to_owned();
            }
            "--matrix-node" => {
                let value = cursor.value("--matrix-node needs a value")?;
                matrix_node = Some(value.to_owned());
            }
            "--max-matrix-nodes" => {
                let value = cursor.value("--max-matrix-nodes needs a value")?;
                let limit = value
                    .parse::<usize>()
                    .map_err(|err| format!("invalid --max-matrix-nodes value {value}: {err}"))?;
                if limit == 0 {
                    return Err("--max-matrix-nodes must be at least 1".to_string());
                }
                run_config.max_matrix_nodes = Some(limit);
            }
            "--readme-path" => {
                let value = cursor.value("--readme-path needs a value")?;
                readme_path = Some(PathBuf::from(value));
            }
            "--shard" => {
                let value = cursor.value("--shard needs a value")?;
                let (index, total) = value.split_once('/').ok_or("--shard needs INDEX/TOTAL")?;
                let parse = |part: &str| {
                    part.parse::<std::num::NonZeroUsize>()
                        .map(std::num::NonZeroUsize::get)
                        .map_err(|_| "--shard needs positive INDEX/TOTAL integers".to_owned())
                };
                let (index, total) = (parse(index)?, parse(total)?);
                if index > total {
                    return Err("--shard INDEX cannot exceed TOTAL".into());
                }
                run_config.shard_index = index - 1;
                run_config.shard_count = total;
            }
            value if !value.starts_with('-') && filter.is_none() => {
                filter = Some(value.to_string());
            }
            value => return Err(format!("unknown test262 arg: {value}")),
        }
    }

    if matrix_node.is_some() && filter.is_some() {
        return Err("--matrix-node cannot be combined with a positional filter".to_string());
    }

    if config.suite_root != PathBuf::from("test262/vendor/test262") {
        let guessed_root = config
            .suite_root
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        if config.snapshot_dir == SuiteConfig::default().snapshot_dir {
            config.snapshot_dir = guessed_root.join("snapshots");
        }
    }

    config.local_harness = match run_config.execution_backend {
        ExecutionBackend::SpecExec => LocalHarnessSource::EmbeddedSpecExec,
        ExecutionBackend::WasmAot => LocalHarnessSource::EmbeddedWasmAot,
    };

    // Every product attempt, including a compile-only negative, runs in the
    // current executable's exact single-case worker entry. The Store's
    // execution deadline remains the inner execution timer; the supervisor
    // owns the whole attempt.
    config.case_runner_bin = Some(
        std::env::current_exe()
            .map_err(|error| format!("cannot select the current Test262 case worker: {error}"))?,
    );

    run_config.filter = filter.clone();

    Ok(ParsedTest262Args {
        provided_options,
        config,
        filter,
        matrix_node,
        run_config,
        readme_path,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedRunArgs {
    backend: ExecutionBackend,
    path: Option<String>,
    /// `--module`: force `ParseGoal::Module` regardless of the file extension.
    force_module: bool,
}

fn parse_run_args(args: &[String]) -> Result<ParsedRunArgs, String> {
    // Product default: Wasm-AOT. `--execution-backend spec-exec` is an
    // explicit, internal/debug-only differential-oracle override (never a
    // silent fallback) — see usage() and ExecutionBackend's docs.
    let mut backend = ExecutionBackend::WasmAot;
    let mut path = None;
    let mut force_module = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--execution-backend" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--execution-backend needs a value".to_string())?;
                backend = parse_execution_backend(value)?;
            }
            "--module" => force_module = true,
            value if !value.starts_with('-') && path.is_none() => {
                path = Some(value.to_string());
            }
            value => return Err(format!("unknown run arg: {value}")),
        }
        index += 1;
    }
    Ok(ParsedRunArgs {
        backend,
        path,
        force_module,
    })
}

fn parse_execution_backend(value: &str) -> Result<ExecutionBackend, String> {
    match value {
        "spec" | "spec-exec" => Ok(ExecutionBackend::SpecExec),
        "wasm" | "wasm-aot" => Ok(ExecutionBackend::WasmAot),
        _ => Err(format!(
            "unknown execution backend: {value} (expected spec or wasm)"
        )),
    }
}

fn read_source(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("failed to read {path}: {err}"))
}

fn is_module_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "mjs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_status_preserves_failures_and_the_published_coverage_boundary() {
        let compiler_identity = CompilerProvenance::current().unwrap();
        let real = PublishedRealSuiteStatus {
            backend: "wasm-aot".into(),
            refresh_date: "2026-10-21".into(),
            manifest_hash: 1,
            compiler_identity: compiler_identity.clone(),
            passed: 102990,
            total: 102994,
            failed: 4,
            pinned_revisions: PublishedPinnedRevisions {
                ecma262: "spec-pin".into(),
                test262: "suite-pin".into(),
            },
            counts_per_outcome: Vec::new(),
            counts_per_kind: Vec::new(),
            counts_per_origin: Vec::new(),
            top_targets: Vec::new(),
            snapshot_paths: PublishedSnapshotPaths {
                json: "published.json".into(),
                txt: "published.txt".into(),
            },
            goal: PublishedRealSuiteGoal {
                name: "Full pinned Test262 green".into(),
                denominator: "pinned-suite-total".into(),
                target_total: 102994,
                current_success: 102990,
                remaining_to_green: 4,
                pass_rate: "100.0%".into(),
                outcome_targets: Vec::new(),
            },
        };
        let rendered = render_current_status_block(&real);
        assert!(rendered.starts_with("## Current Status\n<!-- scriptw-status:start -->\n"));
        assert!(rendered.ends_with("\n<!-- scriptw-status:end -->"));
        assert_eq!(rendered.lines().count(), 9);
        assert!(
            rendered.contains("(`wasm-aot`, `2026-10-21`): `102990/102994` passed; `4` failed.")
        );
        assert!(!rendered.contains("100.0%"));
        assert!(rendered.contains("Native-host conformance includes cases unsupported by standalone Wasm; it does not establish current HEAD coverage."));
        assert!(rendered.contains("`ecma262=spec-pin` `test262=suite-pin`"));
        assert!(rendered.contains(
            &compiler_identity
                .identity()
                .source_fingerprint()
                .to_string()
        ));
        assert!(rendered.contains(&compiler_identity.identity().executable_sha256().to_string()));
        assert!(
            rendered.contains("Published status artifacts: `published.json` and `published.txt`")
        );
        assert!(rendered.contains("docs/rust-rewrite/test262-current.md"));
        assert!(rendered.contains("CONTRIBUTING.md#test262"));
        assert_eq!(
            status_publication::replace_status_section(&rendered, &rendered).unwrap(),
            rendered
        );
    }

    #[test]
    fn manifest_profile_rejects_ambiguous_flags_before_opening_any_input() {
        for extra in [
            vec!["--intl-profile", "custom:x"],
            vec!["--intl-list-locales", "fr"],
            vec!["--intl-manifest", "another-missing.json"],
        ] {
            let mut args = vec![
                "run",
                "missing-source.js",
                "--intl-manifest",
                "missing-manifest.json",
            ]
            .into_iter()
            .chain(extra)
            .map(str::to_owned)
            .collect();
            let error = take_global_intl_profile(&mut args).unwrap_err();
            assert!(
                error.contains("cannot be combined") || error.contains("only be specified once")
            );
            assert!(!error.contains("cannot read"));
        }
        assert!(take_global_intl_profile(&mut vec!["--intl-manifest".into()]).is_err());
    }

    #[test]
    fn global_jobs_are_removed_before_command_parsing() {
        let mut args = vec![
            "run".to_string(),
            "script.js".to_string(),
            "--jobs".to_string(),
            "3".to_string(),
        ];
        assert_eq!(take_global_jobs(&mut args).unwrap(), Some(3));
        assert_eq!(args, vec!["run", "script.js"]);
    }

    #[test]
    fn host_surface_defaults_to_product_and_requires_an_explicit_test262_opt_in() {
        let mut product_args = vec!["run".to_string(), "script.js".to_string()];
        assert_eq!(
            take_global_host_surface_policy(&mut product_args).unwrap(),
            HostSurfacePolicy::Product
        );
        assert_eq!(product_args, vec!["run", "script.js"]);

        let mut test262_args = vec![
            "run".to_string(),
            "script.js".to_string(),
            "--host-surface".to_string(),
            "test262".to_string(),
        ];
        assert_eq!(
            take_global_host_surface_policy(&mut test262_args).unwrap(),
            HostSurfacePolicy::Test262
        );
        assert_eq!(test262_args, vec!["run", "script.js"]);
    }

    #[test]
    fn intl_profile_parser_retains_explicit_selection_and_rejects_invalid_domains() {
        let mut absent = vec!["run".to_string(), "case.js".to_string()];
        assert_eq!(take_global_intl_profile(&mut absent).unwrap(), None);
        for (label, expected) in [
            ("minimal", IntlCompilationProfile::Minimal),
            ("conformance", IntlCompilationProfile::Conformance),
            (
                "custom:release_1.0",
                IntlCompilationProfile::Custom(CustomProfileId::parse("release_1.0").unwrap()),
            ),
        ] {
            let mut args = vec![
                "build".into(),
                "wasm".into(),
                "--intl-profile".into(),
                label.into(),
                "case.js".into(),
            ];
            assert_eq!(take_global_intl_profile(&mut args).unwrap(), Some(expected));
            assert_eq!(args, vec!["build", "wasm", "case.js"]);
        }
        for label in ["custom:", "custom:a/b", "custom:-start", "unknown"] {
            let mut args = vec!["--intl-profile".into(), label.into()];
            let error = take_global_intl_profile(&mut args).unwrap_err();
            assert!(
                error.contains("Intl") || error.contains("--intl-profile"),
                "{error}"
            );
        }
        assert!(take_global_intl_profile(&mut vec!["--intl-profile".into()]).is_err());
        assert!(take_global_intl_profile(&mut vec![
            "--intl-profile".into(),
            "minimal".into(),
            "--intl-profile".into(),
            "minimal".into()
        ])
        .is_err());
    }

    #[test]
    fn explicit_intl_profile_is_rejected_before_unsupported_commands_read_source() {
        for tail in [
            vec!["inspect", "missing.js"],
            vec!["compiler-identity"],
            vec!["cache", "status"],
            vec!["run", "--execution-backend", "spec-exec", "missing.js"],
        ] {
            for profile in ["minimal", "custom:chosen"] {
                let args = [
                    vec!["--intl-profile".to_string(), profile.to_string()],
                    tail.iter().map(|value| (*value).to_string()).collect(),
                ]
                .concat();
                let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
                assert!(error.contains("--intl-profile"), "{error}");
                assert!(
                    !error.contains("missing.js"),
                    "source must not be read: {error}"
                );
            }
        }
    }

    #[test]
    fn list_projection_arguments_are_checked_and_retained_before_source_loading() {
        for args in [
            vec![
                "--intl-list-locales",
                "es,he",
                "--intl-profile",
                "custom:selected",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-list-locales",
                "he,es",
            ],
        ] {
            let mut args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            args.extend(["build".into(), "wasm".into(), "case.js".into()]);
            let selection = take_global_intl_profile(&mut args).unwrap().unwrap();
            assert_eq!(
                selection,
                IntlCompilationProfile::CustomProjection(
                    CustomIntlProfile::new(
                        CustomProfileId::parse("selected").unwrap(),
                        Some(&["es", "he"]),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                    .unwrap()
                )
            );
            assert_eq!(args, ["build", "wasm", "case.js"]);
        }
        for tail in [
            vec!["--intl-list-locales", "es"],
            vec!["--intl-list-locales", "es", "--intl-profile", "minimal"],
            vec![
                "--intl-list-locales",
                "",
                "--intl-profile",
                "custom:selected",
            ],
            vec![
                "--intl-list-locales",
                "es,es",
                "--intl-profile",
                "custom:selected",
            ],
            vec![
                "--intl-list-locales",
                "es,he",
                "--intl-profile",
                "custom:selected",
                "--intl-list-locales",
                "ar",
            ],
            vec!["--intl-list-locales"],
            vec![
                "--intl-list-locales",
                "en--US",
                "--intl-profile",
                "custom:selected",
            ],
            vec![
                "--intl-list-locales",
                "es",
                "--intl-profile",
                "custom:selected",
                "inspect",
                "missing.js",
            ],
            vec![
                "--intl-list-locales",
                "es",
                "--intl-profile",
                "custom:selected",
                "run",
                "--execution-backend",
                "spec-exec",
                "missing.js",
            ],
        ] {
            let args = tail.into_iter().map(str::to_owned).collect();
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(error.contains("--intl-"), "{error}");
            assert!(!error.contains("missing.js"), "{error}");
        }
    }

    #[test]
    fn relative_and_combined_projection_arguments_are_checked_before_source_loading() {
        assert!(take_global_intl_profile(&mut vec![
            "--intl-profile".into(),
            "custom:selected".into(),
            "--intl-relative-time-locales".into(),
        ])
        .unwrap_err()
        .contains("--intl-relative-time-locales needs a value"));
        for (flags, lists, relatives) in [
            (
                vec![
                    "--intl-relative-time-locales",
                    "pl,fr",
                    "--intl-profile",
                    "custom:selected",
                ],
                None,
                Some(&["fr", "pl"][..]),
            ),
            (
                vec![
                    "--intl-profile",
                    "custom:selected",
                    "--intl-relative-time-locales",
                    "fr,pl",
                    "--intl-list-locales",
                    "he,es",
                ],
                Some(&["es", "he"][..]),
                Some(&["fr", "pl"][..]),
            ),
            (
                vec![
                    "--intl-list-locales",
                    "es,he",
                    "--intl-relative-time-locales",
                    "pl,fr",
                    "--intl-profile",
                    "custom:selected",
                ],
                Some(&["es", "he"][..]),
                Some(&["fr", "pl"][..]),
            ),
        ] {
            let mut args = flags.into_iter().map(str::to_owned).collect::<Vec<_>>();
            args.extend(["build".into(), "wasm".into(), "case.js".into()]);
            assert_eq!(
                take_global_intl_profile(&mut args).unwrap().unwrap(),
                IntlCompilationProfile::CustomProjection(
                    CustomIntlProfile::new(
                        CustomProfileId::parse("selected").unwrap(),
                        lists,
                        relatives,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                    .unwrap()
                )
            );
            assert_eq!(args, ["build", "wasm", "case.js"]);
        }
        for flags in [
            vec!["--intl-relative-time-locales", "fr"],
            vec![
                "--intl-relative-time-locales",
                "fr",
                "--intl-profile",
                "minimal",
            ],
            vec![
                "--intl-relative-time-locales",
                "fr",
                "--intl-profile",
                "conformance",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-relative-time-locales",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-relative-time-locales",
                "",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-relative-time-locales",
                "fr,fr",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-relative-time-locales",
                "fr,",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-relative-time-locales",
                "en--US",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-relative-time-locales",
                "fr",
                "--intl-relative-time-locales",
                "pl",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-list-locales",
                "",
                "--intl-relative-time-locales",
                "fr",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-list-locales",
                "es",
                "--intl-relative-time-locales",
                "",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-list-locales",
                "es",
                "--intl-relative-time-locales",
                "fr",
                "--intl-profile",
                "custom:other",
            ],
        ] {
            let mut args = flags.into_iter().map(str::to_owned).collect::<Vec<_>>();
            args.extend(["build".into(), "wasm".into(), "missing.js".into()]);
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(
                error.contains("--intl-") || error.contains("Conformance"),
                "{error}"
            );
            assert!(
                !error.contains("missing.js"),
                "source must not be loaded: {error}"
            );
        }
        for command in [
            vec!["inspect", "missing.js"],
            vec!["run", "--execution-backend", "spec-exec", "missing.js"],
        ] {
            let args = [
                "--intl-profile",
                "custom:selected",
                "--intl-relative-time-locales",
                "fr",
            ]
            .into_iter()
            .chain(command)
            .map(str::to_owned)
            .collect();
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(error.contains("--intl-profile"), "{error}");
            assert!(!error.contains("missing.js"), "{error}");
        }
    }

    #[test]
    fn display_names_projection_arguments_are_checked_before_source_loading() {
        for (flags, lists, relatives) in [
            (
                vec![
                    "--intl-displaynames-locales",
                    "ja,fr",
                    "--intl-profile",
                    "custom:selected",
                ],
                None,
                None,
            ),
            (
                vec![
                    "--intl-profile",
                    "custom:selected",
                    "--intl-relative-time-locales",
                    "pl,fr",
                    "--intl-displaynames-locales",
                    "fr,ja",
                    "--intl-list-locales",
                    "he,es",
                ],
                Some(&["es", "he"][..]),
                Some(&["fr", "pl"][..]),
            ),
            (
                vec![
                    "--intl-displaynames-locales",
                    "ja,fr",
                    "--intl-list-locales",
                    "es,he",
                    "--intl-relative-time-locales",
                    "fr,pl",
                    "--intl-profile",
                    "custom:selected",
                ],
                Some(&["es", "he"][..]),
                Some(&["fr", "pl"][..]),
            ),
        ] {
            let mut args = flags.into_iter().map(str::to_owned).collect::<Vec<_>>();
            args.extend(["build".into(), "wasm".into(), "case.js".into()]);
            assert_eq!(
                take_global_intl_profile(&mut args).unwrap().unwrap(),
                IntlCompilationProfile::CustomProjection(
                    CustomIntlProfile::new(
                        CustomProfileId::parse("selected").unwrap(),
                        lists,
                        relatives,
                        Some(&["fr", "ja"]),
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                    .unwrap()
                )
            );
            assert_eq!(args, ["build", "wasm", "case.js"]);
        }
        for flags in [
            vec!["--intl-displaynames-locales", "fr"],
            vec![
                "--intl-profile",
                "minimal",
                "--intl-displaynames-locales",
                "fr",
            ],
            vec![
                "--intl-profile",
                "conformance",
                "--intl-displaynames-locales",
                "fr",
            ],
            vec![
                "--intl-profile",
                "custom:bad/id",
                "--intl-displaynames-locales",
                "fr",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-displaynames-locales",
            ],
            vec![
                "--intl-displaynames-locales",
                "--intl-profile",
                "custom:selected",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-displaynames-locales",
                "",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-displaynames-locales",
                "fr,",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-displaynames-locales",
                "fr,fr",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-displaynames-locales",
                "en--US",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-displaynames-locales",
                "fr",
                "--intl-displaynames-locales",
                "ja",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-list-locales",
                "",
                "--intl-displaynames-locales",
                "fr",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-relative-time-locales",
                "",
                "--intl-displaynames-locales",
                "fr",
            ],
        ] {
            let mut args = flags.into_iter().map(str::to_owned).collect::<Vec<_>>();
            args.extend(["build".into(), "wasm".into(), "missing.js".into()]);
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(
                error.contains("--intl-") || error.contains("Conformance"),
                "{error}"
            );
            assert!(
                !error.contains("missing.js"),
                "source must not be read: {error}"
            );
        }
        assert!(take_global_intl_profile(&mut vec![
            "--intl-displaynames-locales".into(),
            "--intl-profile".into(),
            "custom:selected".into()
        ])
        .unwrap_err()
        .contains("--intl-displaynames-locales needs a value"));
        for command in [
            vec!["inspect", "missing.js"],
            vec!["run", "--execution-backend", "spec-exec", "missing.js"],
        ] {
            let args = [
                "--intl-profile",
                "custom:selected",
                "--intl-displaynames-locales",
                "fr",
            ]
            .into_iter()
            .chain(command)
            .map(str::to_owned)
            .collect();
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(error.contains("--intl-profile"), "{error}");
            assert!(
                !error.contains("missing.js"),
                "source must not be read: {error}"
            );
        }
    }

    #[test]
    fn duration_projection_arguments_compose_all_filters_and_reject_before_source_loading() {
        for with_other_filters in [false, true] {
            for reverse in [false, true] {
                let mut filters = vec![(
                    "--intl-duration-locales",
                    if reverse { "sr,fr" } else { "fr,sr" },
                )];
                if with_other_filters {
                    filters.extend([
                        ("--intl-list-locales", "es,he"),
                        ("--intl-relative-time-locales", "fr,pl"),
                        ("--intl-displaynames-locales", "fr,ja"),
                    ]);
                }
                if reverse {
                    filters.reverse();
                }
                let mut args = filters
                    .into_iter()
                    .flat_map(|(flag, locales)| [flag.to_owned(), locales.to_owned()])
                    .collect::<Vec<_>>();
                if reverse {
                    args.splice(
                        0..0,
                        ["--intl-profile".to_owned(), "custom:selected".to_owned()],
                    );
                } else {
                    args.extend(["--intl-profile".to_owned(), "custom:selected".to_owned()]);
                }
                args.extend(["build".into(), "wasm".into(), "case.js".into()]);
                assert_eq!(
                    take_global_intl_profile(&mut args).unwrap().unwrap(),
                    IntlCompilationProfile::CustomProjection(
                        CustomIntlProfile::new(
                            CustomProfileId::parse("selected").unwrap(),
                            with_other_filters.then_some(&["es", "he"][..]),
                            with_other_filters.then_some(&["fr", "pl"][..]),
                            with_other_filters.then_some(&["fr", "ja"][..]),
                            Some(&["fr", "sr"]),
                            None,
                            None,
                            None,
                            None,
                        )
                        .unwrap()
                    )
                );
                assert_eq!(args, ["build", "wasm", "case.js"]);
            }
        }
        for flags in [
            vec!["--intl-duration-locales", "fr"],
            vec!["--intl-profile", "minimal", "--intl-duration-locales", "fr"],
            vec![
                "--intl-profile",
                "conformance",
                "--intl-duration-locales",
                "fr",
            ],
            vec![
                "--intl-profile",
                "custom:bad/id",
                "--intl-duration-locales",
                "fr",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-duration-locales",
            ],
            vec![
                "--intl-duration-locales",
                "--intl-profile",
                "custom:selected",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-duration-locales",
                "",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-duration-locales",
                "fr,",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-duration-locales",
                "fr,fr",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-duration-locales",
                "en--US",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-duration-locales",
                "fr",
                "--intl-duration-locales",
                "sr",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-list-locales",
                "",
                "--intl-duration-locales",
                "fr",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-relative-time-locales",
                "",
                "--intl-duration-locales",
                "fr",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-displaynames-locales",
                "",
                "--intl-duration-locales",
                "fr",
            ],
        ] {
            let mut args = flags.into_iter().map(str::to_owned).collect::<Vec<_>>();
            args.extend(["build".into(), "wasm".into(), "missing.js".into()]);
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(
                error.contains("--intl-") || error.contains("Conformance"),
                "{error}"
            );
            assert!(
                !error.contains("missing.js"),
                "source must not be read: {error}"
            );
        }
        assert!(take_global_intl_profile(&mut vec![
            "--intl-duration-locales".into(),
            "--intl-profile".into(),
            "custom:selected".into()
        ])
        .unwrap_err()
        .contains("--intl-duration-locales needs a value"));
        for command in [
            vec!["inspect", "missing.js"],
            vec!["run", "--execution-backend", "spec-exec", "missing.js"],
        ] {
            let args = [
                "--intl-profile",
                "custom:selected",
                "--intl-duration-locales",
                "fr",
            ]
            .into_iter()
            .chain(command)
            .map(str::to_owned)
            .collect();
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(error.contains("--intl-profile"), "{error}");
            assert!(
                !error.contains("missing.js"),
                "source must not be read: {error}"
            );
        }
    }

    #[test]
    fn number_projection_arguments_compose_all_filters_and_reject_before_source_loading() {
        for with_other_filters in [false, true] {
            for reverse in [false, true] {
                let mut filters = vec![(
                    "--intl-number-locales",
                    if reverse { "pl,es" } else { "es,pl" },
                )];
                if with_other_filters {
                    filters.extend([
                        ("--intl-list-locales", "es,he"),
                        ("--intl-relative-time-locales", "fr,pl"),
                        ("--intl-displaynames-locales", "fr,ja"),
                        ("--intl-duration-locales", "fr,sr"),
                    ]);
                }
                if reverse {
                    filters.reverse();
                }
                let mut args = filters
                    .into_iter()
                    .flat_map(|(flag, locales)| [flag.to_owned(), locales.to_owned()])
                    .collect::<Vec<_>>();
                if reverse {
                    args.splice(
                        0..0,
                        ["--intl-profile".to_owned(), "custom:selected".to_owned()],
                    );
                } else {
                    args.extend(["--intl-profile".to_owned(), "custom:selected".to_owned()]);
                }
                args.extend(["build".into(), "wasm".into(), "case.js".into()]);
                assert_eq!(
                    take_global_intl_profile(&mut args).unwrap().unwrap(),
                    IntlCompilationProfile::CustomProjection(
                        CustomIntlProfile::new(
                            CustomProfileId::parse("selected").unwrap(),
                            with_other_filters.then_some(&["es", "he"][..]),
                            with_other_filters.then_some(&["fr", "pl"][..]),
                            with_other_filters.then_some(&["fr", "ja"][..]),
                            with_other_filters.then_some(&["fr", "sr"][..]),
                            Some(&["es", "pl"]),
                            None,
                            None,
                            None,
                        )
                        .unwrap()
                    )
                );
                assert_eq!(args, ["build", "wasm", "case.js"]);
            }
        }
        for flags in [
            vec!["--intl-number-locales", "es"],
            vec!["--intl-profile", "minimal", "--intl-number-locales", "es"],
            vec![
                "--intl-profile",
                "conformance",
                "--intl-number-locales",
                "es",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-number-locales",
                "",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-number-locales",
                "es,",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-number-locales",
                "es,es",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-number-locales",
                "en--US",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-number-locales",
                "es",
                "--intl-number-locales",
                "pl",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-number-locales",
                "es",
                "--intl-list-locales",
                "",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-number-locales",
                "es",
                "--intl-relative-time-locales",
                "",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-number-locales",
                "es",
                "--intl-displaynames-locales",
                "",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-number-locales",
                "es",
                "--intl-duration-locales",
                "",
            ],
        ] {
            let mut args = flags.into_iter().map(str::to_owned).collect::<Vec<_>>();
            args.extend(["build".into(), "wasm".into(), "missing.js".into()]);
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(
                error.contains("--intl-") || error.contains("Conformance"),
                "{error}"
            );
            assert!(
                !error.contains("missing.js"),
                "source must not be read: {error}"
            );
        }
        assert!(take_global_intl_profile(&mut vec![
            "--intl-number-locales".into(),
            "--intl-profile".into(),
            "custom:selected".into()
        ])
        .unwrap_err()
        .contains("--intl-number-locales needs a value"));
        for command in [
            vec!["inspect", "missing.js"],
            vec!["run", "--execution-backend", "spec-exec", "missing.js"],
        ] {
            let args = [
                "--intl-profile",
                "custom:selected",
                "--intl-number-locales",
                "es",
            ]
            .into_iter()
            .chain(command)
            .map(str::to_owned)
            .collect();
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(error.contains("--intl-profile"), "{error}");
            assert!(
                !error.contains("missing.js"),
                "source must not be read: {error}"
            );
        }
    }

    #[test]
    fn datetime_projection_arguments_compose_six_filters_and_reject_before_source_loading() {
        for with_other_filters in [false, true] {
            for reverse in [false, true] {
                let mut filters = vec![(
                    "--intl-datetime-locales",
                    if reverse { "zh,ar-EG" } else { "ar-EG,zh" },
                )];
                if with_other_filters {
                    filters.extend([
                        ("--intl-list-locales", "es,he"),
                        ("--intl-relative-time-locales", "fr,pl"),
                        ("--intl-displaynames-locales", "fr,ja"),
                        ("--intl-duration-locales", "fr,sr"),
                        ("--intl-number-locales", "es,pl"),
                    ]);
                }
                if reverse {
                    filters.reverse();
                }
                let mut args = filters
                    .into_iter()
                    .flat_map(|(flag, locales)| [flag.to_owned(), locales.to_owned()])
                    .collect::<Vec<_>>();
                if reverse {
                    args.splice(0..0, ["--intl-profile".into(), "custom:selected".into()]);
                } else {
                    args.extend(["--intl-profile".into(), "custom:selected".into()]);
                }
                args.extend(["build".into(), "wasm".into(), "case.js".into()]);
                assert_eq!(
                    take_global_intl_profile(&mut args).unwrap().unwrap(),
                    IntlCompilationProfile::CustomProjection(
                        CustomIntlProfile::new(
                            CustomProfileId::parse("selected").unwrap(),
                            with_other_filters.then_some(&["es", "he"][..]),
                            with_other_filters.then_some(&["fr", "pl"][..]),
                            with_other_filters.then_some(&["fr", "ja"][..]),
                            with_other_filters.then_some(&["fr", "sr"][..]),
                            with_other_filters.then_some(&["es", "pl"][..]),
                            Some(&["ar-EG", "zh"]),
                            None,
                            None,
                        )
                        .unwrap()
                    )
                );
                assert_eq!(args, ["build", "wasm", "case.js"]);
            }
        }
        for flags in [
            vec!["--intl-datetime-locales", "zh"],
            vec!["--intl-profile", "minimal", "--intl-datetime-locales", "zh"],
            vec![
                "--intl-profile",
                "conformance",
                "--intl-datetime-locales",
                "zh",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-datetime-locales",
                "",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-datetime-locales",
                "zh,",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-datetime-locales",
                "zh,zh",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-datetime-locales",
                "en--US",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-datetime-locales",
                "zh",
                "--intl-datetime-locales",
                "ar-EG",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-datetime-locales",
                "zh",
                "--intl-number-locales",
                "",
            ],
        ] {
            let mut args = flags.into_iter().map(str::to_owned).collect::<Vec<_>>();
            args.extend(["build".into(), "wasm".into(), "missing.js".into()]);
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(
                error.contains("--intl-") || error.contains("Conformance"),
                "{error}"
            );
            assert!(
                !error.contains("missing.js"),
                "source must not be read: {error}"
            );
        }
        assert!(take_global_intl_profile(&mut vec![
            "--intl-datetime-locales".into(),
            "--intl-profile".into(),
            "custom:selected".into()
        ])
        .unwrap_err()
        .contains("--intl-datetime-locales needs a value"));
        for command in [
            vec!["inspect", "missing.js"],
            vec!["run", "--execution-backend", "spec-exec", "missing.js"],
        ] {
            let args = [
                "--intl-profile",
                "custom:selected",
                "--intl-datetime-locales",
                "zh",
            ]
            .into_iter()
            .chain(command)
            .map(str::to_owned)
            .collect();
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(error.contains("--intl-profile"), "{error}");
            assert!(
                !error.contains("missing.js"),
                "source must not be read: {error}"
            );
        }
    }

    #[test]
    fn segmenter_projection_arguments_compose_eight_filters_and_reject_before_source_loading() {
        for reverse in [false, true] {
            let mut flags = vec![
                ("--intl-list-locales", "es,he"),
                ("--intl-relative-time-locales", "fr,pl"),
                ("--intl-displaynames-locales", "fr,ja"),
                ("--intl-duration-locales", "fr,sr"),
                ("--intl-number-locales", "es,pl"),
                ("--intl-datetime-locales", "ar-EG,zh"),
                ("--intl-collator-locales", "de-CH,sv"),
                (
                    "--intl-segmenter-locales",
                    if reverse { "sv,el" } else { "el,sv" },
                ),
            ];
            if reverse {
                flags.reverse();
            }
            let mut args = flags
                .into_iter()
                .flat_map(|(flag, value)| [flag.to_owned(), value.to_owned()])
                .collect::<Vec<_>>();
            args.extend([
                "--intl-profile".into(),
                "custom:selected".into(),
                "build".into(),
                "wasm".into(),
                "case.js".into(),
            ]);
            let selected = take_global_intl_profile(&mut args).unwrap().unwrap();
            assert_eq!(
                selected,
                IntlCompilationProfile::CustomProjection(
                    CustomIntlProfile::new(
                        CustomProfileId::parse("selected").unwrap(),
                        Some(&["es", "he"]),
                        Some(&["fr", "pl"]),
                        Some(&["fr", "ja"]),
                        Some(&["fr", "sr"]),
                        Some(&["es", "pl"]),
                        Some(&["ar-EG", "zh"]),
                        Some(&["de-CH", "sv"]),
                        Some(&["el", "sv"]),
                    )
                    .unwrap()
                )
            );
            assert_eq!(args, ["build", "wasm", "case.js"]);
        }
        for flags in [
            vec!["--intl-segmenter-locales", "sv"],
            vec![
                "--intl-profile",
                "minimal",
                "--intl-segmenter-locales",
                "sv",
            ],
            vec![
                "--intl-profile",
                "conformance",
                "--intl-segmenter-locales",
                "sv",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-segmenter-locales",
                "",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-segmenter-locales",
                "sv,",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-segmenter-locales",
                "sv,sv",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-segmenter-locales",
                "en--US",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-segmenter-locales",
                "sv",
                "--intl-segmenter-locales",
                "el",
            ],
        ] {
            let mut args = flags.into_iter().map(str::to_owned).collect::<Vec<_>>();
            args.extend(["build".into(), "wasm".into(), "missing.js".into()]);
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(
                error.contains("--intl-") || error.contains("Conformance"),
                "{error}"
            );
            assert!(
                !error.contains("missing.js"),
                "source must not be read: {error}"
            );
        }
        assert!(take_global_intl_profile(&mut vec![
            "--intl-segmenter-locales".into(),
            "--intl-profile".into(),
            "custom:selected".into()
        ])
        .unwrap_err()
        .contains("--intl-segmenter-locales needs a value"));
        for command in [
            vec!["inspect", "missing.js"],
            vec!["run", "--execution-backend", "spec-exec", "missing.js"],
        ] {
            let mut args = vec![
                "--intl-profile".into(),
                "custom:selected".into(),
                "--intl-segmenter-locales".into(),
                "sv".into(),
            ];
            args.extend(command.into_iter().map(str::to_owned));
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(error.contains("--intl-profile"), "{error}");
            assert!(!error.contains("missing.js"));
        }
    }

    #[test]
    fn collator_projection_arguments_compose_seven_filters_and_reject_before_source_loading() {
        for with_other_filters in [false, true] {
            for reverse in [false, true] {
                let mut filters = vec![(
                    "--intl-collator-locales",
                    if reverse { "sv,de-CH" } else { "de-CH,sv" },
                )];
                if with_other_filters {
                    filters.extend([
                        ("--intl-list-locales", "es,he"),
                        ("--intl-relative-time-locales", "fr,pl"),
                        ("--intl-displaynames-locales", "fr,ja"),
                        ("--intl-duration-locales", "fr,sr"),
                        ("--intl-number-locales", "es,pl"),
                        ("--intl-datetime-locales", "ar-EG,zh"),
                    ]);
                }
                if reverse {
                    filters.reverse();
                }
                let mut args = filters
                    .into_iter()
                    .flat_map(|(flag, value)| [flag.to_owned(), value.to_owned()])
                    .collect::<Vec<_>>();
                if reverse {
                    args.splice(0..0, ["--intl-profile".into(), "custom:selected".into()]);
                } else {
                    args.extend(["--intl-profile".into(), "custom:selected".into()]);
                }
                args.extend(["build".into(), "wasm".into(), "case.js".into()]);
                assert_eq!(
                    take_global_intl_profile(&mut args).unwrap().unwrap(),
                    IntlCompilationProfile::CustomProjection(
                        CustomIntlProfile::new(
                            CustomProfileId::parse("selected").unwrap(),
                            with_other_filters.then_some(&["es", "he"][..]),
                            with_other_filters.then_some(&["fr", "pl"][..]),
                            with_other_filters.then_some(&["fr", "ja"][..]),
                            with_other_filters.then_some(&["fr", "sr"][..]),
                            with_other_filters.then_some(&["es", "pl"][..]),
                            with_other_filters.then_some(&["ar-EG", "zh"][..]),
                            Some(&["de-CH", "sv"]),
                            None,
                        )
                        .unwrap()
                    )
                );
                assert_eq!(args, ["build", "wasm", "case.js"]);
            }
        }
        for flags in [
            vec!["--intl-collator-locales", "sv"],
            vec!["--intl-profile", "minimal", "--intl-collator-locales", "sv"],
            vec![
                "--intl-profile",
                "conformance",
                "--intl-collator-locales",
                "sv",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-collator-locales",
                "",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-collator-locales",
                "sv,",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-collator-locales",
                "sv,sv",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-collator-locales",
                "en--US",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-collator-locales",
                "sv",
                "--intl-collator-locales",
                "de-CH",
            ],
            vec![
                "--intl-profile",
                "custom:selected",
                "--intl-collator-locales",
                "sv",
                "--intl-datetime-locales",
                "",
            ],
        ] {
            let mut args = flags.into_iter().map(str::to_owned).collect::<Vec<_>>();
            args.extend(["build".into(), "wasm".into(), "missing.js".into()]);
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(
                error.contains("--intl-") || error.contains("Conformance"),
                "{error}"
            );
            assert!(
                !error.contains("missing.js"),
                "source must not be read: {error}"
            );
        }
        assert!(take_global_intl_profile(&mut vec![
            "--intl-collator-locales".into(),
            "--intl-profile".into(),
            "custom:selected".into()
        ])
        .unwrap_err()
        .contains("--intl-collator-locales needs a value"));
        for command in [
            vec!["inspect", "missing.js"],
            vec!["run", "--execution-backend", "spec-exec", "missing.js"],
        ] {
            let args = [
                "--intl-profile",
                "custom:selected",
                "--intl-collator-locales",
                "sv",
            ]
            .into_iter()
            .chain(command)
            .map(str::to_owned)
            .collect();
            let error = command_main(args, Arc::new(Mutex::new(Vec::new()))).unwrap_err();
            assert!(error.contains("--intl-profile"), "{error}");
            assert!(
                !error.contains("missing.js"),
                "source must not be read: {error}"
            );
        }
    }

    #[test]
    fn differential_replay_requires_an_explicit_oracle() {
        let error = parse_differential_replay_args(&["case.json".to_string()])
            .expect_err("oracle must never be selected by default");

        assert_eq!(
            error,
            "differential replay requires explicit `--oracle spec-exec`; the oracle is never a default"
        );
    }

    #[test]
    fn differential_replay_accepts_one_case_and_spec_exec_oracle() {
        let parsed = parse_differential_replay_args(&[
            "case.json".to_string(),
            "--oracle".to_string(),
            "spec-exec".to_string(),
        ])
        .expect("explicit replay arguments should parse");

        assert_eq!(parsed.case_path, PathBuf::from("case.json"));
        assert_eq!(parsed.oracle, SpecExecOracle::explicitly_enabled());
    }

    #[test]
    fn differential_replay_rejects_non_oracle_backends() {
        let error = parse_differential_replay_args(&[
            "case.json".to_string(),
            "--oracle".to_string(),
            "wasm-aot".to_string(),
        ])
        .expect_err("product backend must not be accepted as the oracle");

        assert_eq!(
            error,
            "unknown differential oracle: wasm-aot (expected spec-exec)"
        );
    }

    #[test]
    fn differential_generate_arithmetic_accepts_one_bounded_plan() {
        let parsed = parse_differential_generate_arithmetic_args(&[
            "generated.json".to_string(),
            "--seed".to_string(),
            "1".to_string(),
            "--checks".to_string(),
            "4".to_string(),
            "--depth".to_string(),
            "2".to_string(),
            "--max-replays".to_string(),
            "64".to_string(),
            "--oracle".to_string(),
            "spec-exec".to_string(),
        ])
        .expect("one explicit bounded generation plan should parse");

        assert_eq!(parsed.output_path, PathBuf::from("generated.json"));
        assert_eq!(parsed.plan.seed().get(), 1);
        assert_eq!(parsed.plan.grammar(), ArithmeticGrammar::IntegerBitwiseV2);
        assert_eq!(parsed.plan.checks().get(), 4);
        assert_eq!(parsed.plan.depth().get(), 2);
        assert_eq!(parsed.reduction_limit.get(), 64);
        assert_eq!(parsed.oracle, SpecExecOracle::explicitly_enabled());
    }

    #[test]
    fn differential_generate_arithmetic_requires_an_explicit_oracle() {
        let error = parse_differential_generate_arithmetic_args(&[
            "generated.json".to_string(),
            "--seed".to_string(),
            "1".to_string(),
            "--checks".to_string(),
            "4".to_string(),
            "--depth".to_string(),
            "2".to_string(),
            "--max-replays".to_string(),
            "64".to_string(),
        ])
        .expect_err("the generator must never select an oracle by default");

        assert_eq!(
            error,
            "differential generate-arithmetic requires explicit `--oracle spec-exec`; the oracle is never a default"
        );
    }

    #[test]
    fn differential_generate_arithmetic_selects_historical_v1_explicitly() {
        let parsed = parse_differential_generate_arithmetic_args(
            &[
                "generated.json",
                "--grammar",
                "integer-arithmetic-v1",
                "--seed",
                "1",
                "--checks",
                "4",
                "--depth",
                "2",
                "--max-replays",
                "64",
                "--oracle",
                "spec-exec",
            ]
            .map(str::to_string),
        )
        .expect("the versioned V1 corpus remains explicitly reproducible");
        assert_eq!(
            parsed.plan.grammar(),
            ArithmeticGrammar::IntegerArithmeticV1
        );
        assert_eq!(parsed.plan.seed().get(), 1);
    }

    #[test]
    fn differential_generate_arithmetic_selects_bounded_product_v3_explicitly() {
        let parsed = parse_differential_generate_arithmetic_args(
            &[
                "product.json",
                "--grammar",
                "integer-product-v3",
                "--seed",
                "7",
                "--checks",
                "32",
                "--depth",
                "4",
                "--max-replays",
                "64",
                "--oracle",
                "spec-exec",
            ]
            .map(str::to_string),
        )
        .expect("the product grammar uses the same bounded campaign and oracle gate");
        assert_eq!(parsed.output_path, PathBuf::from("product.json"));
        assert_eq!(parsed.plan.grammar(), ArithmeticGrammar::IntegerProductV3);
        assert_eq!(parsed.plan.seed().get(), 7);
        assert_eq!(parsed.plan.checks().get(), 32);
        assert_eq!(parsed.plan.depth(), ArithmeticExpressionDepth::Four);
        assert_eq!(parsed.reduction_limit.get(), 64);
        assert_eq!(parsed.oracle, SpecExecOracle::explicitly_enabled());
    }

    #[test]
    fn differential_generate_arithmetic_rejects_unknown_or_duplicate_grammar() {
        let unknown = parse_differential_generate_arithmetic_args(&[
            "--grammar".to_string(),
            "integer-bitwise-v3".to_string(),
        ])
        .expect_err("an unimplemented grammar cannot label a corpus");
        assert_eq!(
            unknown,
            "unknown arithmetic grammar: integer-bitwise-v3 (expected integer-arithmetic-v1, integer-bitwise-v2, or integer-product-v3)"
        );
        let duplicate = parse_differential_generate_arithmetic_args(
            &[
                "--grammar",
                "integer-arithmetic-v1",
                "--grammar",
                "integer-bitwise-v2",
            ]
            .map(str::to_string),
        )
        .expect_err("a plan has one grammar");
        assert_eq!(duplicate, "--grammar may only be specified once");
    }

    #[test]
    fn differential_generate_arithmetic_rejects_an_empty_program() {
        let error = parse_differential_generate_arithmetic_args(&[
            "generated.json".to_string(),
            "--seed".to_string(),
            "1".to_string(),
            "--checks".to_string(),
            "0".to_string(),
            "--depth".to_string(),
            "2".to_string(),
            "--max-replays".to_string(),
            "64".to_string(),
            "--oracle".to_string(),
            "spec-exec".to_string(),
        ])
        .expect_err("the typed check count must reject an empty program");

        assert_eq!(error, "arithmetic check count must be in 1..=32");
    }

    #[test]
    fn parse_test262_args_reads_filter_and_shard() {
        let parsed = parse_test262_args(&[
            "--shard".to_string(),
            "1/4".to_string(),
            "language/expressions".to_string(),
            "--timeout-ms".to_string(),
            "50".to_string(),
        ])
        .expect("args should parse");
        assert_eq!(parsed.run_config.shard_index, 0);
        assert_eq!(parsed.run_config.shard_count, 4);
        assert_eq!(parsed.filter.as_deref(), Some("language/expressions"));
        assert_eq!(parsed.config.timeout_ms, 50);
        // Product default: in-process execution (the Store's execution
        // deadline bounds Wasm-AOT hangs), not the child-process case runner. The
        // Whole-case execution is supervised through the selected CLI image.
        assert_eq!(
            parsed.config.case_runner_bin.as_ref(),
            Some(&std::env::current_exe().unwrap())
        );
        // Product default: `scriptw test262 ...` runs Wasm-AOT flag-free.
        assert_eq!(
            parsed.run_config.execution_backend,
            ExecutionBackend::WasmAot
        );
    }

    #[test]
    fn process_reuse_is_explicit_and_keeps_the_worker_count_and_timeout() {
        let default = parse_test262_args(&[]).unwrap();
        assert!(!default.config.reuse_case_workers);
        let parsed = parse_test262_args(
            &[
                "--reuse-case-workers",
                "--threads",
                "1",
                "--timeout-ms",
                "5000",
            ]
            .map(str::to_string),
        )
        .unwrap();
        assert!(parsed.config.reuse_case_workers);
        assert_eq!(parsed.config.worker_count, 1);
        assert_eq!(parsed.config.timeout_ms, 5000);
    }

    #[test]
    fn exact_case_worker_preserves_harness_and_rejects_duplicate_selection() {
        let parsed = parse_test262_case_worker_args(&[
            "raw-script:language/fixture.js".into(),
            "--threads".into(),
            "1".into(),
            "--case-harness-file".into(),
            "fixture-harness.js".into(),
        ])
        .unwrap();
        assert_eq!(
            parsed.config.local_harness,
            LocalHarnessSource::File(PathBuf::from("fixture-harness.js"))
        );
        assert!(parse_test262_case_worker_args(&[
            "raw-script:language/fixture.js".into(),
            "--case-harness".into(),
            "none".into(),
            "--case-harness".into(),
            "wasm-aot".into(),
        ])
        .is_err());
    }

    #[test]
    fn parse_test262_args_rejects_zero_matrix_node_limit() {
        let error = parse_test262_args(&["--max-matrix-nodes".to_string(), "0".to_string()])
            .expect_err("zero matrix node limit should be rejected");

        assert_eq!(error, "--max-matrix-nodes must be at least 1");
    }

    #[test]
    fn parse_test262_args_reads_exact_matrix_node_selector() {
        let parsed =
            parse_test262_args(&["--matrix-node".to_string(), "built-ins/Promise".to_string()])
                .expect("matrix node selector should parse");

        assert_eq!(parsed.matrix_node.as_deref(), Some("built-ins/Promise"));
        assert!(parsed.filter.is_none());
    }

    #[test]
    fn parse_test262_args_rejects_matrix_node_without_value() {
        let error = parse_test262_args(&["--matrix-node".to_string()])
            .expect_err("matrix node without a selector should be rejected");

        assert_eq!(error, "--matrix-node needs a value");
    }

    #[test]
    fn parse_test262_args_rejects_matrix_node_with_positional_filter() {
        let error = parse_test262_args(&[
            "--matrix-node".to_string(),
            "built-ins/Promise".to_string(),
            "built-ins/Array".to_string(),
        ])
        .expect_err("matrix node and positional filter should be rejected");

        assert_eq!(
            error,
            "--matrix-node cannot be combined with a positional filter"
        );
    }

    #[test]
    fn parse_run_args_defaults_to_wasm_aot_backend() {
        let parsed = parse_run_args(&["script.js".to_string()]).expect("run args should parse");
        assert_eq!(parsed.backend, ExecutionBackend::WasmAot);
        assert_eq!(parsed.path.as_deref(), Some("script.js"));
    }

    #[test]
    fn parse_run_args_spec_exec_requires_explicit_flag() {
        let parsed = parse_run_args(&[
            "--execution-backend".to_string(),
            "spec-exec".to_string(),
            "script.js".to_string(),
        ])
        .expect("run args should parse");
        assert_eq!(parsed.backend, ExecutionBackend::SpecExec);
    }

    #[test]
    fn parse_test262_args_defaults_to_wasm_aot_backend() {
        let parsed = parse_test262_args(&[]).expect("empty args should parse");
        assert_eq!(
            parsed.run_config.execution_backend,
            ExecutionBackend::WasmAot
        );
        assert_eq!(
            parsed.config.local_harness,
            LocalHarnessSource::EmbeddedWasmAot
        );
    }

    #[test]
    fn parse_test262_args_spec_exec_requires_explicit_flag() {
        // wasm-aot is the harness default (conformance area T25); spec-exec requires the
        // explicit --execution-backend flag exercised here.
        let parsed =
            parse_test262_args(&["--execution-backend".to_string(), "spec-exec".to_string()])
                .expect("explicit spec-exec backend should parse");
        assert_eq!(
            parsed.run_config.execution_backend,
            ExecutionBackend::SpecExec
        );
        assert_eq!(
            parsed.config.local_harness,
            LocalHarnessSource::EmbeddedSpecExec
        );
    }

    #[test]
    fn usage_omits_the_removed_javascript_oracle() {
        assert!(!usage().contains("compare-js-oracle"));
    }

    #[test]
    fn parse_test262_args_reads_execution_backend() {
        let parsed = parse_test262_args(&["--execution-backend".to_string(), "wasm".to_string()])
            .expect("backend should parse");
        assert_eq!(
            parsed.run_config.execution_backend,
            ExecutionBackend::WasmAot
        );
    }

    #[test]
    fn parse_test262_args_uses_embedded_wasm_aot_harness() {
        let parsed = parse_test262_args(&[
            "--execution-backend".to_string(),
            "wasm-aot".to_string(),
            "--suite-root".to_string(),
            "test262/vendor/test262".to_string(),
        ])
        .expect("backend should parse");
        assert_eq!(
            parsed.config.local_harness,
            LocalHarnessSource::EmbeddedWasmAot
        );
    }
}
