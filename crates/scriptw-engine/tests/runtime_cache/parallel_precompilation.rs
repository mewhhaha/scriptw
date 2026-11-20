//! Parallel native compilation produces the same precompiled image as serial
//! compilation. The job count is process-wide, so each count compiles in its own
//! fresh child of this test binary and the parent compares the images.

use scriptw_engine::{
    configure_compilation_jobs, configure_wasm_runtime, CompileOptions, EmbeddedRuntime, Engine,
    NativeCompilationPolicy, RealmBuilder,
};
use sha2::{Digest, Sha256};
use std::process::Command;

const JOBS: &str = "SCRIPTW_PARALLEL_PRECOMPILATION_TEST_JOBS";
const TEST: &str = "serial_and_parallel_compilation_produce_the_same_precompiled_image";
const IMAGE: &str = "precompiled image:";

// Several functions of different sizes, so two jobs really split the module.
const SOURCE: &str = r#"
function fib(n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
function words(text) { return text.split(/\s+/).filter(Boolean).map((w) => w.toUpperCase()); }
class Counter {
  #count = 0;
  add(step) { this.#count += step; return this; }
  get value() { return this.#count; }
}
export function run(limit) {
  const counter = new Counter();
  for (let i = 0; i < limit; i++) counter.add(fib(i % 12));
  return `${counter.value}:${words("serial and parallel images").join("-")}`;
}
export const answer = run(20);
"#;

fn libtest_path(test: &str) -> String {
    match module_path!().split_once("::") {
        Some((_, module)) => format!("{module}::{test}"),
        None => test.to_owned(),
    }
}

fn precompile_in_this_process(jobs: usize) {
    let jobs = configure_compilation_jobs(jobs).expect("a fresh process chooses its job count");
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(SOURCE, CompileOptions::default())
        .expect("fixture compiles");
    let artifact = compiler
        .emit_wasm(&unit)
        .expect("fixture emits Portable P/R");
    let mut config = wasmtime::Config::new();
    configure_wasm_runtime(&mut config, NativeCompilationPolicy::Speed, jobs);
    let engine = wasmtime::Engine::new(&config).unwrap();
    assert_eq!(engine.get_parallel_compilation(), jobs.get().get() > 1);
    let image = EmbeddedRuntime::precompile(&engine, &artifact).expect("precompiles");

    // The flag must follow the process count in both directions.
    let mut disagreeing = wasmtime::Config::new();
    configure_wasm_runtime(&mut disagreeing, NativeCompilationPolicy::Speed, jobs);
    disagreeing.parallel_compilation(jobs.get().get() == 1);
    let disagreeing = wasmtime::Engine::new(&disagreeing).unwrap();
    let error = EmbeddedRuntime::precompile(&disagreeing, &artifact)
        .err()
        .expect("an Engine whose parallel flag disagrees is refused")
        .to_string();
    assert!(error.contains(&format!("{} job(s)", jobs.get())), "{error}");

    println!(
        "{IMAGE} {} {:x}",
        image.as_bytes().len(),
        Sha256::digest(image.as_bytes())
    );
}

#[test]
fn serial_and_parallel_compilation_produce_the_same_precompiled_image() {
    if let Ok(jobs) = std::env::var(JOBS) {
        precompile_in_this_process(jobs.parse().unwrap());
        return;
    }
    let executable = std::env::current_exe().unwrap();
    let image = |jobs: usize| {
        let output = Command::new(&executable)
            .args([
                "--exact",
                &libtest_path(TEST),
                "--nocapture",
                "--test-threads=1",
            ])
            .env(JOBS, jobs.to_string())
            .output()
            .expect("fresh test process starts");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "{jobs} job(s):\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        stdout
            .lines()
            // libtest may print the test name on the same line.
            .find_map(|line| line.split_once(IMAGE))
            .map(|(_, image)| image.trim().to_owned())
            .expect("the child reports its image")
    };
    assert_eq!(image(1), image(2));
}
