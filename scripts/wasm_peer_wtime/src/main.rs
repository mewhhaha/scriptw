//! Standalone benchmark peer for Porffor and Scriptw Portable Wasm.

mod porffor;
mod scriptw;

use std::{
    env, fs,
    path::PathBuf,
    process::ExitCode,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use wasmparser::{Import, Parser, Payload, TypeRef};
use wtime_runtime::{
    Config, Engine, ErrorKind, ExecutionPolicy, External, ExternalKind, FeatureSet, Func, Instance,
    Module, ResourceLimits, RunOutcome, Store,
};

type Error = Box<dyn std::error::Error>;

const STORE_MEMORY_BYTES: u64 = 1024 * 1024 * 1024;
const USAGE: &str = "Usage: scriptw-wasm-peer-wtime porffor PROGRAM.wasm EXPECTED_STDOUT SAMPLES WARMUPS [--policy decoded|register]
       scriptw-wasm-peer-wtime scriptw-standalone PROGRAM.wasm EXPECTED_STDOUT SAMPLES WARMUPS [--policy decoded|register]
       scriptw-wasm-peer-wtime scriptw-linked PROGRAM.wasm RUNTIME.wasm EXPECTED_STDOUT SAMPLES WARMUPS [--policy decoded|register]

SAMPLES is 1..=100 and WARMUPS is 0..=1000. The default policy is decoded.
Build with --features register-executor to use the register policy.";

#[derive(Clone, Copy)]
enum Mode {
    Porffor,
    ScriptwStandalone,
    ScriptwLinked,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Porffor => "porffor",
            Self::ScriptwStandalone => "scriptw-standalone",
            Self::ScriptwLinked => "scriptw-linked",
        }
    }

    fn entry(self) -> &'static str {
        match self {
            Self::Porffor => "m",
            Self::ScriptwStandalone | Self::ScriptwLinked => "main",
        }
    }
}

struct Args {
    mode: Mode,
    policy: ExecutionPolicy,
    policy_name: &'static str,
    program: PathBuf,
    runtime: Option<PathBuf>,
    expected: String,
    samples: usize,
    warmups: usize,
}

impl Args {
    fn parse() -> Result<Option<Self>, Error> {
        let mut args = env::args_os().skip(1);
        let mode = match args.next().as_deref().and_then(|arg| arg.to_str()) {
            Some("-h" | "--help") => return Ok(None),
            Some("porffor") => Mode::Porffor,
            Some("scriptw-standalone") => Mode::ScriptwStandalone,
            Some("scriptw-linked") => Mode::ScriptwLinked,
            _ => return Err("expected a benchmark subcommand".into()),
        };
        let program = args.next().ok_or("missing PROGRAM.wasm")?;
        if program == "-h" || program == "--help" {
            return Ok(None);
        }
        let runtime = match mode {
            Mode::ScriptwLinked => Some(args.next().ok_or("missing RUNTIME.wasm")?.into()),
            Mode::Porffor | Mode::ScriptwStandalone => None,
        };
        let expected = args
            .next()
            .ok_or("missing EXPECTED_STDOUT")?
            .into_string()
            .map_err(|_| "EXPECTED_STDOUT must be UTF-8")?;
        let mut count = |name| {
            args.next()
                .as_deref()
                .and_then(|arg| arg.to_str())
                .and_then(|arg| arg.parse::<usize>().ok())
                .ok_or(name)
        };
        let samples = count("expected an integer SAMPLES")?;
        let warmups = count("expected an integer WARMUPS")?;
        if !(1..=100).contains(&samples) || warmups > 1000 {
            return Err("SAMPLES must be 1..=100 and WARMUPS must be 0..=1000".into());
        }
        let (policy, policy_name) = match args.next() {
            None => (ExecutionPolicy::Decoded, "decoded"),
            Some(flag) if flag == "--policy" => {
                match args.next().as_deref().and_then(|arg| arg.to_str()) {
                    Some("decoded") => (ExecutionPolicy::Decoded, "decoded"),
                    Some("register") => (ExecutionPolicy::Register, "register"),
                    _ => return Err("expected --policy decoded or --policy register".into()),
                }
            }
            _ => return Err("expected --policy decoded or --policy register".into()),
        };
        if args.next().is_some() {
            return Err("unexpected trailing arguments".into());
        }
        Ok(Some(Self {
            mode,
            policy,
            policy_name,
            program: program.into(),
            runtime,
            expected,
            samples,
            warmups,
        }))
    }
}

fn imports(bytes: &[u8]) -> Result<Vec<Import<'_>>, Error> {
    for payload in Parser::new(0).parse_all(bytes) {
        match payload? {
            Payload::ImportSection(section) => {
                return Ok(section.into_imports().collect::<Result<_, _>>()?);
            }
            Payload::FunctionSection(_) | Payload::CodeSectionStart { .. } => break,
            _ => {}
        }
    }
    Ok(Vec::new())
}

