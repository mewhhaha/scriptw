//! Experimental consumer-only host using the existing Portable embedding SDK.
//! Build this package separately so workspace feature unification cannot enable
//! native source compilation. Compiler dependency crates still exist; this
//! experiment removes producer commands and the native-source callback roots.
#[path = "../../scriptw-cli/examples/runtime_precompiled/consume.rs"]
#[allow(dead_code)]
mod consume;
#[path = "../../scriptw-cli/examples/runtime_precompiled/phase_barriers.rs"]
mod phase_barriers;
#[path = "../../scriptw-cli/examples/runtime_precompiled/residency.rs"]
mod residency;
#[path = "../../scriptw-cli/examples/compiler_precompiled_baseline/wasi.rs"]
mod wasi;

include!("../../scriptw-cli/examples/runtime_precompiled/shared.rs");

fn run() -> Result<Value, String> {
    if scriptw_engine::NATIVE_SOURCE_COMPILER_ENABLED != cfg!(feature = "native-source-compiler") {
        return Err("consumer feature mismatch: build this package separately to avoid workspace feature unification".into());
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    // Match the shared host's identity-read ordering before memory endpoints.
    let mut report = identity()?;
    match (args.first().map(String::as_str), args.len()) {
        (Some("identity"), 1) => { report["schema"] = json!("scriptw-precompiled-consumer-v1"); report["adapter"] = json!("shared"); },
        (Some("execute-scriptw"), 5) => {
            report = consume::run_with_config(Path::new(&args[1]), &args[2], &args[3], &args[4], configure_consumer)?;
            report["adapter"] = json!("scriptw");
        }
        (Some("execute-wasi"), 5) => {
            report = wasi::execute(Path::new(&args[1]), &args[2], &args[3], &args[4])?;
        }
        _ => return Err("usage: scriptw-precompiled-consumer identity | execute-{scriptw|wasi} TRUSTED_IMAGE EXPECTED_STDOUT SAMPLES WARMUPS".into()),
    }
    report["shared_configuration"] = shared_configuration();
    report["comparison_cohort"] = json!("scriptw-consumer-capability-diagnostic-v1");
    report["excluded_from_scored_cohorts"] = json!(true);
    report["excluded_from_ordinary_cohorts"] = json!(true);
    report["native_source_compiler_enabled"] =
        json!(scriptw_engine::NATIVE_SOURCE_COMPILER_ENABLED);
    report["scope"] = json!("consumer-only entrypoints; compiler dependencies remain; fresh Stores; exact trusted producer images");
    Ok(report)
}

fn main() {
    let result = std::thread::Builder::new()
        .stack_size(WORKER_STACK_BYTES)
        .spawn(run)
        .map_err(|e| e.to_string())
        .and_then(|thread| thread.join().map_err(|_| "consumer panicked".to_owned()))
        .and_then(|result| result);
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
