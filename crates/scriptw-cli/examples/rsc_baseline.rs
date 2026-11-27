//! Retained production React Flight requests; setup and output checks are separate.
//! Run under scripts/limited_verification.py. Inputs are the unmodified rsc.js
//! bundle and Node-generated support.json from the existing React fixture driver.
// Rustc links an unused `--extern` crate only when the code names it; this
// makes `--features jemalloc` install the lib's global allocator here too.
// The whole process then runs on jemalloc, its execution phases included,
// unlike scriptw-precompiled-consumer and the runtime examples.
#[cfg(feature = "jemalloc")]
use scriptw_cli as _;
use scriptw_engine::{
    configure_compilation_jobs, configure_precompiled_wasm_runtime, configure_wasm_runtime,
    CompilationJobs, CompileOptions, CompilerIdentity, CompilerSourceRevision, EmbeddedEvaluation,
    EmbeddedInstance, EmbeddedRuntime, Engine, ExecutionProfile, ExecutionProfileUse, ForeignValue,
    JsCompletion, JsPromiseState, JsValue, NativeCompilationPolicy, PrecompiledEmbeddedProgram,
    RealmBuilder, WasmStoreState,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

const WRAPPER: &str = "\nexport const run = globalThis.applicationRun;\nexport function encode(value) { return JSON.stringify(value); }\n";
const DEADLINE: Duration = Duration::from_secs(30);
const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Copy)]
enum NativeMode {
    Fast,
    Speed,
    FastBacktracking,
    SpeedSinglePass,
}

impl NativeMode {
    fn parse(value: Option<String>) -> Result<Self> {
        match value.as_deref() {
            None | Some("fast") => Ok(Self::Fast),
            Some("speed") => Ok(Self::Speed),
            Some("fast-backtracking") => Ok(Self::FastBacktracking),
            Some("speed-single-pass") => Ok(Self::SpeedSinglePass),
            Some(value) => {
                Err(format!("unknown native mode {value:?}; expected fast, speed, fast-backtracking or speed-single-pass").into())
            }
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Speed => "speed",
            Self::FastBacktracking => "fast-backtracking",
            Self::SpeedSinglePass => "speed-single-pass",
        }
    }

    /// `fast` and `speed` are the public embedding policies. The excluded
    /// diagnostics cross the two settings that a policy keeps together.
    fn configure(self, config: &mut wasmtime::Config, jobs: CompilationJobs) {
        match self {
            Self::Fast => configure_wasm_runtime(config, NativeCompilationPolicy::Fast, jobs),
            Self::Speed => configure_wasm_runtime(config, NativeCompilationPolicy::Speed, jobs),
            Self::FastBacktracking => {
                configure_wasm_runtime(config, NativeCompilationPolicy::Fast, jobs);
                config.cranelift_regalloc_algorithm(wasmtime::RegallocAlgorithm::Backtracking);
            }
            Self::SpeedSinglePass => {
                configure_wasm_runtime(config, NativeCompilationPolicy::Speed, jobs);
                config.cranelift_regalloc_algorithm(wasmtime::RegallocAlgorithm::SinglePass);
            }
        }
    }
}

// Excluded diagnostics only. The parent owns the inherited connected socket;
// this process takes ownership of its child descriptor. Normal runs have no
// profiler, control socket, or sampling barriers.
struct ProfileBarrier {
    #[cfg(target_os = "linux")]
    socket: std::os::unix::net::UnixStream,
    nonce: String,
    mode: String,
}

