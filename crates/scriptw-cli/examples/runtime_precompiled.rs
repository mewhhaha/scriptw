//! Independent fresh-process compiler-disabled execution diagnostic.
#[path = "runtime_precompiled/consume.rs"]
mod consume;
#[path = "runtime_precompiled/phase_barriers.rs"]
mod phase_barriers;
#[path = "runtime_precompiled/produce.rs"]
mod produce;
#[path = "runtime_precompiled/residency.rs"]
mod residency;

use scriptw_engine::{CompilerIdentity, CompilerSourceRevision};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn identity() -> Result<Value, String> {
    let identity = CompilerIdentity::current()?;
    let revision = match identity.source_revision() {
        CompilerSourceRevision::GitCommit(commit) => Some(commit.as_str()),
        CompilerSourceRevision::UnversionedArchive => None,
    };
    Ok(json!({"source_commit": revision,
        "source_fingerprint": identity.source_fingerprint().to_string(),
        "executable_sha256": identity.executable_sha256().to_string(),
        "compiler_identity_scheme": CompilerIdentity::source_fingerprint_scheme()}))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|error| error.to_string())
}

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