fn import_names(imports: &[Import<'_>]) -> Vec<String> {
    imports
        .iter()
        .map(|import| format!("{}.{}", import.module, import.name))
        .collect()
}

fn export_func(store: &Store, instance: Instance, name: &str) -> Result<Func, Error> {
    match instance.export(store, name)? {
        External::Func(function) => Ok(function),
        _ => Err(format!("{name} is not a function export").into()),
    }
}

fn invoke(store: &mut Store, entry: Func, mode: Mode) -> (u128, &'static str, String, Option<i32>) {
    let started = Instant::now();
    let outcome = store.invoke(entry, &[]);
    let elapsed = started.elapsed().as_nanos();
    let (completion, error, kind) = match outcome {
        Ok(RunOutcome::Returned(_)) if matches!(mode, Mode::Porffor) => {
            ("normal", String::new(), None)
        }
        Ok(RunOutcome::Returned(results)) => match scriptw::completion_kind(&results) {
            Ok(0) => ("normal", String::new(), Some(0)),
            Ok(1) => ("throw", "Scriptw returned Throw".into(), Some(1)),
            Ok(kind) => (
                "error",
                format!("unexpected Scriptw completion kind {kind}"),
                Some(kind),
            ),
            Err(error) => ("error", error.to_string(), None),
        },
        Ok(RunOutcome::Exception(_)) => ("throw", "uncaught Wasm exception".into(), None),
        Ok(RunOutcome::Suspended { .. }) => ("error", "Wasm execution suspended".into(), None),
        Err(error) => {
            let completion = if error.kind == ErrorKind::Trap {
                "trap"
            } else {
                "error"
            };
            (completion, error.to_string(), None)
        }
    };
    (elapsed, completion, error, kind)
}

enum Peer {
    Porffor(Module),
    Scriptw {
        runtime: Option<Module>,
        memory_imports: scriptw::MemoryImports,
    },
}

struct Prepared {
    engine: Engine,
    program: Module,
    peer: Peer,
}

fn prepare(args: &Args, report: &mut Json) -> Result<Prepared, Error> {
    report["status"] = json!("input-error");
    let bytes = fs::read(&args.program)
        .map_err(|error| format!("reading {}: {error}", args.program.display()))?;
    report["wasm_bytes"] = json!(bytes.len());
    report["wasm_sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
    let runtime_bytes = if let Some(path) = &args.runtime {
        let bytes =
            fs::read(path).map_err(|error| format!("reading {}: {error}", path.display()))?;
        report["runtime_wasm_bytes"] = json!(bytes.len());
        report["runtime_wasm_sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
        Some(bytes)
    } else {
        None
    };

    let config = Config {
        features: FeatureSet::all_phase3_core(),
        limits: ResourceLimits {
            max_memory_bytes: STORE_MEMORY_BYTES,
            max_store_memory_bytes: STORE_MEMORY_BYTES,
            compile_threads: 1,
            ..ResourceLimits::default()
        },
    };
    report["status"] = json!("engine-rejected");
    let started = Instant::now();
    let engine = Engine::with_execution_policy(config, args.policy);
    report["engine_ns"] = json!(started.elapsed().as_nanos());
    let engine = engine?;
    report["status"] = json!("module-rejected");
    let started = Instant::now();
    let program = engine.compile(&bytes);
    report["module_compilation_ns"] = json!(started.elapsed().as_nanos());
    let program = program.map_err(|error| format!("program module: {error}"))?;
    let program_imports = imports(&bytes)?;
    report["imports"] = json!(import_names(&program_imports));
    let (runtime, runtime_imports) = if let Some(bytes) = &runtime_bytes {
        let started = Instant::now();
        let module = engine.compile(bytes);
        report["runtime_compilation_ns"] = json!(started.elapsed().as_nanos());
        let module = module.map_err(|error| format!("runtime module: {error}"))?;
        let runtime_imports = imports(bytes)?;
        report["runtime_imports"] = json!(import_names(&runtime_imports));
        (Some(module), runtime_imports)
    } else {
        (None, Vec::new())
    };

    report["status"] = json!("unsupported-interface");
    let entry_name = args.mode.entry();
    if !program
        .exports()
        .iter()
        .any(|(name, kind, _)| *name == entry_name && *kind == ExternalKind::Func)
    {
        return Err(format!("missing function export {entry_name}").into());
    }
    let peer = match args.mode {
        Mode::Porffor => {
            for import in &program_imports {
                if !import.module.is_empty()
                    || !matches!(import.name, "a" | "b" | "c" | "d")
                    || !matches!(import.ty, TypeRef::Func(_) | TypeRef::FuncExact(_))
                {
                    return Err(format!(
                        "unsupported Porffor import {}.{}",
                        import.module, import.name
                    )
                    .into());
                }
            }
            report["status"] = json!("adapter-rejected");
            Peer::Porffor(porffor::compile_adapter(&engine)?)
        }
        Mode::ScriptwStandalone | Mode::ScriptwLinked => Peer::Scriptw {
            runtime,
            memory_imports: scriptw::MemoryImports::inspect(&program_imports, &runtime_imports)?,
        },
    };
    Ok(Prepared {
        engine,
        program,
        peer,
    })
}

enum Capture {
    Porffor(porffor::Iteration),
    Scriptw(Arc<Mutex<Vec<u8>>>),
}

fn run(args: &Args, report: &mut Json, observations: &mut Vec<Json>) -> Result<(), Error> {
    let Prepared {
        engine,
        program,
        peer,
    } = prepare(args, report)?;
    let entry_name = args.mode.entry();
    let clock_origin = Instant::now();
    let origin_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64() * 1000.0;
    for index in 0..args.warmups + args.samples {
        report["status"] = json!("instantiation-failed");
        let mut store = Store::new(&engine);
        let started = Instant::now();
        let (instance, output) = match &peer {
            Peer::Porffor(adapter) => {
                let iteration = porffor::instantiate(
                    &engine,
                    &mut store,
                    &program,
                    adapter,
                    clock_origin,
                    origin_ms,
                )?;
                (iteration.program, Capture::Porffor(iteration))
            }
            Peer::Scriptw {
                runtime,
                memory_imports,
            } => {
                let output = Arc::new(Mutex::new(Vec::new()));
                let instance = scriptw::instantiate(
                    &engine,
                    &mut store,
                    &program,
                    runtime.as_ref(),
                    memory_imports,
                    Arc::clone(&output),
                )?;
                (instance, Capture::Scriptw(output))
            }
        };
        let entry = export_func(&store, instance, entry_name)?;
        let instantiation_ns = started.elapsed().as_nanos();
        let empty_call_ns = if let Capture::Porffor(iteration) = &output {
            report["status"] = json!("adapter-call-failed");
            let empty = export_func(&store, iteration.adapter, "empty")?;
            let started = Instant::now();
            let outcome = store.invoke(empty, &[]);
            let elapsed = started.elapsed().as_nanos();
            if !matches!(outcome?, RunOutcome::Returned(values) if values.is_empty()) {
                return Err("the empty adapter call did not return normally".into());
            }
            Some(elapsed)
        } else {
            None
        };

        let (execution_ns, mut completion, mut error, kind) = invoke(&mut store, entry, args.mode);
        let mut observation = json!({
            "index": index,
            "warmup": index < args.warmups,
            "execution_ns": execution_ns,
            "instantiation_ns": instantiation_ns,
        });
        let captured = match output {
            Capture::Porffor(iteration) => {
                observation["empty_call_ns"] = json!(empty_call_ns);
                iteration.read_output(&store).map(|captured| {
                    observation["numeric_print_calls"] = json!(captured.numeric_print_calls);
                    observation["clock_calls"] = json!(captured.clock_calls);
                    observation["mixed_output"] = json!(captured.mixed_output);
                    (captured.stdout, captured.mixed_output)
                })
            }
            Capture::Scriptw(output) => {
                observation["completion_kind"] = json!(kind);
                let bytes =
                    std::mem::take(&mut *output.lock().map_err(|_| "stdout lock poisoned")?);
                String::from_utf8(bytes)
                    .map(|stdout| (stdout, false))
                    .map_err(Error::from)
            }
        };
        let (stdout, mixed) = match captured {
            Ok(output) => output,
            Err(output_error) => {
                if completion == "normal" {
                    completion = "error";
                    error = output_error.to_string();
                }
                (String::new(), false)
            }
        };
        observation["matched"] = json!(
            completion == "normal" && !mixed && stdout.as_bytes() == args.expected.as_bytes()
        );
        observation["stdout"] = json!(stdout);
        observation["completion"] = json!(completion);
        observation["error"] = json!(error);
        observations.push(observation);
        if completion != "normal" {
            report["status"] = json!("interrupted");
            return Err(format!("observation {index} ended with {completion}: {error}").into());
        }
    }
    report["status"] = json!("complete");
    Ok(())
}

fn main() -> ExitCode {
    let args = match Args::parse() {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("{error}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut report = json!({
        "schema": "scriptw-wasm-peer-wtime-v1",
        "engine": "wtime-runtime",
        "mode": args.mode.name(),
        "policy": args.policy_name,
        "samples": args.samples,
        "warmups": args.warmups,
        "configuration": {
            "features": "all_phase3_core",
            "parallel_compilation": false,
            "wasm_exceptions": true,
            "wasm_gc": true,
            "allocation": "fresh Store per iteration",
            "store_memory_limit_bytes": STORE_MEMORY_BYTES,
        }
    });
    let mut observations = Vec::new();
    let failed = match run(&args, &mut report, &mut observations) {
        Ok(()) => false,
        Err(error) => {
            report["error"] = json!(error.to_string());
            if report.get("status").is_none() {
                report["status"] = json!("error");
            }
            true
        }
    };
    report["observations"] = Json::Array(observations);
    println!("{report}");
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