impl ProfileBarrier {
    fn from_environment() -> Result<Option<Self>> {
        let names = [
            "SCRIPTW_RSC_PROFILE_FD",
            "SCRIPTW_RSC_PROFILE_NONCE",
            "SCRIPTW_RSC_PROFILE_MODE",
        ];
        let values = names.map(std::env::var_os);
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        #[cfg(not(target_os = "linux"))]
        return Err("RSC profiling barriers require Linux".into());
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::FromRawFd;
            let [fd, nonce, mode] = values.map(|value| value.and_then(|v| v.into_string().ok()));
            let fd: i32 = fd.ok_or("missing profile FD")?.parse()?;
            let nonce = nonce.ok_or("missing profile nonce")?;
            let mode = mode.ok_or("missing profile mode")?;
            if fd < 3
                || nonce.len() != 32
                || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
                || !matches!(mode.as_str(), "sync" | "async")
            {
                return Err("invalid RSC profiling configuration".into());
            }
            let descriptor = fs::read_link(format!("/proc/self/fd/{fd}"))?;
            if !descriptor.to_string_lossy().starts_with("socket:[") {
                return Err("profile FD is not an inherited socket".into());
            }
            let map = format!("/tmp/perf-{}.map", std::process::id());
            if fs::symlink_metadata(map).is_ok() {
                return Err("refusing to append to a preexisting process perf map".into());
            }
            // The explicitly opted-in diagnostic parent transfers this open FD.
            let socket = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
            socket.peer_addr()?;
            socket.set_read_timeout(Some(DEADLINE))?;
            socket.set_write_timeout(Some(DEADLINE))?;
            Ok(Some(Self {
                socket,
                nonce,
                mode,
            }))
        }
    }

    fn exchange(&mut self, stage: &str, index: usize) -> Result<()> {
        #[cfg(not(target_os = "linux"))]
        return Err("RSC profiling barriers require Linux".into());
        #[cfg(target_os = "linux")]
        {
            use std::io::Read;
            let message = format!("SWRSC1 {} {} {} {}\n", self.nonce, stage, self.mode, index);
            self.socket.write_all(message.as_bytes())?;
            let expected = message.replacen("SWRSC1", "SWACK1", 1);
            let mut received = vec![0; expected.len()];
            self.socket.read_exact(&mut received)?;
            if received != expected.as_bytes() {
                return Err("incorrect RSC profiling acknowledgment".into());
            }
            Ok(())
        }
    }
}

/// Excluded diagnostics of profile-guided compilation, never a scored
/// cohort. `SCRIPTW_RSC_PROFILE_COLLECT=<path>` builds the instrumented
/// program, trains it on this run's requests and writes their profile to a
/// new file; `SCRIPTW_RSC_PROFILE_USE=<path>` builds with a profile of this
/// same bundle and compiler; `SCRIPTW_RSC_PROFILE_ADVERSARIAL=<seed>` builds
/// with a wrong one (every read site hot, its kind rotated from the seed;
/// only with the `profile-experiments` feature). The exact Flight output
/// check is unchanged in all three.
enum ExecutionProfileHook {
    Collect(PathBuf),
    Use(PathBuf),
    #[cfg(feature = "profile-experiments")]
    Adversarial(usize),
}

impl ExecutionProfileHook {
    fn from_environment() -> Result<Option<Self>> {
        let collect = std::env::var_os("SCRIPTW_RSC_PROFILE_COLLECT").map(PathBuf::from);
        let apply = std::env::var_os("SCRIPTW_RSC_PROFILE_USE").map(PathBuf::from);
        let adversarial = std::env::var("SCRIPTW_RSC_PROFILE_ADVERSARIAL").ok();
        Ok(match (collect, apply, adversarial) {
            (None, None, None) => None,
            (Some(path), None, None) => Some(Self::Collect(path)),
            (None, Some(path), None) => Some(Self::Use(path)),
            #[cfg(feature = "profile-experiments")]
            (None, None, Some(seed)) => {
                Some(Self::Adversarial(seed.parse().map_err(|_| {
                    "SCRIPTW_RSC_PROFILE_ADVERSARIAL must be a seed"
                })?))
            }
            #[cfg(not(feature = "profile-experiments"))]
            (None, None, Some(_)) => {
                return Err(
                    "SCRIPTW_RSC_PROFILE_ADVERSARIAL needs the profile-experiments feature".into(),
                )
            }
            _ => return Err("choose at most one execution profile diagnostic".into()),
        })
    }

    fn mode(&self) -> &'static str {
        match self {
            Self::Collect(_) => "collect",
            Self::Use(_) => "apply",
            #[cfg(feature = "profile-experiments")]
            Self::Adversarial(_) => "adversarial",
        }
    }
}

/// Excluded diagnostic of emitter headroom, never a scored cohort:
/// `SCRIPTW_RSC_PORTABLE_BUNDLE=<path>` runs a portable bundle in place of the
/// artifact this run emitted, after the unchanged front end and emission. The
/// bundle is this build's P and R for rsc.js as `SCRIPTW_RSC_ARTIFACT_DIR`
/// dumps them, with either module rewritten by an external Wasm optimiser and
/// its checksums and runtime key recomputed. `Engine::load_portable_bundle`
/// admits it as it admits any portable bundle (Wasm validation, R layout and
/// Intl identity), and the exact Flight output check is unchanged. A rewritten
/// R misses the embedded native R image, so native preparation then includes
/// compiling R. Only with the `module-replacement` feature.
#[cfg(feature = "module-replacement")]
fn replace_artifact(
    compiler: &Engine,
    emitted: &scriptw_engine::Artifact,
    path: &std::path::Path,
) -> Result<(scriptw_engine::Artifact, Value)> {
    let bytes = fs::read(path)?;
    let replaced = compiler.load_portable_bundle(&bytes)?;
    let report = json!({
        "bundle":path,"bundle_sha256":hash(&bytes),
        "emitted_program_wasm_sha256":hash(&emitted.bytes),
        "program_replaced":replaced.bytes != emitted.bytes,
        "runtime_replaced":replaced.runtime != emitted.runtime,
        "excluded_from_scored_cohorts":true,
    });
    Ok((replaced, report))
}

