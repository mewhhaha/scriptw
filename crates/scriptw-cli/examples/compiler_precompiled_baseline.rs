//! Shared benchmark-only host. External native images require trusted producers.
#[path = "runtime_precompiled/consume.rs"]
#[allow(dead_code)]
mod consume;
#[path = "runtime_precompiled/phase_barriers.rs"]
mod phase_barriers;
#[path = "runtime_precompiled/produce.rs"]
#[allow(dead_code)]
mod produce;
#[path = "runtime_precompiled/residency.rs"]
mod residency;
#[path = "compiler_precompiled_baseline/wasi.rs"]
mod wasi;

include!("runtime_precompiled/shared.rs");
use scriptw_engine::PrecompiledEmbeddedProgram;
#[path = "compiler_precompiled_baseline/wasi/produce.rs"]
mod wasi_produce;

fn prepare_scriptw(fixtures: &Path, output: &Path) -> Result<Value, String> {
    let mut report = produce::run_with_config(fixtures, output, configure_producer)?;
    for case in report["cases"]
        .as_array_mut()
        .ok_or("missing producer cases")?
    {
        let name = case["file"].as_str().ok_or("missing producer file")?;
        let wasm_name = format!("{name}.wasm");
        let image_name = case["image"].as_str().ok_or("missing producer image")?;
        let bytes = fs::read(output.join(image_name)).map_err(|e| e.to_string())?;
        // SAFETY: these are the exact bytes just written above by this process's
        // EmbeddedRuntime::precompile, not an external or digest-based authority.
        let image =
            unsafe { PrecompiledEmbeddedProgram::from_bytes(&bytes) }.map_err(|e| e.to_string())?;
        write_new(&output.join(&wasm_name), image.program_wasm())?;
        case["wasm"] = json!(wasm_name);
    }
    report["compiler_enabled"] = json!(true); // precompile required a real compiler.
    let report = decorate(report, "scriptw");
    // Preserve the delegated producer's original receipt, then publish the
    // augmented receipt in this newly created, exclusively owned directory.
    fs::rename(
        output.join("producer.json"),
        output.join("scriptw-producer.json"),
    )
    .map_err(|e| e.to_string())?;
    write_new(&output.join("producer.json"), report.to_string().as_bytes())?;
    Ok(report)
}

fn execute_scriptw(args: &[String]) -> Result<Value, String> {
    let mut report = consume::run_with_config(
        Path::new(&args[1]),
        &args[2],
        &args[3],
        &args[4],
        configure_consumer,
    )?;
    for row in report["observations"]
        .as_array_mut()
        .ok_or("missing observations")?
    {
        // The delegated path has already required normal completion and exact
        // output. It rejects reported exceptions rather than returning rows.
        row["stderr"] = json!("");
        row["matched"] = json!(true);
    }
    Ok(decorate(report, "scriptw"))
}

fn run() -> Result<Value, String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    // Bind/read the executable identically before either adapter's measured
    // load endpoints. Subsequent identity() calls use its immutable cached owner.
    let mut identity = identity()?;
    match (args.first().map(String::as_str), args.len()) {
        (Some("identity"), 1) => {
            identity["schema"] = json!("scriptw-precompiled-shared-host-v1");
            Ok(decorate(identity, "shared"))
        }
        (Some("prepare-scriptw"), 3) => prepare_scriptw(Path::new(&args[1]), Path::new(&args[2])),
        (Some("prepare-wasi"), 3) => wasi_produce::prepare(Path::new(&args[1]), Path::new(&args[2])),
        (Some("execute-scriptw"), 5) => execute_scriptw(&args),
        (Some("execute-wasi"), 5) => wasi::execute(Path::new(&args[1]), &args[2], &args[3], &args[4]),
        _ => Err("usage: compiler_precompiled_baseline identity | prepare-scriptw FIXTURES NEW_DIRECTORY | prepare-wasi INPUT.wasm NEW_IMAGE | execute-{scriptw|wasi} IMAGE EXPECTED_STDOUT SAMPLES WARMUPS".into()),
    }
}

fn main() {
    let result = std::thread::Builder::new()
        .stack_size(WORKER_STACK_BYTES)
        .spawn(run)
        .map_err(|e| e.to_string())
        .and_then(|thread| {
            thread
                .join()
                .map_err(|_| "shared precompiled host panicked".to_owned())
        })
        .and_then(|result| result);
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
