//! Memory bounds for source shapes whose compilation once grew by hundreds of
//! bytes per empty statement or by megabytes per method.
//!
//! Test262's `staging/sm/regress/regress-610026.js` evaluates about four
//! million `{}` blocks, and `staging/sm/class/methDefn.js` an object literal
//! of 1,000 methods. Each peaked at about 2.2 GB. The blocks were 464-byte
//! `Statement`s (the inline for-in/of variants doubled every statement). Every
//! method's signature held a deep copy of the global object's shape as its
//! `this` (about 1 MB with the builtin globals), and recording a return copied
//! the signature again; heap shapes are now shared. A `delete`, write or call
//! that invalidates shape facts decides from the shared shapes and copies
//! only those it changes.
//!
//! The lowering bounds read the whole process's peak resident set, so they
//! take a lock; this binary holds nothing else. They skip, saying why, where
//! the kernel cannot reset or report that peak.

use std::sync::Mutex;

use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;

static MEASUREMENT: Mutex<()> = Mutex::new(());

#[cfg(target_os = "linux")]
fn status_kib(field: &str) -> Result<usize, String> {
    let status = std::fs::read_to_string("/proc/self/status")
        .map_err(|error| format!("/proc/self/status is unreadable: {error}"))?;
    let line = status
        .lines()
        .find(|line| line.starts_with(field))
        .ok_or_else(|| format!("/proc/self/status has no {field} line"))?;
    line[field.len()..]
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .map_err(|error| format!("/proc/self/status {field} is not a kB count: {error}"))
}

/// Runs `work` and returns its result with the growth of the process's peak
/// resident set above the resident set at its start, or `None`, without
/// running `work`, where the peak cannot be measured. Writing 5 to
/// `clear_refs` resets the peak (`VmHWM`) to the current resident set; the
/// file exists only with `CONFIG_PROC_PAGE_MONITOR`, and gVisor and WSL1
/// emulate procfs without it.
#[cfg(target_os = "linux")]
fn peak_resident_growth<T>(work: impl FnOnce() -> T) -> Option<(T, usize)> {
    let start = std::fs::write("/proc/self/clear_refs", "5")
        .map_err(|error| format!("cannot reset the peak through /proc/self/clear_refs: {error}"))
        .and_then(|()| Ok((status_kib("VmHWM:")?, status_kib("VmRSS:")?)))
        .and_then(|(reset_peak, start)| {
            // The resident set only grows between the two reads, by far less
            // than a MiB, unless the reset was ignored.
            if reset_peak > start + 1024 {
                Err(format!(
                    "writing /proc/self/clear_refs left VmHWM at {reset_peak} kB above \
                     VmRSS {start} kB"
                ))
            } else {
                Ok(start)
            }
        });
    let start = match start {
        Ok(start) => start,
        Err(reason) => {
            eprintln!("skipped: this kernel cannot measure the peak resident set: {reason}");
            return None;
        }
    };
    let result = work();
    let peak = status_kib("VmHWM:").expect("VmHWM was readable before the work");
    Some((result, peak.saturating_sub(start) * 1024))
}

#[test]
#[cfg(target_pointer_width = "64")]
fn an_empty_block_statement_is_a_224_byte_node() {
    // The parser boxes every statement of a list, so `{}` costs a whole
    // `Statement`: 464 bytes with the for-in/of loops inline.
    assert!(std::mem::size_of::<boa_ast::statement::Statement>() <= 224);
}

const METHODS: usize = 200;

/// Lowers an object literal of [`METHODS`] methods followed by `tail` and
/// returns how far lowering raised the peak resident set, or `None` where
/// that cannot be measured.
#[cfg(target_os = "linux")]
fn methods_then(tail: &str) -> Option<usize> {
    let _serial = MEASUREMENT
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    // Builds the lazily initialized builtin catalogues outside the measurement.
    let warm = lower(
        &parse(
            "var w = {m() { return 1; }}; w.m(); w.x = 1; delete w.x;",
            ParseOptions::script(),
        )
        .unwrap(),
    );
    assert!(warm.is_wasm_supported(), "{:?}", warm.diagnostics);
    let mut source = String::from("var o = {");
    for index in 0..METHODS {
        source.push_str(&format!("['m' + {index}]() {{ return 'ok'; }}, "));
    }
    source.push_str("};\n");
    source.push_str(tail);
    let parsed = parse(source, ParseOptions::script()).unwrap();
    let (program, peak) = peak_resident_growth(|| lower(&parsed))?;
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    Some(peak)
}

#[test]
#[cfg(target_os = "linux")]
fn methods_share_the_global_object_shape_rather_than_copying_it() {
    let Some(peak) = methods_then("for (var i = 0; i < 2; i++) o['m' + i]();\n") else {
        return;
    };
    // Each method copied the builtin shapes: 208 MiB here, against 8 MiB shared.
    assert!(
        peak <= 64 << 20,
        "lowering {METHODS} methods peaked {peak} bytes above the start"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn invalidating_aliases_copies_only_the_shapes_it_changes() {
    // `delete q.x` drops every fact that may alias `q`. The other 200
    // bindings share one 500-property shape that does not; copying it for
    // each before deciding to keep it cost 312 MiB here, against 27 MiB.
    const PROPERTIES: usize = 500;
    const ALIASES: usize = 200;
    let mut tail = String::from("var shared = {");
    for index in 0..PROPERTIES {
        tail.push_str(&format!("p{index}: {index}, "));
    }
    tail.push_str("};\n");
    for index in 0..ALIASES {
        tail.push_str(&format!("var a{index} = shared;\n"));
    }
    tail.push_str("var q = {x: 1};\ndelete q.x;\n");
    let Some(peak) = methods_then(&tail) else {
        return;
    };
    assert!(
        peak <= 64 << 20,
        "lowering {ALIASES} aliases of a shape and a delete peaked {peak} bytes above the start"
    );
}