// rsc_baseline uses only the smaps_rollup endpoint snapshot.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "runtime_precompiled/residency.rs"]
mod residency;

/// Process memory at one stage, outside every request clock. VmHWM is the
/// kernel's high-water mark since the process started. It is never reset:
/// writing 5 to /proc/self/clear_refs would also lower ru_maxrss, and GNU
/// time's whole-child peak would then be silently understated.
fn memory(stage: &str) -> Result<Value> {
    #[cfg(target_os = "linux")]
    {
        use std::io::Read;
        let mut status = String::new();
        fs::File::open("/proc/self/status")?
            .take(64 * 1024)
            .read_to_string(&mut status)?;
        let field = |name: &str| -> Result<u64> {
            let mut matches = status.lines().filter_map(|line| line.strip_prefix(name));
            let mut words = matches
                .next()
                .ok_or_else(|| format!("missing {name}"))?
                .split_whitespace();
            let kib: u64 = words.next().ok_or("missing memory count")?.parse()?;
            if words.next() != Some("kB") || words.next().is_some() || matches.next().is_some() {
                return Err(format!("ambiguous {name}").into());
            }
            Ok(kib.checked_mul(1024).ok_or("memory count overflow")?)
        };
        Ok(json!({"stage":stage,"vm_hwm_bytes":field("VmHWM:")?,
            "vm_rss_bytes":field("VmRSS:")?,"smaps_rollup":residency::snapshot()?}))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = stage;
        Ok(Value::Null)
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn normal(completion: JsCompletion) -> Result<JsValue> {
    match completion {
        JsCompletion::Normal(value) => Ok(value),
        JsCompletion::Throw(value) => Err(format!("JavaScript operation threw: {value:?}").into()),
    }
}

fn initialized(
    instance: &EmbeddedInstance,
    store: &mut wasmtime::Store<WasmStoreState>,
    mut evaluation: EmbeddedEvaluation,
) -> Result<()> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        match evaluation {
            EmbeddedEvaluation::Settled(completion) => {
                normal(completion)?;
                return Ok(());
            }
            EmbeddedEvaluation::Pending => {
                if Instant::now() >= deadline {
                    return Err("module initialization exceeded its deadline".into());
                }
                thread::yield_now();
                evaluation = instance.pump(store)?;
            }
        }
    }
}

fn flight(
    instance: &EmbeddedInstance,
    store: &mut wasmtime::Store<WasmStoreState>,
    run: &JsValue,
    argument: &JsValue,
) -> Result<JsValue> {
    let deadline = Instant::now() + DEADLINE;
    let promise = normal(instance.call(store, run, None, &[argument.clone()])?)?;
    loop {
        match instance.promise_state(store, &promise)? {
            JsPromiseState::Fulfilled(value) => return Ok(value),
            JsPromiseState::Rejected(value) => {
                return Err(format!("React Flight request rejected: {value:?}").into());
            }
            JsPromiseState::Pending => {
                if Instant::now() >= deadline {
                    return Err(
                        "React Flight request did not fully drain before its deadline".into(),
                    );
                }
                thread::yield_now();
                if let EmbeddedEvaluation::Settled(completion) = instance.pump(store)? {
                    normal(completion)?;
                }
            }
        }
    }
}

fn count(value: Option<String>, default: usize, name: &str) -> Result<usize> {
    match value {
        Some(value) => value
            .parse()
            .map_err(|error| format!("invalid {name}: {error}").into()),
        None => Ok(default),
    }
}

