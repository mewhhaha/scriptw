//! Independent fresh-process compiler-disabled execution diagnostic.
#[path = "runtime_precompiled/consume.rs"]
mod consume;
#[path = "runtime_precompiled/phase_barriers.rs"]
mod phase_barriers;
#[path = "runtime_precompiled/produce.rs"]
mod produce;
#[path = "runtime_precompiled/residency.rs"]
mod residency;

include!("runtime_precompiled/shared.rs");

fn run() -> Result<Value, String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match (args.first().map(String::as_str), args.len()) {
        (Some("prepare"), 3) => produce::run(Path::new(&args[1]), Path::new(&args[2])),
        (Some("execute"), 5) => consume::run(Path::new(&args[1]), &args[2], &args[3], &args[4]),
        _ => Err("usage: runtime_precompiled prepare FIXTURES NEW_DIRECTORY | execute IMAGE EXPECTED_STDOUT SAMPLES WARMUPS".into()),
    }
}

fn main() {
    match run() {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
