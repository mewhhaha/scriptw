//! Compiled code has no deadline checks. A deadline removes access to the
//! Store's GC heap and private linear memory, so each of these must stop
//! through a different kind of access.
use scriptw_engine::{
    CompileOptions, Engine, ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder,
    RunOptions, WasmExecutionFailureKind,
};
use std::time::{Duration, Instant};

fn assert_times_out(label: &str, source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    let started = Instant::now();
    let error = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions::default(),
            RunOptions {
                timeout_ms: Some(150),
                ..RunOptions::default()
            },
        )
        .expect_err(label);
    assert_eq!(
        error.wasm_execution_failure_kind(),
        Some(WasmExecutionFailureKind::Timeout),
        "{label}: {error}"
    );
    // Compilation is outside the bound; this only rules out a run that was
    // never stopped.
    assert!(
        started.elapsed() < Duration::from_secs(120),
        "{label} took {:?}",
        started.elapsed()
    );
}

#[test]
fn source_loops_and_recursion_stop_without_heap_access() {
    // The poll store at the loop header and at function entry.
    assert_times_out("empty loop", "for (;;) {}");
    assert_times_out(
        "scalar do-while",
        "var i = 0; do { i = (i + 1) | 0; } while (true);",
    );
    // Depth stays below the bound on active source bodies; time does not.
    assert_times_out(
        "exponential recursion",
        "function f(n) { return n < 2 ? n : f(n - 1) + f(n - 2); } f(90);",
    );
    assert_times_out(
        "generator resumption",
        "function* g() { for (;;) yield 1; } var it = g(); for (;;) it.next();",
    );
}

#[test]
fn runtime_loops_stop_at_their_heap_or_linear_memory_access() {
    // A builtin's own loop carries no poll: it reaches the GC heap.
    assert_times_out(
        "sparse indexOf",
        "var a = []; a.length = 4294967295; a.indexOf(1);",
    );
    // The regular expression matcher backtracks in linear memory only.
    assert_times_out(
        "catastrophic regular expression",
        "/^(a+)+$/.test('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa!');",
    );
    // A builtin whose every step enters source code: the poll at function
    // entry stops it even if the builtin itself reached no watched memory.
    assert_times_out(
        "callback loop",
        "var endless = new Proxy({}, { has: function () { return true; }, \
         get: function (target, key) { return key === 'length' ? 1e15 : 0; } }); \
         Array.prototype.some.call(endless, function () { return false; });",
    );
}

#[test]
fn a_run_within_its_budget_is_unaffected_before_and_after_a_timeout() {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    let run = || {
        Engine::new(RealmBuilder::new().build())
            .observe_script(
                "var total = 0; for (var i = 0; i < 100000; i++) total += i; total;",
                CompileOptions::default(),
                RunOptions {
                    timeout_ms: Some(60_000),
                    ..RunOptions::default()
                },
            )
            .expect("a short loop finishes within its budget")
    };
    let expected = ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(
        4999950000.0,
    )));
    assert_eq!(run().completion, expected);
    assert_times_out("empty loop", "for (;;) {}");
    assert_eq!(run().completion, expected);
}