fn run() -> Result<Value> {
    let mut profile = ProfileBarrier::from_environment()?;
    let mut args = std::env::args().skip(1);
    let source_path = PathBuf::from(args.next().ok_or(
        "usage: rsc_baseline <rsc.js> <support.json> <output.json> [samples=7] [warmups=3] [native-mode=fast|speed|fast-backtracking|speed-single-pass] [case=all|rsc-sync|rsc-async]",
    )?);
    let support_path = PathBuf::from(args.next().ok_or("missing support.json")?);
    let output = PathBuf::from(args.next().ok_or("missing output.json")?);
    let samples = count(args.next(), 7, "samples")?;
    let warmups = count(args.next(), 3, "warmups")?;
    let native_mode = NativeMode::parse(args.next())?;
    let selected_case = args.next().unwrap_or_else(|| "all".into());
    if !matches!(selected_case.as_str(), "all" | "rsc-sync" | "rsc-async") {
        return Err("case must be all, rsc-sync or rsc-async".into());
    }
    if let Some(profile) = &profile {
        if selected_case != "all" && selected_case != format!("rsc-{}", profile.mode) {
            return Err("selected case differs from profiling mode".into());
        }
    }
    let calls = samples
        .checked_add(warmups)
        .ok_or("sample count overflow")?;
    if args.next().is_some() || samples == 0 || calls > 1000 {
        return Err("require 1..1000 samples and at most 1000 total calls per mode".into());
    }
    let original = fs::read_to_string(&source_path)?;
    let support_bytes = fs::read(&support_path)?;
    let support: Value = serde_json::from_slice(&support_bytes)?;
    if support["versions"]["react"] != "19.3.1"
        || support["versions"]["react-server-dom-webpack"] != "19.3.1"
    {
        return Err("expected pinned production React and react-server-dom-webpack 19.3.1".into());
    }
    let source = format!("{original}{WRAPPER}");
    let identity = CompilerIdentity::current()?;
    let revision = match identity.source_revision() {
        CompilerSourceRevision::GitCommit(commit) => Some(commit.as_str()),
        CompilerSourceRevision::UnversionedArchive => None,
    };
    // Excluded diagnostic: load an image from `scriptw precompile` (of rsc.js
    // plus WRAPPER) on a compiler-disabled Engine instead of compiling.
    let precompiled_image = std::env::var_os("SCRIPTW_RSC_PRECOMPILED_IMAGE").map(PathBuf::from);
    let execution_profile_hook = ExecutionProfileHook::from_environment()?;
    if precompiled_image.is_some()
        && (profile.is_some()
            || std::env::var_os("SCRIPTW_RSC_ARTIFACT_DIR").is_some()
            || execution_profile_hook.is_some())
    {
        return Err(
            "a precompiled image excludes profiling, artifact dumps and execution profiles".into(),
        );
    }
    let replacement_bundle = std::env::var_os("SCRIPTW_RSC_PORTABLE_BUNDLE").map(PathBuf::from);
    #[cfg(not(feature = "module-replacement"))]
    if replacement_bundle.is_some() {
        return Err("SCRIPTW_RSC_PORTABLE_BUNDLE needs the module-replacement feature".into());
    }
    if replacement_bundle.is_some()
        && (precompiled_image.is_some()
            || std::env::var_os("SCRIPTW_RSC_ARTIFACT_DIR").is_some()
            || execution_profile_hook.is_some())
    {
        return Err(
            "a replacement bundle excludes precompiled images, artifact dumps and execution profiles"
                .into(),
        );
    }
    #[cfg_attr(not(feature = "module-replacement"), allow(unused_mut))]
    let mut replacement_report: Option<Value> = None;
    let mut execution_profile_report = None;
    let load_started;
    let mut phases = serde_json::Map::new();
    // A precompiled image runs no front end or emitter: that stage stays null.
    let mut front_and_emit_high_water = Value::Null;
    let mut artifact_directory = None;
    enum ProgramWasmEvidence {
        Owned(Vec<u8>),
        Precompiled(PrecompiledEmbeddedProgram),
    }
    impl ProgramWasmEvidence {
        fn as_slice(&self) -> &[u8] {
            match self {
                Self::Owned(bytes) => bytes,
                Self::Precompiled(image) => image.program_wasm(),
            }
        }
    }
    let (native, runtime, configuration, program_wasm, runtime_identity) = if let Some(path) =
        &precompiled_image
    {
        load_started = Instant::now();
        // SAFETY: the operator supplies this build's trusted precompile output
        // and keeps its inode unchanged throughout execution.
        let image = unsafe { PrecompiledEmbeddedProgram::from_file(path) }?;
        phases.insert(
            "image_read_and_admission".into(),
            json!(load_started.elapsed().as_nanos()),
        );
        let started = Instant::now();
        let mut config = wasmtime::Config::new();
        configure_precompiled_wasm_runtime(&mut config);
        let native = wasmtime::Engine::new(&config)?;
        phases.insert("engine_setup".into(), json!(started.elapsed().as_nanos()));
        let started = Instant::now();
        let runtime = EmbeddedRuntime::from_precompiled(&native, &image)?;
        phases.insert(
            "native_preparation".into(),
            json!(started.elapsed().as_nanos()),
        );
        let configuration = json!({
            "wasmtime":"47.0.0","precompiled_image":path,
            "precompiled_image_sha256":image.image_sha256().iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "precompiled_image_bytes":image.as_bytes().len(),
            "native_policy":"chosen by scriptw precompile; native-mode names it",
            "compiler_enabled":false,"target":native.get_target(),
            "target_features_policy":"host-inferred features, as scriptw precompile used",
            "collector":native.get_collector().map(|value|format!("{value:?}")),
            "driver_worker_stack_bytes":WORKER_STACK_BYTES,
            "store_limiter":"EmbeddedRuntime::new_state limits_mut",
            "pending_deadline_ms":DEADLINE.as_millis(),
        });
        let runtime_identity = image.runtime_key().map(|key| {
            json!({
                "key":key.iter().map(|byte|format!("{byte:02x}")).collect::<String>(),
            })
        });
        (
            native,
            runtime,
            configuration,
            ProgramWasmEvidence::Precompiled(image),
            runtime_identity,
        )
    } else {
        // Scored cohorts compile serially. SCRIPTW_RSC_COMPILATION_JOBS selects an
        // excluded parallel-compilation diagnostic, which benchmark-rsc.py refuses.
        let jobs =
            configure_compilation_jobs(match std::env::var("SCRIPTW_RSC_COMPILATION_JOBS") {
                Ok(value) => value
                    .parse::<std::num::NonZeroUsize>()
                    .map_err(|_| "SCRIPTW_RSC_COMPILATION_JOBS must be a positive integer")?
                    .get(),
                Err(std::env::VarError::NotPresent) => 1,
                Err(error) => return Err(error.into()),
            })?;
        let compiler = Engine::new(RealmBuilder::new().build());
        let started = Instant::now();
        let execution_profile = match &execution_profile_hook {
            None => ExecutionProfileUse::None,
            Some(ExecutionProfileHook::Collect(_)) => ExecutionProfileUse::Collect,
            Some(ExecutionProfileHook::Use(path)) => {
                let bytes = fs::read(path)?;
                let applied = ExecutionProfile::decode(&bytes)?;
                execution_profile_report = Some(json!({
                    "profile":path,"profile_sha256":hash(&bytes),
                    "training_requests":applied.training_requests().get(),
                    "hot_read_sites":applied.hot_reads().count(),
                }));
                ExecutionProfileUse::Apply(std::sync::Arc::new(applied))
            }
            #[cfg(feature = "profile-experiments")]
            Some(ExecutionProfileHook::Adversarial(seed)) => {
                let wrong = compiler
                    .adversarial_execution_profile(
                        &source,
                        scriptw_engine::EmbeddedModuleGoal::Module,
                        &CompileOptions::default(),
                        *seed,
                    )?
                    .ok_or("the bundle does not compile unprofiled")?;
                execution_profile_report = Some(json!({
                    "seed":seed,"hot_read_sites":wrong.hot_reads().count(),
                }));
                ExecutionProfileUse::Apply(std::sync::Arc::new(wrong))
            }
        };
        let unit = compiler.compile_module(
            &source,
            CompileOptions {
                execution_profile,
                ..CompileOptions::default()
            },
        )?;
        let front_and_lower_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let artifact = compiler.emit_wasm(&unit)?;
        let wasm_emission_ns = started.elapsed().as_nanos();
        artifact_directory = std::env::var_os("SCRIPTW_RSC_ARTIFACT_DIR").map(PathBuf::from);
        if let Some(directory) = &artifact_directory {
            fs::create_dir(directory)?;
            fs::write(directory.join("program.wasm"), &artifact.bytes)?;
            if let Some(runtime) = &artifact.runtime {
                fs::write(directory.join("runtime.wasm"), runtime.bytes())?;
            }
            fs::write(
                directory.join("portable.bundle"),
                artifact
                    .encode_portable_bundle(&scriptw_engine::IntlCompilationProfile::Minimal)?,
            )?;
        }
        #[cfg(feature = "module-replacement")]
        let artifact = match &replacement_bundle {
            Some(path) => {
                let (replaced, report) = replace_artifact(&compiler, &artifact, path)?;
                replacement_report = Some(report);
                replaced
            }
            None => artifact,
        };
        drop(unit);
        drop(compiler);
        front_and_emit_high_water = memory("after Wasm emission, source compiler dropped")?;
        // An embedder that compiles at boot starts here, with the artifact.
        load_started = Instant::now();
        let started = load_started;
        let mut config = wasmtime::Config::new();
        native_mode.configure(&mut config, jobs);
        config.target(env!("SCRIPTW_PERF_TARGET"))?;
        if profile.is_some() {
            config.profiler(wasmtime::ProfilingStrategy::PerfMap);
        }
        let native = wasmtime::Engine::new(&config)?;
        if let Some(directory) = &artifact_directory {
            // The native images under this policy, for disassembly.
            fs::write(
                directory.join("program.cwasm"),
                native.precompile_module(&artifact.bytes)?,
            )?;
            if let Some(runtime) = &artifact.runtime {
                fs::write(
                    directory.join("runtime.cwasm"),
                    native.precompile_module(runtime.bytes())?,
                )?;
            }
        }
        let engine_setup_ns = started.elapsed().as_nanos();
        let configuration = json!({
            "wasmtime":"47.0.0",
            "compilation_jobs":scriptw_engine::configured_compilation_jobs(),
            "cranelift_opt_level":native.get_cranelift_opt_level().map(|value|format!("{value:?}")),
            "native_parallel_compilation":native.get_parallel_compilation(),
            "register_allocation":native.get_cranelift_regalloc_algorithm().map(|value|format!("{value:?}")),
            "native_inlining":format!("{:?}", native.get_compiler_inlining()),
            "native_inlining_requested_flags":native.get_cranelift_flags_set()
                .filter(|(name, _)| name.starts_with("wasmtime_inlining_"))
                .collect::<std::collections::BTreeMap<_, _>>(),
            "target":native.get_target(),
            "target_features_policy":"explicit Config::target disables host-inferred feature flags",
            "collector":native.get_collector().map(|value|format!("{value:?}")),
            "gc_initial_bytes":native.get_gc_heap_initial_size(),
            "gc_reservation_bytes":native.get_gc_heap_reservation(),
            "gc_guard_bytes":native.get_gc_heap_guard_size(),
            "gc_heap_may_move":native.get_gc_heap_may_move(),
            "max_wasm_stack_bytes":native.get_max_wasm_stack(),
            "wasmtime_async_stack_bytes":native.get_async_stack_size(),
            "driver_worker_stack_bytes":WORKER_STACK_BYTES,
            "store_limiter":"EmbeddedRuntime::new_state limits_mut",
            "epoch_interruption":native.get_epoch_interruption(),
            "pending_deadline_ms":DEADLINE.as_millis(),
        });
        phases.insert("front_and_lower".into(), json!(front_and_lower_ns));
        phases.insert("wasm_emission".into(), json!(wasm_emission_ns));
        phases.insert(
            "source_to_wasm".into(),
            json!(front_and_lower_ns + wasm_emission_ns),
        );
        phases.insert("engine_setup".into(), json!(engine_setup_ns));
        let started = Instant::now();
        let runtime = EmbeddedRuntime::compile(&native, &artifact)?;
        phases.insert(
            "native_preparation".into(),
            json!(started.elapsed().as_nanos()),
        );
        let runtime_identity = artifact.runtime.as_ref().map(|runtime| json!({
            "key":runtime.key().as_bytes().iter().map(|byte|format!("{byte:02x}")).collect::<String>(),
            "wasm_sha256":hash(runtime.bytes()),"wasm_bytes":runtime.bytes().len(),
        }));
        (
            native,
            runtime,
            configuration,
            ProgramWasmEvidence::Owned(artifact.bytes),
            runtime_identity,
        )
    };
    let native_mode_scope = match native_mode {
        _ if precompiled_image.is_some() => {
            "excluded precompiled-image diagnostic; keep separate from compiling cohorts"
        }
        NativeMode::Fast => "matched Fast cohort",
        NativeMode::Speed => {
            "unscored Speed/Backtracking diagnostic; keep separate from Fast cohort"
        }
        NativeMode::FastBacktracking | NativeMode::SpeedSinglePass => {
            "excluded crossed compiler-policy diagnostic; keep separate from scored Fast/Speed cohorts"
        }
    };
    let compile_high_water = memory(if precompiled_image.is_some() {
        "after precompiled image admission and from_precompiled"
    } else {
        "after native preparation"
    })?;
    let started = Instant::now();
    let state = runtime.new_state(RealmBuilder::new().build(), ())?;
    let mut store = wasmtime::Store::new(&native, state);
    store.limiter(|state| state.limits_mut());
    let mut linker = wasmtime::Linker::<WasmStoreState>::new(&native);
    runtime.register(&mut store, &mut linker)?;
    let mut instance = runtime.instantiate(&mut store, &mut linker)?;
    phases.insert(
        "realm_and_instantiation".into(),
        json!(started.elapsed().as_nanos()),
    );
    let started = Instant::now();
    let evaluation = instance.initialize(&mut store)?;
    initialized(&instance, &mut store, evaluation)?;
    phases.insert(
        "module_initialization".into(),
        json!(started.elapsed().as_nanos()),
    );
    if !instance.take_output(&mut store)?.is_empty() {
        return Err(
            "bundle initialization emitted output; use rsc.js without capability autorun".into(),
        );
    }
    let run = normal(instance.get_export(&mut store, "run")?)?;
    let encode = normal(instance.get_export(&mut store, "encode")?)?;
    let mut cases = Vec::new();
    // Load (an image, or an emitted artifact to compile) through the end of
    // the first checked request, warm-up or not.
    let mut load_to_first_request_ns = None;
    let mut post_compile = None;
    for mode in ["sync", "async"] {
        if profile.as_ref().is_some_and(|profile| profile.mode != mode) {
            continue;
        }
        let name = format!("rsc-{mode}");
        if selected_case != "all" && selected_case != name {
            continue;
        }
        let expected = support["expected"][&name]
            .as_str()
            .ok_or("missing Node Flight expectation")?;
        let expected = expected.strip_suffix('\n').unwrap_or(expected);
        let argument = instance.value(
            &mut store,
            ForeignValue::StringUtf16(mode.encode_utf16().collect()),
        )?;
        let mut observations = Vec::new();
        let mut measured = Vec::new();
        for index in 0..calls {
            if index == warmups && post_compile.is_none() {
                post_compile = Some(memory("after the first selected case's warm-up requests")?);
            }
            if index >= warmups {
                if let Some(profile) = &mut profile {
                    profile.exchange("ready", index)?;
                }
            }
            let started = Instant::now();
            let value = flight(&instance, &mut store, &run, &argument)?;
            let elapsed_ns = started.elapsed().as_nanos();
            load_to_first_request_ns.get_or_insert_with(|| load_started.elapsed().as_nanos());
            if index >= warmups {
                if let Some(profile) = &mut profile {
                    profile.exchange("done", index)?;
                }
            }
            // JSON serialization, host string copying and exact Node validation
            // follow the request clock, as in react-driver.cjs server().
            let encoded = normal(instance.call(&mut store, &encode, None, &[value])?)?;
            let actual = String::from_utf16(&encoded.to_utf16(&mut store)?)?;
            if actual != expected {
                return Err(format!("{name} call {index} Flight differs from Node: actual sha256 {}, expected sha256 {}", hash(actual.as_bytes()), hash(expected.as_bytes())).into());
            }
            let events = instance.take_output(&mut store)?;
            if !events.is_empty() {
                return Err(format!(
                    "{name} call {index} emitted unexpected output/exception: {events:?}"
                )
                .into());
            }
            let warmup = index < warmups;
            observations.push(
                json!({"index":index,"warmup":warmup,"elapsed_ns":elapsed_ns,
                "exact_node_output_checked":true,"output_json_sha256":hash(actual.as_bytes()),
                "output_json_bytes":actual.len()}),
            );
            if !warmup {
                measured.push(elapsed_ns);
            }
        }
        measured.sort_unstable();
        let mut median_ns = measured[measured.len() / 2];
        if measured.len() % 2 == 0 {
            median_ns = (measured[measured.len() / 2 - 1] + median_ns) / 2;
        }
        let mean_ns = measured.iter().sum::<u128>() as f64 / samples as f64;
        let p95_ns = measured[(samples * 95).div_ceil(100) - 1];
        cases.push(json!({"case":name,"samples":samples,"warmups":warmups,
            "mean_ns":mean_ns,"p95_ns":p95_ns,"median_ns":median_ns,"observations":observations,
            "exact_node_output_checked":true,"expected_json_sha256":hash(expected.as_bytes()),
            "expected_json_bytes":expected.len()}));
    }
    // The counters also count module initialization and the encode() calls
    // after each request, so executions per request read slightly high.
    let training_requests = cases.len() * calls;
    if let Some(ExecutionProfileHook::Collect(path)) = &execution_profile_hook {
        let requests = u32::try_from(training_requests)
            .ok()
            .and_then(std::num::NonZeroU32::new)
            .ok_or("a training run needs at least one request")?;
        let bytes = instance.collect_profile(&mut store, requests)?.encode();
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?
            .write_all(&bytes)?;
        let collected = ExecutionProfile::decode(&bytes)?;
        execution_profile_report = Some(json!({
            "profile":path,"profile_sha256":hash(&bytes),"profile_bytes":bytes.len(),
            "training_requests":training_requests,
            "read_sites":collected.reads().count(),
            "hot_read_sites":collected.hot_reads().count(),
            "megamorphic_read_sites":collected.reads().filter(|(_, read)| read.megamorphic).count(),
        }));
    }
    let mut report = json!({
        "schema":"scriptw-react-retained-baseline-v1","status":"complete",
        "scope":"one process and one retained Realm/module; diagnostic, not a statistical comparison",
        "case_selection":selected_case,
        "gc_policy":"natural Wasmtime GC; no forced collection between calls",
        "request_clock":"retained call through fulfilled applicationRun Promise after complete Flight consumption; includes bridge and required pumps; excludes setup, JSON validation serialization, host string copying and output checking",
        "source_commit":revision,"source_fingerprint":identity.source_fingerprint().to_string(),
        "native_mode":native_mode.as_str(),
        "native_mode_scope":native_mode_scope,
        "executable_sha256":identity.executable_sha256().to_string(),
        "compiler_identity_scheme":CompilerIdentity::source_fingerprint_scheme(),
        "toolchain":{"rustc":env!("SCRIPTW_PERF_RUSTC"),"target":env!("SCRIPTW_PERF_TARGET"),
            "profile":env!("SCRIPTW_PERF_PROFILE"),"opt_level":env!("SCRIPTW_PERF_OPT_LEVEL"),
            "rustflags":env!("SCRIPTW_PERF_RUSTFLAGS")},
        "configuration":configuration,
        "inputs":{"source":source_path,"source_sha256":hash(original.as_bytes()),
            "source_bytes":original.len(),"wrapper":WRAPPER,"adapted_source_sha256":hash(source.as_bytes()),
            "adapted_source_bytes":source.len(),"support":support_path,"support_sha256":hash(&support_bytes),
            "versions":support["versions"]},
        "program_wasm_sha256":hash(program_wasm.as_slice()),"program_wasm_bytes":program_wasm.as_slice().len(),
        "runtime":runtime_identity,
        "phases_ns":phases,
        "load_to_first_request_ns":load_to_first_request_ns,
        "memory":{
            "front_and_emit_high_water":front_and_emit_high_water,
            "compile_high_water":compile_high_water,
            "post_compile":post_compile,
            "steady":memory("after the last request")?,
            "scope":"Linux /proc/self/status VmHWM (high water since process start, never reset) and VmRSS, plus smaps_rollup endpoints; compile_high_water bounds front end, emission and native preparation; post_compile and steady RSS in a compiling process include compiler memory the allocator retains",
        },
        "cases":cases,
    });
    if let Some(profile) = profile {
        report["profiling"] = json!({
            "excluded_from_scored_cohorts":true,"strategy":"PerfMap",
            "protocol":"SWRSC1","mode":profile.mode,"nonce":profile.nonce,
            "windows":samples,"warmups_excluded":warmups,
            "scope":"flight() only; setup, warmups and output validation outside every window",
        });
    }
    if let (Some(hook), Some(mut details)) = (&execution_profile_hook, execution_profile_report) {
        details["mode"] = json!(hook.mode());
        details["excluded_from_scored_cohorts"] = json!(true);
        report["execution_profile"] = details;
    }
    if let Some(path) = precompiled_image {
        report["precompiled_image"] = json!({
            "path":path,"excluded_from_scored_cohorts":true,
        });
    }
    if let Some(directory) = artifact_directory {
        report["artifact_dump"] = json!({
            "directory":directory,"excluded_from_scored_cohorts":true,
        });
    }
    if let Some(details) = replacement_report {
        report["portable_bundle"] = details;
    }
    let bytes = serde_json::to_vec_pretty(&report)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    Ok(report)
}

fn main() {
    let result = thread::Builder::new()
        .stack_size(WORKER_STACK_BYTES)
        .spawn(run)
        .map_err(|error| error.to_string())
        .and_then(|worker| {
            worker
                .join()
                .map_err(|_| "RSC baseline worker panicked".to_string())
        })
        .and_then(|result| result.map_err(|error| error.to_string()));
    match result {
        Ok(report) => println!("{report}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
