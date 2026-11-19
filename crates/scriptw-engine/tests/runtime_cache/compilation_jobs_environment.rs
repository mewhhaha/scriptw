//! The process-wide compilation-pool default honours `SCRIPTW_JOBS`.
//!
//! Integration-test binaries link the engine without `cfg(test)`, so only the
//! environment keeps an unconfigured first compilation from choosing a
//! CPU-derived pool that later one-job requests refuse. The pool is chosen once
//! per process, so each case runs in a fresh child of this test binary.

use std::process::Command;

const EXPECTED: &str = "SCRIPTW_COMPILATION_JOBS_PROCESS_TEST_EXPECTED";
const TEST: &str = "unconfigured_compilation_pool_honours_scriptw_jobs";

fn libtest_path(test: &str) -> String {
    match module_path!().split_once("::") {
        Some((_, module)) => format!("{module}::{test}"),
        None => test.to_owned(),
    }
}

fn cpu_default() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(2)
        .div_ceil(2)
        .max(1)
}

#[test]
fn unconfigured_compilation_pool_honours_scriptw_jobs() {
    if let Ok(expected) = std::env::var(EXPECTED) {
        let expected: usize = expected.parse().unwrap();
        // Nothing configured the pool yet: the first reader chooses it.
        assert_eq!(scriptw_engine::configured_compilation_jobs(), None);
        assert_eq!(scriptw_engine::default_compilation_jobs(), expected);
        assert_eq!(scriptw_engine::compilation_jobs(), expected);
        assert_eq!(scriptw_engine::configure_compilation_jobs(expected), Ok(()));
        assert!(scriptw_engine::configure_compilation_jobs(expected + 1).is_err());
        return;
    }

    let executable = std::env::current_exe().unwrap();
    for (scriptw_jobs, expected) in [
        (Some("1"), 1),
        (Some("3"), 3),
        (Some("not-a-count"), cpu_default()),
        (Some("0"), cpu_default()),
        (None, cpu_default()),
    ] {
        let mut command = Command::new(&executable);
        command
            .args(["--exact", &libtest_path(TEST), "--nocapture", "--test-threads=1"])
            .env(EXPECTED, expected.to_string());
        match scriptw_jobs {
            Some(value) => command.env("SCRIPTW_JOBS", value),
            None => command.env_remove("SCRIPTW_JOBS"),
        };
        let output = command.output().expect("fresh test process starts");
        assert!(
            output.status.success(),
            "SCRIPTW_JOBS={scriptw_jobs:?}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        // An empty filter match would also exit successfully.
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "child must run exactly the selected test: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
