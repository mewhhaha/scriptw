//! Explicit reports for existing opt-in whole-CLI timing spans.
use scriptw_test262::CompilerProvenance;
use scriptw_engine::HostSurfacePolicy;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
#[path = "../tests/perf/chunk_cases.rs"]
mod chunk_cases;
mod report;
mod stats;
mod compiler;
mod runtime;
mod evidence;
mod budget;
use report::*;
const MEASUREMENT_SCOPE: &str =
    "whole-cli-invocation-wall-clock;no-subsystem-breakdown;not-conformance";
const BASELINE_MAX_BYTES: u64 = 16 * 1024 * 1024;
const LOG_MAX_BYTES: u64 = 64 * 1024 * 1024;
// The original twenty-case corpus deliberately uses conformance host helpers.
// Exact CLI gates retain the ordinary product surface.
const CORPUS_HOST_SURFACE: HostSurfacePolicy = HostSurfacePolicy::Test262;
const fn host_surface_label(policy: HostSurfacePolicy) -> &'static str {
    match policy {
        HostSurfacePolicy::Product => "product",
        HostSurfacePolicy::Test262 => "test262",
    }
}
#[derive(Debug)]
struct Options {
    mode: MeasurementMode,
    output: PathBuf,
    fixture_root: PathBuf,
    baseline: Option<PathBuf>,
    samples: usize,
    machine_label: String,
    gates: Vec<Gate>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MeasurementMode { WholeCli, Compiler, Runtime }
fn parse(arguments: Vec<String>) -> Result<Options, String> {
    let mode = match arguments.first().map(String::as_str) {
        Some("report") => MeasurementMode::WholeCli,
        Some("compile") => MeasurementMode::Compiler,
        Some("runtime") => MeasurementMode::Runtime,
        _ => return Err("performance requires report|compile|runtime --output-dir PATH --samples 3..100 --machine-label LABEL --idle-machine".into()),
    };
    let mut output = None;
    let mut fixture_root = None;
    let mut baseline = None;
    let mut samples = None;
    let mut machine_label = None;
    let mut gates = None;
    let mut idle = false;
    let mut index = 1;
    while index < arguments.len() {
        let flag = arguments[index].as_str();
        if flag == "--idle-machine" {
            if idle {
                return Err("duplicate --idle-machine".into());
            }
            idle = true;
            index += 1;
            continue;
        }
        let value = arguments
            .get(index + 1)
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag {
            "--output-dir" if output.is_none() => output = Some(PathBuf::from(value)),
            "--fixture-root" if fixture_root.is_none() => fixture_root = Some(PathBuf::from(value)),
            "--baseline" if baseline.is_none() => baseline = Some(PathBuf::from(value)),
            "--samples" if samples.is_none() => {
                let count = value
                    .parse::<usize>()
                    .map_err(|_| "--samples must be an integer in 3..100")?;
                if !(3..=100).contains(&count) {
                    return Err("--samples must be an integer in 3..100".into());
                }
                samples = Some(count);
            }
            "--machine-label" if machine_label.is_none() => {
                if value.trim().is_empty() || value.len() > 256 {
                    return Err("machine label must contain 1..256 bytes of nonblank text".into());
                }
                machine_label = Some(value.clone());
            }
            "--gate" if gates.is_none() => {
                gates = Some(match value.as_str() {
                    "all" => Gate::ALL.to_vec(),
                    "warm-exact" => vec![Gate::WarmExact],
                    "warm-chunk" => vec![Gate::WarmChunk],
                    "cold-exact" => vec![Gate::ColdExact],
                    _ => {
                        return Err(
                            "--gate must be all, warm-exact, warm-chunk or cold-exact".into()
                        )
                    }
                })
            }
            _ => return Err(format!("unknown or duplicate performance option: {flag}")),
        }
        index += 2;
    }
    if !idle {
        return Err("timing requires explicit --idle-machine acknowledgment".into());
    }
    if mode == MeasurementMode::Compiler && gates.is_some() {
        return Err("performance compile measures all four compiler stages; --gate belongs to whole-CLI reports".into());
    }
    if mode == MeasurementMode::Runtime && gates.is_some() {
        return Err("performance runtime measures emitted corpus execution; --gate belongs to whole-CLI reports".into());
    }
    Ok(Options {
        mode,
        output: output.ok_or("report needs --output-dir")?,
        fixture_root: fixture_root
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")),
        baseline,
        samples: samples.ok_or("report needs --samples")?,
        machine_label: machine_label.ok_or("report needs --machine-label")?,
        gates: gates.unwrap_or_else(|| match mode {
            MeasurementMode::WholeCli => Gate::ALL.to_vec(), MeasurementMode::Compiler | MeasurementMode::Runtime => vec![],
        }),
    })
}

pub(super) fn command(arguments: Vec<String>) -> Result<PathBuf, String> {
    match arguments.first().map(String::as_str) {
        Some("conformance") => return evidence::conformance(arguments),
        Some("data") => return evidence::data(arguments),
        Some("check") => return budget::command(arguments),
        _ => {}
    }
    let options = parse(arguments)?;
    if options.mode == MeasurementMode::Compiler { return compiler::command(options); }
    if options.mode == MeasurementMode::Runtime { return runtime::command(options); }
    let fixture_root = options
        .fixture_root
        .canonicalize()
        .map_err(|error| format!("cannot resolve benchmark fixtures: {error}"))?;
    require_fixtures(&fixture_root, &chunk_cases::CASES)?;
    let identity = capture_identity(&options, &fixture_root)?;
    // Baseline admission and executable binding happen before pruning/timing.
    let baseline_bytes = options
        .baseline
        .as_ref()
        .map(|path| read_bounded(path, BASELINE_MAX_BYTES))
        .transpose()?;
    let baseline = baseline_bytes
        .as_ref()
        .map(|bytes| {
            let report: Report = serde_json::from_slice(bytes)
                .map_err(|error| format!("invalid baseline: {error}"))?;
            verify_retained_evidence(
                &report,
                options
                    .baseline
                    .as_ref()
                    .expect("baseline bytes have a source path"),
            )?;
            Baseline::admit(report, digest(bytes), &identity)
        })
        .transpose()?;
    let executable = if options.gates.contains(&Gate::ColdExact) {
        Some(checked_cli_image(&identity.compiler)?)
    } else {
        None
    };
    fs::create_dir(&options.output)
        .map_err(|error| format!("report output must be a fresh directory: {error}"))?;
    let output = options
        .output
        .canonicalize()
        .map_err(|error| error.to_string())?;
    fs::create_dir(output.join("logs")).map_err(|error| error.to_string())?;
    fs::create_dir(output.join("workload")).map_err(|error| error.to_string())?;
    for (&name, source) in chunk_cases::CASES.iter().zip(chunk_cases::SOURCES) {
        write_new(&output.join("workload").join(name), source.as_bytes())?;
    }
    if let Some(bytes) = &baseline_bytes {
        write_new(&output.join("baseline.json"), bytes)?;
    }
    let mut report = Report {
        version: VERSION,
        started_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_secs(),
        identity,
        completion: Completion::Running,
        samples: Vec::new(),
        summaries: Vec::new(),
        comparison: None,
        measurement_scope: MEASUREMENT_SCOPE.into(),
        full_conformance_verified: false,
    };
    persist(&output, &report)?;
    let result = measure(
        &options,
        &fixture_root,
        &output,
        executable.as_deref(),
        &mut report,
    )
    .and_then(|()| {
        report.completion = Completion::Complete;
        report.summaries = report.summaries()?;
        if let Some(baseline) = &baseline {
            report.comparison = Some(baseline.compare(&report)?);
        }
        Ok(())
    });
    if let Err(reason) = result {
        report.completion = Completion::Failed {
            reason: reason.clone(),
        };
        report.summaries.clear();
        report.comparison = None;
        persist(&output, &report)?;
        return Err(format!(
            "{reason}; retained {}",
            output.join("report.json").display()
        ));
    }
    persist(&output, &report)?;
    if report
        .summaries
        .iter()
        .any(|gate| gate.over_budget_samples != 0)
    {
        return Err(format!(
            "measured samples exceeded an existing opt-in timing budget; retained {}",
            output.join("report.json").display()
        ));
    }
    Ok(output.join("report.json"))
}

fn measure(
    options: &Options,
    root: &Path,
    output: &Path,
    executable: Option<&Path>,
    report: &mut Report,
) -> Result<(), String> {
    for &gate in &options.gates {
        for repetition in 0..options.samples {
            let index = report.samples.len();
            report.samples.push(Sample {
                gate,
                repetition,
                warmup: Vec::new(),
                prune: None,
                measured: Vec::new(),
                aggregate_elapsed_ns: None,
            });
            persist(output, report)?;
            let names = gate.cases();
            if gate == Gate::ColdExact {
                let prune =
                    Command::new(executable.ok_or("cold sample requires the checked CLI image")?)
                        .args(["cache", "prune"])
                        .output()
                        .map_err(|error| format!("cannot start cache prune: {error}"))?;
                let code = prune.status.code().unwrap_or(-1);
                let stem = format!("{}-{repetition}-prune", gate.label());
                report.samples[index].prune = Some(Prune {
                    exit_code: code,
                    stdout: save_log(output, &format!("{stem}.stdout"), &prune.stdout)?,
                    stderr: save_log(output, &format!("{stem}.stderr"), &prune.stderr)?,
                });
                persist(output, report)?;
                if code != 0 {
                    return Err("cold sample cache prune failed".into());
                }
            } else {
                require_fixtures(root, &names)?;
                for (case_index, &name) in names.iter().enumerate() {
                    let run = invoke(name, root, None, gate)?;
                    let failed = run.1 != 0;
                    report.samples[index].warmup.push(retain_invocation(
                        output, gate, repetition, "warmup", case_index, name, run,
                    )?);
                    persist(output, report)?;
                    if failed {
                        return Err(format!("{} warmup failed for {name}", gate.label()));
                    }
                }
            }
            require_fixtures(root, &names)?;
            // File identity checks and artifact I/O stay outside the measured
            // span. Nested Instants retain actual case durations. The whole
            // chunk remains one serial loop, including capture/assert overhead.
            let started = Instant::now();
            let mut captures = Vec::new();
            let mut interrupted = None;
            for &name in &names {
                match invoke(
                    name,
                    root,
                    if gate == Gate::ColdExact {
                        executable
                    } else {
                        None
                    },
                    gate,
                ) {
                    Ok(run) => {
                        let failed = run.1 != 0;
                        captures.push((name, run));
                        if failed {
                            break;
                        }
                    }
                    Err(reason) => {
                        interrupted = Some(reason);
                        break;
                    }
                }
            }
            let elapsed = elapsed_ns(started)?;
            let mut failed = false;
            for (case_index, (name, run)) in captures.into_iter().enumerate() {
                failed |= run.1 != 0;
                report.samples[index].measured.push(retain_invocation(
                    output, gate, repetition, "measured", case_index, name, run,
                )?);
            }
            report.samples[index].aggregate_elapsed_ns = Some(elapsed);
            persist(output, report)?;
            if let Some(reason) = interrupted {
                return Err(reason);
            }
            if failed {
                return Err(format!("{} measured invocation failed", gate.label()));
            }
            require_fixtures(root, &names)?;
        }
    }
    Ok(())
}

type CapturedInvocation = (u64, i32, Vec<u8>, Vec<u8>);
fn invoke(
    name: &str,
    root: &Path,
    executable: Option<&Path>,
    gate: Gate,
) -> Result<CapturedInvocation, String> {
    let path = root.join(name);
    let path = path.to_str().ok_or("fixture path must be UTF-8")?;
    let host = host_surface_label(gate.host_surface());
    let started = Instant::now();
    let (code, stdout, stderr) = match executable {
        Some(image) => {
            let output = Command::new(image)
                .args([
                    "run",
                    path,
                    "--host-surface",
                    host,
                    "--jobs",
                    &scriptw_engine::compilation_jobs().to_string(),
                ])
                .output()
                .map_err(|error| format!("cannot start cold measured CLI: {error}"))?;
            (
                output.status.code().unwrap_or(-1),
                output.stdout,
                output.stderr,
            )
        }
        None => {
            let capture = crate::run_cli_capture([
                "run", "--execution-backend", "wasm", "--host-surface", host, path,
            ]);
            (i32::from(capture.exit_code), capture.stdout, capture.stderr)
        }
    };
    Ok((elapsed_ns(started)?, code, stdout, stderr))
}
fn retain_invocation(
    output: &Path,
    gate: Gate,
    repetition: usize,
    phase: &str,
    index: usize,
    name: &str,
    run: CapturedInvocation,
) -> Result<Invocation, String> {
    let stem = format!("{}-{repetition}-{phase}-{index}", gate.label());
    Ok(Invocation {
        case: name.into(),
        elapsed_ns: run.0,
        exit_code: run.1,
        stdout: save_log(output, &format!("{stem}.stdout"), &run.2)?,
        stderr: save_log(output, &format!("{stem}.stderr"), &run.3)?,
    })
}
fn require_fixtures(root: &Path, names: &[&str]) -> Result<(), String> {
    for &name in names {
        let source = fixture_source(name)?;
        if fs::read(root.join(name))
            .map_err(|error| format!("cannot read fixture {name}: {error}"))?
            != source.as_bytes()
        {
            return Err(format!(
                "fixture {name} differs from its embedded workload bytes"
            ));
        }
    }
    Ok(())
}
fn fixture_source(name: &str) -> Result<&'static str, String> {
    chunk_cases::CASES
        .iter()
        .position(|&candidate| candidate == name)
        .map(|index| chunk_cases::SOURCES[index])
        .ok_or_else(|| format!("unregistered fixture {name}"))
}
fn elapsed_ns(started: Instant) -> Result<u64, String> {
    u64::try_from(started.elapsed().as_nanos())
        .map_err(|_| "duration exceeds u64 nanoseconds".into())
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn save_log(output: &Path, name: &str, bytes: &[u8]) -> Result<Log, String> {
    if bytes.len() as u64 > LOG_MAX_BYTES {
        return Err("benchmark output exceeds the 64 MiB per-log retention bound".into());
    }
    let path = format!("logs/{name}");
    write_new(&output.join(&path), bytes)?;
    Ok(Log {
        path,
        bytes: bytes.len(),
        sha256: digest(bytes),
    })
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("cannot retain {}: {error}", path.display()))
}
fn persist(output: &Path, report: &Report) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report).map_err(|error| error.to_string())?;
    let next = output.join("report.next.json");
    write_new(&next, &bytes)?;
    fs::rename(next, output.join("report.json"))
        .map_err(|error| format!("cannot checkpoint report: {error}"))
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let file =
        fs::File::open(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "{} exceeds its retained evidence byte limit",
            path.display()
        ));
    }
    Ok(bytes)
}
fn verify_retained_evidence(report: &Report, path: &Path) -> Result<(), String> {
    report.summaries()?;
    let root = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|error| error.to_string())?;
    for log in report.logs() {
        log.validate()?;
        let path = root
            .join(&log.path)
            .canonicalize()
            .map_err(|error| format!("baseline log is missing: {error}"))?;
        if !path.starts_with(&root) {
            return Err("baseline log escapes its retained artifact root".into());
        }
        let bytes = read_bounded(&path, LOG_MAX_BYTES)?;
        if bytes.len() != log.bytes || digest(&bytes) != log.sha256 {
            return Err("baseline log bytes disagree with its evidence identity".into());
        }
    }
    for fixture in &report.identity.fixtures {
        if !chunk_cases::CASES.contains(&fixture.name.as_str()) {
            return Err("baseline contains an unregistered fixture".into());
        }
        let bytes = read_bounded(&root.join("workload").join(&fixture.name), LOG_MAX_BYTES)?;
        if bytes.len() != fixture.bytes || digest(&bytes) != fixture.sha256 {
            return Err("baseline fixture evidence is missing or changed".into());
        }
    }
    Ok(())
}
fn checked_cli_image(compiler: &CompilerProvenance) -> Result<PathBuf, String> {
    // Linux can select the parent's loaded image even if its public pathname is
    // replaced. Other supported platforms verify the selected pathname digest.
    #[cfg(target_os = "linux")]
    let executable = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
    #[cfg(not(target_os = "linux"))]
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let output = Command::new(&executable)
        .arg("compiler-identity")
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("cold reporting requires an actual scriptw executable".into());
    }
    let identity: CompilerProvenance = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid cold CLI identity: {error}"))?;
    if &identity != compiler {
        return Err("cold selected image differs from the running compiler".into());
    }
    Ok(executable)
}
fn capture_identity(options: &Options, root: &Path) -> Result<Identity, String> {
    let fixtures = chunk_cases::CASES
        .iter()
        .zip(chunk_cases::SOURCES)
        .map(|(&name, source)| Fixture {
            name: name.into(),
            bytes: source.len(),
            sha256: digest(source.as_bytes()),
        })
        .collect();
    let mut corpus = Sha256::new();
    corpus.update(b"scriptw-existing-perf-corpus-v1");
    for (&name, source) in chunk_cases::CASES.iter().zip(chunk_cases::SOURCES) {
        corpus.update((name.len() as u64).to_le_bytes());
        corpus.update(name.as_bytes());
        corpus.update((source.len() as u64).to_le_bytes());
        corpus.update(source.as_bytes());
    }
    let cache = scriptw_engine::cache_status().map_err(|error| error.to_string())?;
    let cache_paths_and_limits = [
        ("function", cache.function_cache),
        ("module", cache.module_cache),
        ("build-module", cache.build_module_cache),
        ("program", cache.program_cache),
        ("legacy-wasmtime", cache.legacy_wasmtime_cache),
    ]
    .into_iter()
    .map(|(name, status)| {
        (
            name.into(),
            status.path.to_string_lossy().into_owned(),
            status.limit_bytes,
        )
    })
    .collect();
    Ok(Identity {
        compiler: CompilerProvenance::current()?,
        build: Build {
            rustc_verbose: env!("SCRIPTW_PERF_RUSTC").into(),
            target: env!("SCRIPTW_PERF_TARGET").into(),
            host: env!("SCRIPTW_PERF_HOST").into(),
            profile: env!("SCRIPTW_PERF_PROFILE").into(),
            opt_level: env!("SCRIPTW_PERF_OPT_LEVEL").into(),
            debug: env!("SCRIPTW_PERF_DEBUG").into(),
            rustflags: env!("SCRIPTW_PERF_RUSTFLAGS").into(),
            spec_exec_oracle_linked: cfg!(feature = "spec-exec-oracle"),
        },
        platform: capture_platform(&options.machine_label)?,
        configuration: Configuration {
            backend: "wasm-aot".into(),
            fixture_root: root.to_string_lossy().into_owned(),
            working_directory: std::env::current_dir()
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .into_owned(),
            host_surface: match options.mode {
                MeasurementMode::WholeCli => "per-gate",
                MeasurementMode::Compiler | MeasurementMode::Runtime => host_surface_label(CORPUS_HOST_SURFACE),
            }.into(),
            compilation_jobs: scriptw_engine::compilation_jobs(),
            cache_paths_and_limits,
            idle_machine_acknowledged: true,
            environment: [
                "SCRIPTW_CACHE_DIR",
                "SCRIPTW_CACHE_LIMIT_BYTES",
                "SCRIPTW_FUNCTION_CACHE_LIMIT_BYTES",
                "SCRIPTW_MODULE_CACHE_LIMIT_BYTES",
                "SCRIPTW_PROGRAM_CACHE_LIMIT_BYTES",
                "SCRIPTW_VERIFY_FUNCTION_CACHE",
                "SCRIPTW_WASM_TRACE",
                "SCRIPTW_WASM_TRACE_DUMP",
                "SCRIPTW_EMIT_SIZE_REPORT",
                "SCRIPTW_WASM_DUMP",
                "RAYON_NUM_THREADS",
                "TMPDIR",
            ]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    std::env::var_os(name).map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect(),
        },
        measurement_policy: POLICY.into(),
        corpus_sha256: format!("{:x}", corpus.finalize()),
        fixtures,
        gates: options.gates.clone(),
        samples_per_gate: options.samples,
    })
}
fn capture_platform(label: &str) -> Result<Platform, String> {
    let cpus = std::thread::available_parallelism()
        .map_err(|error| error.to_string())?
        .get();
    #[cfg(target_os = "linux")]
    {
        let cpuinfo = fs::read_to_string("/proc/cpuinfo").map_err(|error| error.to_string())?;
        let model = cpuinfo
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(key, _)| {
                        matches!(key.trim(), "model name" | "Hardware" | "Processor")
                    })
                    .map(|(_, value)| value.trim().to_owned())
            })
            .ok_or("cannot identify CPU model")?;
        let memory = fs::read_to_string("/proc/meminfo").map_err(|error| error.to_string())?;
        let physical_memory = memory
            .lines()
            .find(|line| line.starts_with("MemTotal:"))
            .ok_or("cannot identify physical memory")?
            .to_owned();
        let status = fs::read_to_string("/proc/self/status").map_err(|error| error.to_string())?;
        let affinity = status
            .lines()
            .find(|line| line.starts_with("Cpus_allowed_list:"))
            .map(str::to_owned);
        let mut limits = Vec::new();
        let groups = fs::read_to_string("/proc/self/cgroup").map_err(|error| error.to_string())?;
        if let Some(path) = groups.lines().find_map(|line| line.strip_prefix("0::")) {
            let relative = Path::new(path)
                .strip_prefix("/")
                .map_err(|error| error.to_string())?;
            if relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
            {
                return Err("invalid cgroup path".into());
            }
            let root = Path::new("/sys/fs/cgroup").join(relative);
            for name in [
                "memory.max",
                "memory.swap.max",
                "cpu.max",
                "cpuset.cpus.effective",
            ] {
                let path = root.join(name);
                if path.exists() {
                    limits.push((
                        name.into(),
                        fs::read_to_string(path)
                            .map_err(|error| error.to_string())?
                            .trim()
                            .into(),
                    ));
                }
            }
        }
        Ok(Platform {
            os: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            kernel: fs::read_to_string("/proc/sys/kernel/osrelease")
                .map_err(|error| error.to_string())?
                .trim()
                .into(),
            cpu_model: model,
            logical_cpus_available: cpus,
            physical_memory,
            process_cpu_affinity: affinity,
            cgroup_limits: limits,
            machine_label: label.into(),
        })
    }
    #[cfg(target_os = "macos")]
    {
        let inspect = |name: &str| -> Result<String, String> {
            let output = Command::new("sysctl")
                .args(["-n", name])
                .output()
                .map_err(|error| error.to_string())?;
            if !output.status.success() {
                return Err(format!("cannot identify {name}"));
            }
            String::from_utf8(output.stdout)
                .map(|value| value.trim().into())
                .map_err(|error| error.to_string())
        };
        Ok(Platform {
            os: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            kernel: inspect("kern.osrelease")?,
            cpu_model: inspect("machdep.cpu.brand_string")?,
            logical_cpus_available: cpus,
            physical_memory: inspect("hw.memsize")?,
            process_cpu_affinity: None,
            cgroup_limits: Vec::new(),
            machine_label: label.into(),
        })
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (label, cpus);
        Err("performance platform identification supports Linux and macOS; no unidentified timing starts".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn arguments() -> Vec<String> {
        [
            "report",
            "--output-dir",
            "unused-report",
            "--samples",
            "3",
            "--machine-label",
            "unit-fixture",
            "--idle-machine",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }
    #[test]
    fn timing_admission_requires_explicit_idle_acknowledgment_and_finite_sample_plan() {
        let mut missing = arguments();
        missing.pop();
        assert!(parse(missing).is_err());
        for count in ["0", "1", "2", "101", "unbounded"] {
            let mut args = arguments();
            args[4] = count.into();
            assert!(parse(args).is_err());
        }
        let parsed = parse(arguments()).unwrap();
        assert_eq!(parsed.samples, 3);
        assert_eq!(parsed.gates, Gate::ALL);
        let mut duplicate = arguments();
        duplicate.extend(["--samples".into(), "4".into()]);
        assert!(parse(duplicate).is_err());
    }
    #[test]
    fn corpus_host_helpers_are_authorized_only_for_the_chunk_gate() {
        let root = std::env::temp_dir().join(format!("scriptw-perf-host-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        write_new(&root.join("host.js"),
            b"typeof __scriptwAssertThrows + ':' + typeof __scriptwDetachArrayBuffer;").unwrap();
        for (gate, expected) in [
            (Gate::WarmChunk, "string(function:function)"),
            (Gate::WarmExact, "string(undefined:undefined)"),
            (Gate::ColdExact, "string(undefined:undefined)"),
        ] {
            let (_, code, stdout, stderr) = invoke("host.js", &root, None, gate).unwrap();
            assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr));
            assert!(String::from_utf8_lossy(&stdout).contains(expected),
                "{}", String::from_utf8_lossy(&stdout));
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn actual_cli_rejects_unacknowledged_timing_before_output_creation() {
        let path = std::env::temp_dir().join(format!("scriptw-perf-rejected-{}", std::process::id()));
        assert!(!path.exists());
        let output = crate::run_cli_capture([
            "performance",
            "report",
            "--output-dir",
            path.to_str().unwrap(),
            "--samples",
            "3",
            "--machine-label",
            "unit-fixture",
        ]);
        assert_ne!(output.exit_code, 0);
        assert!(String::from_utf8_lossy(&output.stderr).contains("--idle-machine"));
        assert!(!path.exists());
    }
    #[test]
    fn fresh_artifacts_and_bounded_evidence_refuse_overwrite_or_oversized_reads() {
        let root = std::env::temp_dir().join(format!("scriptw-perf-evidence-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let file = root.join("evidence");
        write_new(&file, b"first evidence").unwrap();
        assert!(write_new(&file, b"replacement").is_err());
        assert_eq!(fs::read(&file).unwrap(), b"first evidence");
        assert!(read_bounded(&file, 1).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
