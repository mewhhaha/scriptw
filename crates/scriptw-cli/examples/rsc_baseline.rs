//! Retained production React Flight requests; setup and output checks are separate.
//! Run under scripts/limited_verification.py. Inputs are the unmodified rsc.js
//! bundle and Node-generated support.json from the existing React fixture driver.
use scriptw_engine::{
    configure_compilation_jobs, configure_wasm_runtime, CompileOptions, CompilerIdentity,
    CompilerSourceRevision, EmbeddedEvaluation, EmbeddedInstance, EmbeddedRuntime, Engine,
    ForeignValue, JsCompletion, JsPromiseState, JsValue, RealmBuilder, WasmStoreState,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const WRAPPER: &str = "\nexport const run = globalThis.applicationRun;\nexport function encode(value) { return JSON.stringify(value); }\n";
const DEADLINE: Duration = Duration::from_secs(30);
const EPOCH_PERIOD: Duration = Duration::from_millis(5);
const EPOCH_TICKS: u64 = 6_000;
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

    fn configure(self, config: &mut wasmtime::Config) {
        let (optimization, allocation) = match self {
            Self::Fast => (
                wasmtime::OptLevel::None,
                wasmtime::RegallocAlgorithm::SinglePass,
            ),
            Self::Speed => (
                wasmtime::OptLevel::Speed,
                wasmtime::RegallocAlgorithm::Backtracking,
            ),
            Self::FastBacktracking => (
                wasmtime::OptLevel::None,
                wasmtime::RegallocAlgorithm::Backtracking,
            ),
            Self::SpeedSinglePass => (
                wasmtime::OptLevel::Speed,
                wasmtime::RegallocAlgorithm::SinglePass,
            ),
        };
        config.cranelift_opt_level(optimization);
        config.cranelift_regalloc_algorithm(allocation);
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

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn normal(completion: JsCompletion) -> Result<JsValue> {
    match completion {
        JsCompletion::Normal(value) => Ok(value),
        JsCompletion::Throw(value) => Err(format!("JavaScript operation threw: {value:?}").into()),
    }
}

struct EpochTicker {
    stop: mpsc::Sender<()>,
    thread: Option<thread::JoinHandle<()>>,
}

impl EpochTicker {
    fn start(engine: &wasmtime::Engine) -> Self {
        let engine = engine.clone();
        let (stop, receiver) = mpsc::channel();
        let thread = thread::spawn(move || {
            while matches!(
                receiver.recv_timeout(EPOCH_PERIOD),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                engine.increment_epoch();
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
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
        "usage: rsc_baseline <rsc.js> <support.json> <output.json> [samples=7] [warmups=3] [native-mode=fast|speed|fast-backtracking|speed-single-pass]",
    )?);
    let support_path = PathBuf::from(args.next().ok_or("missing support.json")?);
    let output = PathBuf::from(args.next().ok_or("missing output.json")?);
    let samples = count(args.next(), 7, "samples")?;
    let warmups = count(args.next(), 3, "warmups")?;
    let native_mode = NativeMode::parse(args.next())?;
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
    configure_compilation_jobs(1)?;
    let compiler = Engine::new(RealmBuilder::new().build());
    let started = Instant::now();
    let unit = compiler.compile_module(&source, CompileOptions::default())?;
    let front_and_lower_ns = started.elapsed().as_nanos();
    let started = Instant::now();
    let artifact = compiler.emit_wasm(&unit)?;
    let wasm_emission_ns = started.elapsed().as_nanos();
    drop(unit);
    drop(compiler);
    let started = Instant::now();
    let mut config = wasmtime::Config::new();
    configure_wasm_runtime(&mut config);
    native_mode.configure(&mut config);
    config.target(env!("SCRIPTW_PERF_TARGET"))?;
    if profile.is_some() {
        config.profiler(wasmtime::ProfilingStrategy::PerfMap);
    }
    let native = wasmtime::Engine::new(&config)?;
    let engine_setup_ns = started.elapsed().as_nanos();
    let configuration = json!({
        "wasmtime":"47.0.0",
        "compilation_jobs":scriptw_engine::configured_compilation_jobs(),
        "cranelift_opt_level":native.get_cranelift_opt_level().map(|value|format!("{value:?}")),
        "native_parallel_compilation":native.get_parallel_compilation(),
        "register_allocation":native.get_cranelift_regalloc_algorithm().map(|value|format!("{value:?}")),
        "target":native.get_target(),
        "target_features_policy":"explicit Config::target disables host-inferred feature flags",
        "collector":native.get_collector().map(|value|format!("{value:?}")),
        "gc_initial_bytes":native.get_gc_heap_initial_size(),
        "max_wasm_stack_bytes":native.get_max_wasm_stack(),
        "wasmtime_async_stack_bytes":native.get_async_stack_size(),
        "driver_worker_stack_bytes":WORKER_STACK_BYTES,
        "store_limiter":"EmbeddedRuntime::new_state limits_mut",
        "epoch_interruption":native.get_epoch_interruption(),
        "epoch_tick_ms":EPOCH_PERIOD.as_millis(),"epoch_deadline_ticks":EPOCH_TICKS,
        "pending_deadline_ms":DEADLINE.as_millis(),
    });
    let native_mode_scope = match native_mode {
        NativeMode::Fast => "matched Fast cohort",
        NativeMode::Speed => {
            "unscored Speed/Backtracking diagnostic; keep separate from Fast cohort"
        }
        NativeMode::FastBacktracking | NativeMode::SpeedSinglePass => {
            "excluded crossed compiler-policy diagnostic; keep separate from scored Fast/Speed cohorts"
        }
    };
    let started = Instant::now();
    let runtime = EmbeddedRuntime::compile(&native, &artifact)?;
    let native_preparation_ns = started.elapsed().as_nanos();
    let _ticker = EpochTicker::start(&native);
    let started = Instant::now();
    let state = runtime.new_state(RealmBuilder::new().build(), ())?;
    let mut store = wasmtime::Store::new(&native, state);
    store.limiter(|state| state.limits_mut());
    store.set_epoch_deadline(EPOCH_TICKS);
    let mut linker = wasmtime::Linker::<WasmStoreState>::new(&native);
    runtime.register(&mut store, &mut linker)?;
    let mut instance = runtime.instantiate(&mut store, &mut linker)?;
    let realm_and_instantiation_ns = started.elapsed().as_nanos();
    let started = Instant::now();
    let evaluation = instance.initialize(&mut store)?;
    initialized(&instance, &mut store, evaluation)?;
    let module_initialization_ns = started.elapsed().as_nanos();
    if !instance.take_output(&mut store)?.is_empty() {
        return Err(
            "bundle initialization emitted output; use rsc.js without capability autorun".into(),
        );
    }
    let run = normal(instance.get_export(&mut store, "run")?)?;
    let encode = normal(instance.get_export(&mut store, "encode")?)?;
    let mut cases = Vec::new();
    for mode in ["sync", "async"] {
        if profile.as_ref().is_some_and(|profile| profile.mode != mode) {
            continue;
        }
        let name = format!("rsc-{mode}");
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
            store.set_epoch_deadline(EPOCH_TICKS);
            if index >= warmups {
                if let Some(profile) = &mut profile {
                    profile.exchange("ready", index)?;
                }
            }
            let started = Instant::now();
            let value = flight(&instance, &mut store, &run, &argument)?;
            let elapsed_ns = started.elapsed().as_nanos();
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
            observations.push(json!({"index":index,"warmup":warmup,"elapsed_ns":elapsed_ns}));
            if !warmup {
                measured.push(elapsed_ns);
            }
        }
        measured.sort_unstable();
        let mut median_ns = measured[measured.len() / 2];
        if measured.len() % 2 == 0 {
            median_ns = (measured[measured.len() / 2 - 1] + median_ns) / 2;
        }
        cases.push(json!({"case":name,"samples":samples,"warmups":warmups,
            "median_ns":median_ns,"observations":observations,
            "exact_node_output_checked":true,"expected_json_sha256":hash(expected.as_bytes()),
            "expected_json_bytes":expected.len()}));
    }
    let runtime_identity = artifact.runtime.as_ref().map(|runtime| json!({
        "key":runtime.key().as_bytes().iter().map(|byte|format!("{byte:02x}")).collect::<String>(),
        "wasm_sha256":hash(runtime.bytes()),"wasm_bytes":runtime.bytes().len(),
    }));
    let mut report = json!({
        "schema":"scriptw-react-retained-baseline-v1","status":"complete",
        "scope":"one process and one retained Realm/module; diagnostic, not a statistical comparison",
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
        "program_wasm_sha256":hash(&artifact.bytes),"program_wasm_bytes":artifact.bytes.len(),
        "runtime":runtime_identity,
        "phases_ns":{"front_and_lower":front_and_lower_ns,"wasm_emission":wasm_emission_ns,
            "source_to_wasm":front_and_lower_ns+wasm_emission_ns,"engine_setup":engine_setup_ns,
            "native_preparation":native_preparation_ns,"realm_and_instantiation":realm_and_instantiation_ns,
            "module_initialization":module_initialization_ns},
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
