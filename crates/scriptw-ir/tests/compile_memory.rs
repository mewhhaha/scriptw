//! Memory bounds for source shapes whose compilation once grew by hundreds of
//! bytes per empty statement or by megabytes per method.
//!
//! Test262's `staging/sm/regress/regress-610026.js` evaluates about four
//! million `{}` blocks, and `staging/sm/class/methDefn.js` an object literal
//! of 1,000 methods. Each peaked at about 2.2 GB. The blocks were 464-byte
//! `Statement`s (the inline for-in/of variants doubled every statement). Every
//! method's signature held a deep copy of the global object's shape as its
//! `this` (about 1 MB with the builtin globals), and recording a return copied
//! the signature again; heap shapes are now shared.
//!
//! The lowering bound reads the whole process's peak resident set, so the
//! tests take a lock; this binary holds nothing else.

use std::sync::Mutex;

use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;

static MEASUREMENT: Mutex<()> = Mutex::new(());

#[cfg(target_os = "linux")]
fn status_kib(field: &str) -> usize {
    let status = std::fs::read_to_string("/proc/self/status").expect("procfs status");
    let line = status
        .lines()
        .find(|line| line.starts_with(field))
        .unwrap_or_else(|| panic!("{field} missing from /proc/self/status"));
    line[field.len()..]
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .expect("status value in kB")
}

/// Runs `work` and returns its result with the growth of the process's peak
/// resident set above the resident set at its start. Writing 5 to
/// `clear_refs` resets the peak (`VmHWM`) to the current resident set.
#[cfg(target_os = "linux")]
fn peak_resident_growth<T>(work: impl FnOnce() -> T) -> (T, usize) {
    std::fs::write("/proc/self/clear_refs", "5").expect("reset the peak resident set");
    let start = status_kib("VmRSS:");
    let result = work();
    let peak = status_kib("VmHWM:");
    (result, peak.saturating_sub(start) * 1024)
}

#[test]
#[cfg(target_pointer_width = "64")]
fn an_empty_block_statement_is_a_224_byte_node() {
    let _serial = MEASUREMENT
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    // The parser boxes every statement of a list, so `{}` costs a whole
    // `Statement`: 464 bytes with the for-in/of loops inline.
    assert!(std::mem::size_of::<boa_ast::statement::Statement>() <= 224);
    const BLOCKS: usize = 1 << 12;
    let parsed = parse("{}".repeat(BLOCKS), ParseOptions::script()).expect("empty blocks parse");
    let statements = parsed
        .as_script()
        .expect("parsed as a Script")
        .with_compiler_session(|script, _| script.statements().statements().len());
    assert_eq!(statements, BLOCKS);
}

#[test]
#[cfg(target_os = "linux")]
fn methods_share_the_global_object_shape_rather_than_copying_it() {
    let _serial = MEASUREMENT
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    // Builds the lazily initialized builtin catalogues outside the measurement.
    let warm = lower(
        &parse(
            "var w = {m() { return 1; }}; w.m();",
            ParseOptions::script(),
        )
        .unwrap(),
    );
    assert!(warm.is_wasm_supported(), "{:?}", warm.diagnostics);
    const METHODS: usize = 200;
    let mut source = String::from("var o = {");
    for index in 0..METHODS {
        source.push_str(&format!("['m' + {index}]() {{ return 'ok'; }}, "));
    }
    source.push_str("};\nfor (var i = 0; i < 2; i++) o['m' + i]();\n");
    let parsed = parse(source, ParseOptions::script()).unwrap();
    let (program, peak) = peak_resident_growth(|| lower(&parsed));
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    // Each method copied the builtin shapes: 208 MiB here, against 8 MiB shared.
    assert!(
        peak <= 64 << 20,
        "lowering {METHODS} methods peaked {peak} bytes above the start"
    );
}
