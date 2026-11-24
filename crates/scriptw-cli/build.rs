//! Capture the toolchain which actually builds this CLI image, and link the
//! product executables without position independence.
use std::process::Command;
fn main() {
    // `scriptw` and the example hosts (compiler_precompiled_baseline and the
    // other benchmark hosts) link non-PIE (ET_EXEC) on Linux. As a PIE, the
    // CLI's ~57,700 R_X86_64_RELATIVE relocations dirty its whole RELRO
    // segment (.data.rel.ro and .got, about 0.93 MiB) in every process; a
    // fixed link resolves them once, so those pages stay clean and shared.
    // The cost is executable ASLR: CONTRIBUTING (Dist compiler binary)
    // records the trade-off. Code stays PIC, which links into a fixed image
    // unchanged; the runtime image's origin proof admits ET_EXEC only at load
    // bias 0. A link argument, not a rustflag, so it reaches release, dist,
    // LTO and PGO links and leaves every dependency's fingerprint alone.
    // Integration and unit test binaries, and scriptw-precompiled-consumer,
    // which shares this script, stay PIE.
    let linux = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "linux");
    if linux && std::env::var("CARGO_PKG_NAME").is_ok_and(|name| name == "scriptw-cli") {
        println!("cargo:rustc-link-arg-bins=-no-pie");
        println!("cargo:rustc-link-arg-examples=-no-pie");
    }
    for variable in ["RUSTC", "CARGO_ENCODED_RUSTFLAGS"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    let version = Command::new(std::env::var_os("RUSTC").expect("Cargo supplies rustc"))
        .args(["--version", "--verbose"])
        .output()
        .expect("compiling rustc is readable");
    assert!(version.status.success(), "rustc version query failed");
    let version = String::from_utf8(version.stdout).expect("rustc version is UTF-8");
    println!(
        "cargo:rustc-env=SCRIPTW_PERF_RUSTC={}",
        version
            .chars()
            .flat_map(char::escape_default)
            .collect::<String>()
    );
    for variable in ["TARGET", "HOST", "PROFILE", "OPT_LEVEL", "DEBUG"] {
        println!(
            "cargo:rustc-env=SCRIPTW_PERF_{variable}={}",
            std::env::var(variable).expect("Cargo build metadata is present")
        );
    }
    println!(
        "cargo:rustc-env=SCRIPTW_PERF_RUSTFLAGS={}",
        std::env::var("CARGO_ENCODED_RUSTFLAGS")
            .unwrap_or_default()
            .chars()
            .flat_map(char::escape_default)
            .collect::<String>()
    );
}
