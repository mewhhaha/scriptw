//! Benchmark-only WASI host for externally compiled programs; never a product path.
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, time::Instant};
use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::{p1, p1::WasiP1Ctx, p2::pipe::MemoryOutputPipe, I32Exit, WasiCtxBuilder};

fn rss() -> Result<u64, String> {
    let status = fs::read_to_string("/proc/self/status").map_err(|e| e.to_string())?;
    let line = status
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .ok_or("VmRSS is unavailable")?;
    let kib: u64 = line
        .split_whitespace()
        .nth(1)
        .ok_or("missing RSS")?
        .parse()
        .map_err(|_| "invalid RSS")?;
    Ok(kib * 1024)
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: wasi_baseline MODULE.wasm EXPECTED_STDOUT SAMPLES WARMUPS".into());
    }
    let samples: usize = args[2].parse().map_err(|_| "invalid samples")?;
    let warmups: usize = args[3].parse().map_err(|_| "invalid warmups")?;
    if !(3..=100).contains(&samples) || !(1..=100).contains(&warmups) {
        return Err("samples must be 3..100 and warmups 1..100".into());
    }
    let bytes = fs::read(&args[0]).map_err(|e| e.to_string())?;
    let expected = fs::read(&args[1]).map_err(|e| e.to_string())?;
    let mut config = Config::new();
    scriptw_engine::configure_wasm_runtime(&mut config);
    let engine = Engine::new(&config).map_err(|e| e.to_string())?;
    let begin = Instant::now();
    let module = Module::new(&engine, &bytes).map_err(|e| e.to_string())?;
    let compilation_ns = begin.elapsed().as_nanos();
    let mut observations = Vec::new();
    for index in 0..warmups + samples {
        let begin = Instant::now();
        let stdout = MemoryOutputPipe::new(1024 * 1024);
        let stderr = MemoryOutputPipe::new(1024 * 1024);
        let wasi = WasiCtxBuilder::new()
            .arg("benchmark")
            .stdout(stdout.clone())
            .stderr(stderr.clone())
            .build_p1();
        let mut store = Store::new(&engine, wasi);
        // The parent enforces a wall-clock timeout. This host has no epoch ticker.
        store.set_epoch_deadline(u64::MAX);
        let mut linker: Linker<WasiP1Ctx> = Linker::new(&engine);
        p1::add_to_linker_sync(&mut linker, |context| context).map_err(|e| e.to_string())?;
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| e.to_string())?;
        let start = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .map_err(|e| e.to_string())?;
        let setup_ns = begin.elapsed().as_nanos();
        let begin = Instant::now();
        let result = start.call(&mut store, ());
        let output = stdout.contents();
        let errors = stderr.contents();
        let execution_ns = begin.elapsed().as_nanos();
        let rss_after_bytes = rss()?;
        let completion = match result {
            Ok(()) => "normal".to_owned(),
            Err(error)
                if error
                    .downcast_ref::<I32Exit>()
                    .is_some_and(|exit| exit.0 == 0) =>
            {
                "normal".to_owned()
            }
            Err(error) => format!("{error:#}"),
        };
        let matched = completion == "normal" && output.as_ref() == expected && errors.is_empty();
        observations.push(json!({"warmup": index < warmups,
            "execution_ns": execution_ns, "setup_ns": setup_ns,
            "rss_after_bytes": rss_after_bytes, "completion": completion,
            "stdout": String::from_utf8_lossy(&output),
            "stderr": String::from_utf8_lossy(&errors), "matched": matched}));
        if !matched {
            break;
        }
    }
    println!(
        "{}",
        json!({"schema": "scriptw-wasi-baseline-v1",
        "wasmtime_version": "47.0.0", "wasm_sha256": format!("{:x}", Sha256::digest(&bytes)),
        "compilation_ns": compilation_ns, "samples": samples, "warmups": warmups,
        "observations": observations})
    );
    Ok(())
}

fn main() {
    let result = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(run)
        .map_err(|e| e.to_string())
        .and_then(|thread| thread.join().map_err(|_| "WASI host panicked".to_owned()))
        .and_then(|result| result);
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
