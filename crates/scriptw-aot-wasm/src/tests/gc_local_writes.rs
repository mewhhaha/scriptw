//! Every holder of a pooled typed GC local writes it before reading it.
//!
//! The code sink checks each finished body in test and debug builds
//! (`code_sink/gc_local_writes.rs`) and panics at a read some path reaches
//! without a write. This runs that check over R and over the engine's
//! JavaScript fixtures, so a new emitter path is checked even when no other
//! emitter test reaches it.
use super::*;

fn scripts(directory: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
    let mut entries = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            scripts(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "js") {
            found.push(path);
        }
    }
}

#[test]
fn r_and_the_engine_fixtures_write_each_typed_gc_local_before_reading_it() {
    let runtime = emit_script("print([1].map(x => x));")
        .expect("a heap program emits")
        .runtime()
        .expect("a heap program links R")
        .clone();
    assert!(!runtime.bytes().is_empty());
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scriptw-engine/tests/fixtures");
    let mut found = Vec::new();
    scripts(&fixtures, &mut found);
    let mut emitted = 0;
    for path in &found {
        let source = std::fs::read_to_string(path).unwrap();
        // Module-goal files do not parse as Scripts; every emitted body,
        // whatever the outcome of the rest of the program, was checked.
        let Ok(parsed) = parse(&source, ParseOptions::script()) else {
            continue;
        };
        if emit(&lower_with_host_surface_policy(
            &parsed,
            HostSurfacePolicy::Test262,
        ))
        .is_ok()
        {
            emitted += 1;
        }
    }
    assert!(
        emitted >= found.len() / 2,
        "only {emitted} of {} fixture scripts emitted",
        found.len()
    );
}
