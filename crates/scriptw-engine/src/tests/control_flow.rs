use super::*;

#[test]
fn wasm_agents_notify_wait_async_promises_across_instances() {
    let source = format!(
        r#"{TEST262_AGENT_PRELUDE}
$262.agent.start("$262.agent.receiveBroadcast(async function (sab) {{ var view = new Int32Array(sab); Atomics.add(view, 1, 1); var outcome = await Atomics.waitAsync(view, 0, 0).value; $262.agent.report(outcome); $262.agent.leaving(); }});");
var sab = new SharedArrayBuffer(8);
var view = new Int32Array(sab);
$262.agent.broadcast(sab);
while (Atomics.load(view, 1) !== 1) {{
  $262.agent.sleep(1);
}}
var notified = 0;
while (notified === 0) {{
  notified = Atomics.notify(view, 0, 1);
  $262.agent.sleep(1);
}}
var report = null;
while (report === null) {{
  $262.agent.sleep(1);
  report = $262.agent.getReport();
}}
notified + ":" + report;
"#
    );
    let outcome = engine()
        .run_wasm_aot_script_with_agents(
            &source,
            test262_compile_options(),
            Some(SHARED_AGENT_STARTUP_EXECUTION_TIMEOUT_MS),
            true,
            TEST262_AGENT_PRELUDE.to_string(),
        )
        .expect("cross-instance notify should settle the originating waitAsync promise");

    assert!(outcome.note.contains("1:ok"), "{}", outcome.note);
}

#[test]
fn wasm_agent_wait_async_timeout_removes_the_host_waiter() {
    let source = format!(
        r#"{TEST262_AGENT_PRELUDE}
$262.agent.start("$262.agent.receiveBroadcast(async function (sab) {{ var view = new Int32Array(sab); var outcome = await Atomics.waitAsync(view, 0, 0, 5).value; $262.agent.report(outcome); $262.agent.leaving(); }});");
var sab = new SharedArrayBuffer(4);
$262.agent.broadcast(sab);
var report = null;
while (report === null) {{
  $262.agent.sleep(1);
  report = $262.agent.getReport();
}}
report;
"#
    );
    let outcome = engine()
        .run_wasm_aot_script_with_agents(
            &source,
            test262_compile_options(),
            Some(SHARED_AGENT_STARTUP_EXECUTION_TIMEOUT_MS),
            true,
            TEST262_AGENT_PRELUDE.to_string(),
        )
        .expect("an agent waitAsync timeout should settle after canceling its host waiter");

    assert!(outcome.note.contains("timed-out"), "{}", outcome.note);
}

#[test]
fn observed_wasm_throw_stays_distinct_from_engine_error_and_legacy_adapter() {
    let engine = engine();
    let source = "throw { secret: 't25-private-property', get message() { print('inspected'); return 'wrong'; } };";
    let outcome = engine
        .observe_script(source, CompileOptions::default(), RunOptions::default())
        .expect("an ECMAScript throw is a successful observed execution");

    assert_eq!(
        outcome.completion,
        ObservedCompletion::Throw(ObservedJsValue::Object)
    );
    assert_eq!(outcome.note, "uncaught ECMAScript throw (object)");
    assert!(outcome.output_events.is_empty());
    assert!(!format!("{outcome:?}").contains("handle@"));

    let error = engine
        .run_script(source, CompileOptions::default(), RunOptions::default())
        .expect_err("the legacy run API must keep throws as EngineError");
    assert!(error.message().starts_with("uncaught throw: "));

    let non_scalar_error = engine
        .observe_script(
            "throw new Error('\\ud800');",
            CompileOptions::default(),
            RunOptions::default(),
        )
        .expect("private legacy diagnostics must not break structured object observation");
    assert_eq!(
        non_scalar_error.completion,
        ObservedCompletion::Throw(ObservedJsValue::Object)
    );
    assert_eq!(non_scalar_error.note, "uncaught ECMAScript throw (object)");
}

#[test]
fn wasm_output_capture_is_enabled_only_for_structured_execution() {
    let legacy = WasmOutputEvents::for_mode(&WasmExecutionMode::Legacy);
    legacy.record("discarded");
    assert!(legacy.take().is_empty());

    let structured = WasmOutputEvents::for_mode(&WasmExecutionMode::Structured);
    structured.record("captured");
    assert_eq!(
        structured.take(),
        vec![HostOutputEvent::PrintLine("captured".to_string())]
    );
}

/// The attribution is appended to the failure detail that `scriptw-test262`
/// classifies with, and both `classify_failure_origin` and
/// `classify_failure_outcome` (`crates/scriptw-test262/src/lib.rs`) match
/// case-insensitive substrings over the whole detail. Re-introducing a
/// symbol name here would silently re-file Cranelift backend defects as
/// `IcuIntl`/`BoaParser`/`NotImplemented` records, so the keyword list those
/// two functions use is duplicated here as a tripwire.
#[test]
fn attribution_is_inert_for_the_test262_failure_classifiers() {
    const CLASSIFIER_KEYWORDS: &[&str] = &[
        "local harness",
        "worker panic",
        "icu_",
        "hijri",
        "intl",
        "datetimeformat",
        "numberformat",
        "durationformat",
        "relativetimeformat",
        "spec-exec",
        "syntaxerror",
        "syntax error",
        "parse",
        "referenceerror",
        "typeerror",
        "rangeerror",
        "urierror",
        "runtime",
        "index out of bounds",
        "must be declarative environment",
        "not implemented",
        "unsupported",
        "not supported",
        "stub",
        "panic",
        "panicked",
        "crash",
        "segmentation fault",
        "trapped",
        "timeout exceeded",
        "wasm `unreachable`",
    ];

    let engine = engine();
    let unit = engine
        .compile_script("var x = 1 + 1;", CompileOptions::default())
        .expect("script compile should succeed");
    let artifact = engine.emit_wasm(&unit).expect("wasm emit should succeed");
    let description = describe_largest_wasm_code_body(&artifact.bytes)
        .expect("an emitted module always has a code section")
        .to_ascii_lowercase();
    for keyword in CLASSIFIER_KEYWORDS {
        assert!(
            !description.contains(keyword),
            "attribution contains classifier keyword {keyword:?}: {description}"
        );
    }
}

#[test]
fn wasm_atomics_wait_async_expiry_skips_blocked_agent_waiters_before_notify() {
    for directive in ["", "'use strict';\n"] {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let source = format!(
            r#"{directive}
                const view = new Int32Array(new SharedArrayBuffer(4));
                const expired = Atomics.waitAsync(view, 0, 0, 1);
                const live = Atomics.waitAsync(view, 0, 0);
                const delay = new Int32Array(new SharedArrayBuffer(4));
                Atomics.wait(delay, 0, 0, 5);
                print('notify:' + Atomics.notify(view, 0, 1));
                expired.value.then(function (status) {{ print('expired:' + status); }});
                live.value.then(function (status) {{ print('live:' + status); }});
            "#
        );
        engine_with_captured_prints(Arc::clone(&lines))
            .run_script(
                &source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(30_000),
                    ..RunOptions::default()
                },
            )
            .expect("native notify must skip expired entries while preserving live FIFO waiters");
        assert_eq!(
            lines.lock().expect("capture mutex poisoned").as_slice(),
            &["notify:1", "expired:timed-out", "live:ok"]
        );
    }
}

#[test]
fn wasm_atomics_wait_async_runs_settled_reactions_before_waiting_again() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            const view = new Int32Array(new SharedArrayBuffer(8));
            const later = Atomics.waitAsync(view, 1, 0, 250);
            const first = Atomics.waitAsync(view, 0, 0, 5);
            first.value.then(function (status) {
                print("first:" + status);
                print("notified:" + Atomics.notify(view, 1, 1));
            });
            later.value.then(function (status) { print("later:" + status); });
            print("sync:" + first.async + ":" + later.async);
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("a settled waiter's reaction must be able to notify another waiter");
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync:true:true",
            "first:timed-out",
            "notified:1",
            "later:ok"
        ]
    );
}

#[test]
fn current_thread_wasm_aot_entry_runs_on_a_sized_worker() {
    let outcome = run_on_sized_stack(|| {
        engine().run_wasm_aot_script_on_current_thread(
            "40 + 2;",
            CompileOptions::default(),
            None,
            true,
        )
    })
    .expect("current-thread Wasm-AOT run should succeed");

    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(outcome.note.contains("number(42)"), "outcome: {outcome:?}");
}

#[test]
fn wasm_emit_succeeds_for_supported_script() {
    let unit = engine()
        .compile_script("1 + 1;", CompileOptions::default())
        .expect("script compile should succeed");
    let artifact = engine().emit_wasm(&unit).expect("wasm emit should succeed");
    assert!(artifact.bytes.starts_with(b"\0asm"));
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn wasm_backend_renders_a_heap_bigint_throw_completion() {
    let error = engine()
        .run_script(
            "throw -340282366920938463481821351505477763073n;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect_err("heap BigInt throw completion should remain observable");

    assert_eq!(
        error.message(),
        "uncaught throw: wasm-aot completion: \
             bigint(-340282366920938463481821351505477763073n)"
    );
}

#[test]
fn wasm_backend_wasm_aot_deadline_bounds_infinite_loop() {
    // CRITICAL correctness check for the in-process default execution
    // path (see `run_case_entry` in scriptw-test262 and
    // `run_with_wasm_aot_inner` above): a genuinely hanging Wasm-AOT
    // module must trap out on its own on a bounded schedule instead of
    // hanging this thread forever, since nothing else (no child process
    // to kill) protects against it anymore.
    let engine = engine();
    // Native compilation is intentionally outside the execution timeout
    // and can take several seconds for today's bootstrap-heavy artifact.
    // Prime both artifact and native-module caches so this assertion
    // measures the interrupted execution boundary it names.
    let warmup = engine
        .run_script(
            "while (true) {}",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(1),
                ..RunOptions::default()
            },
        )
        .expect_err("the warmup infinite loop must also time out");
    assert!(
        warmup.message().contains("timeout exceeded"),
        "expected a timeout-classified warmup error, got: {warmup}"
    );

    let start = std::time::Instant::now();
    let err = engine
        .run_script(
            "while (true) {}",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(300),
                ..RunOptions::default()
            },
        )
        .expect_err("an infinite loop must be interrupted at its deadline, not hang");
    let elapsed = start.elapsed();
    assert!(
        err.message().contains("timeout exceeded"),
        "expected a timeout-classified error, got: {err}"
    );
    // The loop has no call and no heap access of its own, so it is stopped
    // through the store its source loop makes on every iteration. Generous
    // upper bound to stay robust under a loaded scheduler while still
    // proving this is bounded, not "never returns".
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "the deadline should interrupt promptly, took {elapsed:?}"
    );
}

#[test]
fn wasm_backend_json_parse_reviver_context_sources_for_static_json() {
    let outcome = engine()
            .run_script(
                "var ok = true; var result = JSON.parse('[1.0,\"2\",true,null,{\"x\":1}]', function(k, v, c) { if (k === '0') ok = ok && c.source === '1.0'; if (k === '1') ok = ok && c.source === '\"2\"'; if (k === '2') ok = ok && c.source === 'true'; if (k === '3') ok = ok && c.source === 'null'; if (k === 'x') ok = ok && c.source === '1'; if (k === '4') ok = ok && typeof v === 'object' && v.x === 1 && Object.getOwnPropertyNames(c).length === 0; if (k === '') ok = ok && Object.getOwnPropertyNames(c).length === 0; return v; }); ok && result.length === 5 && result[0] === 1 && result[1] === '2' && result[2] === true && result[3] === null && result[4].x === 1;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("JSON.parse reviver context sources should run");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_date_now_advances_promise_timer_retry_loop() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
function setTimeout(callback, delay) {
  let promise = Promise.resolve();
  let endTime = Date.now() + delay;
  function check() {
    if (endTime - Date.now() > 0) {
      promise.then(check);
    } else {
      callback();
    }
  }
  promise.then(check);
}

let polls = 0;
let reports = ["B ok", "A ok"];
function getReport() {
  polls += 1;
  if (polls <= 2) return null;
  return reports.pop();
}

function getReportAsync() {
  return new Promise(function (resolve) {
    function retry() {
      let result = getReport();
      if (result !== null) {
        resolve(result);
      } else {
        setTimeout(retry, 1);
      }
    }
    retry();
  });
}

(async function () {
  let results = [await getReportAsync(), await getReportAsync()];
  results.sort();
  print(results.join(":"));
})();
"#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Date.now should let Promise-based timer polling reach its deadline");
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["A ok:B ok".to_string()]
    );
}

#[test]
fn wasm_backend_const_assignment_throws_type_error() {
    let err = engine()
        .run_script(
            "const x = 1; x = 2;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect_err("const assignment should throw");
    assert!(
        err.message().starts_with("uncaught throw: TypeError: "),
        "{}",
        err.message()
    );
}

#[test]
fn wasm_backend_outlined_to_numeric_uses_calling_realm_for_conversion_errors() {
    let source = r#"
            const other = __scriptwCreateRealm().global;

            function throwsMainRealmTypeError(operation) {
                try {
                    operation();
                    return false;
                } catch (error) {
                    return Object.getPrototypeOf(error) === TypeError.prototype &&
                        error instanceof TypeError &&
                        !(error instanceof other.TypeError);
                }
            }

            const subtractionUsesCallingRealm = throwsMainRealmTypeError(function () {
                return other.Symbol("subtraction") - 1;
            });
            const exponentiationUsesCallingRealm = throwsMainRealmTypeError(function () {
                return other.Symbol("exponentiation") ** 1;
            });

            subtractionUsesCallingRealm && exponentiationUsesCallingRealm;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("outlined ToNumeric should create errors in the calling realm");
    assert!(
        outcome.note.contains("boolean(true"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_loose_equality_routes_coercion_throws_to_catch() {
    let source = r#"
            let sentinel = {};
            let caught = false;
            try {
                1 == {
                    valueOf: function () {
                        throw sentinel;
                    }
                };
            } catch (error) {
                caught = error === sentinel;
            }
            caught;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should route loose equality coercion throws to catch");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_iterator_from_iterable_primitives() {
    let outcome = engine()
            .run_script(
                r#"
Number.prototype[Symbol.iterator] = function* () {
  let i = 0;
  let target = this >>> 0;
  while (i < target) {
    yield i;
    ++i;
  }
};

let primitiveThrows = "no";
try {
  Iterator.from(5);
} catch (e) {
  primitiveThrows = e instanceof TypeError ? "typeerror" : e.name;
}

function numberArraySnapshot(array) {
  return array.length + ":" + array[0] + "," + array[1] + "," + array[2] + "," + array[3] + "," + array[4];
}

let numberArray = numberArraySnapshot(Array.from(5));
let boxedNumberArray = numberArraySnapshot(Array.from(Iterator.from(new Number(5))));
let stringResult = Array.from(Iterator.from("string"));
let stringArray = stringResult.length + ":" + stringResult[0] + stringResult[1] + stringResult[2] + stringResult[3] + stringResult[4] + stringResult[5];

const originalStringIterator = String.prototype[Symbol.iterator];
let observedType;
Object.defineProperty(String.prototype, Symbol.iterator, {
  get() {
    "use strict";
    observedType = typeof this;
    return originalStringIterator;
  },
});
Iterator.from("");
let firstObservedType = observedType;
Iterator.from(new String(""));
let secondObservedType = observedType;

numberArray + "|" + primitiveThrows + "|" + boxedNumberArray + "|" + stringArray + "|" + firstObservedType + "," + secondObservedType;
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("Iterator.from primitive iterable cases should run");
    assert!(
        outcome
            .note
            .contains("string(5:0,1,2,3,4|typeerror|5:0,1,2,3,4|6:string|string,object)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_arguments_iterators_observe_length_truncation() {
    let outcome = engine()
        .run_script(
            r#"
function mapped(a, b, c) {
  let iterator = arguments[Symbol.iterator]();
  iterator.next();
  iterator.next();
  arguments.length = 2;
  let result = iterator.next();
  return typeof result.value + ":" + result.done;
}

function unmapped(a, b, c) {
  "use strict";
  let iterator = arguments[Symbol.iterator]();
  iterator.next();
  iterator.next();
  arguments.length = 2;
  let result = iterator.next();
  return typeof result.value + ":" + result.done;
}

mapped(2, 1, 3) + "|" + unmapped(2, 1, 3);
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("arguments iterators should observe length truncation");
    assert!(
        outcome
            .note
            .contains("string(undefined:true|undefined:true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_string_iterator_preserves_code_points_and_brand() {
    let outcome = engine()
        .run_script(
            r#"
let pair = String.fromCharCode(0xD834, 0xDF06);
let iterator = ("a" + pair + "b")[Symbol.iterator]();
let first = iterator.next();
let second = iterator.next();
let third = iterator.next();
let exhausted = iterator.next();
let prototype = Object.getPrototypeOf(iterator);
let other = __scriptwCreateRealm().global;
let otherIterator = other.String.prototype[Symbol.iterator].call("x");
let otherPrototype = Object.getPrototypeOf(otherIterator);
let realmLocal = otherPrototype !== prototype &&
  Object.getPrototypeOf(otherPrototype) !== Object.getPrototypeOf(prototype);
let marker = {};
let propagated = false;
try {
  String.prototype[Symbol.iterator].call({ toString() { throw marker; } });
} catch (error) {
  propagated = error === marker;
}
first.value + ":" + first.done + "|" +
second.value.length + ":" + (second.value === pair) + "|" +
third.value + ":" + third.done + "|" +
typeof exhausted.value + ":" + exhausted.done + "|" +
prototype[Symbol.toStringTag] + "|" + propagated + "|" + realmLocal;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("String iterator code point and brand cases should run");
    assert!(
        outcome
            .note
            .contains("string(a:false|2:true|b:false|undefined:true|String Iterator|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_cross_realm_eager_iterator_helpers_validate_arguments_in_other_realm() {
    let outcome = engine()
            .run_script(
                r#"
let other = __scriptwCreateRealm().global;
let iteratorPrototype = other.Iterator.prototype;
let helperNames = ["toArray", "forEach", "every", "some", "find", "reduce"];
let callbackNames = ["forEach", "every", "some", "find", "reduce", "map", "filter", "flatMap"];
let nullishResults = [];
let callbackResults = [];
let rangeResults = [];
function collectNullishResults() {
for (let i = 0; i < 11; i++) {
  let name = i < 6 ? helperNames[i] : ["map", "filter", "flatMap", "take", "drop"][i - 6];
  try {
    iteratorPrototype[name].call(null, function() { return true; });
  } catch (error) {
    nullishResults.push(name + ":" + (Object.getPrototypeOf(error) === other.TypeError.prototype) + ":" + (error instanceof other.TypeError) + ":" + (error instanceof TypeError));
  }
}
}
function collectCallbackResults() {
for (let i = 0; i < callbackNames.length; i++) {
  let name = callbackNames[i];
  try {
    iteratorPrototype[name].call(iteratorPrototype, null);
  } catch (error) {
    callbackResults.push(name + ":" + (Object.getPrototypeOf(error) === other.TypeError.prototype) + ":" + (error instanceof other.TypeError) + ":" + (error instanceof TypeError));
  }
}
}
function collectRangeResults() {
for (let i = 0; i < 2; i++) {
  let name = i === 0 ? "take" : "drop";
  try {
    iteratorPrototype[name].call(iteratorPrototype);
  } catch (error) {
    rangeResults.push(name + ":" + (Object.getPrototypeOf(error) === other.RangeError.prototype) + ":" + (error instanceof other.RangeError) + ":" + (error instanceof RangeError));
  }
}
}
collectNullishResults();
collectCallbackResults();
collectRangeResults();
[
  nullishResults.join(","),
  callbackResults.join(","),
  rangeResults.join(",")
].join("|");
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("cross-realm eager Iterator helpers should use their defining realm");
    assert!(
            outcome.note.contains(
                "string(toArray:true:true:false,forEach:true:true:false,every:true:true:false,some:true:true:false,find:true:true:false,reduce:true:true:false,map:true:true:false,filter:true:true:false,flatMap:true:true:false,take:true:true:false,drop:true:true:false|forEach:true:true:false,every:true:true:false,some:true:true:false,find:true:true:false,reduce:true:true:false,map:true:true:false,filter:true:true:false,flatMap:true:true:false|take:true:true:false,drop:true:true:false)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_cross_realm_eager_iterator_helpers_validate_results_in_other_realm() {
    let outcome = engine()
            .run_script(
                r#"
let other = __scriptwCreateRealm().global;
let iteratorPrototype = other.Iterator.prototype;
let helperNames = ["toArray", "forEach", "every", "some", "find", "reduce"];
let nextMethodResults = [];
let nextResultResults = [];
function collectNextMethodResults() {
for (let i = 0; i < helperNames.length; i++) {
  let name = helperNames[i];
  try {
    if (name === "toArray") {
      iteratorPrototype[name].call({ next: null });
    } else {
      iteratorPrototype[name].call({ next: null }, function() { return true; });
    }
  } catch (error) {
    nextMethodResults.push(name + ":" + (Object.getPrototypeOf(error) === other.TypeError.prototype) + ":" + (error instanceof other.TypeError) + ":" + (error instanceof TypeError));
  }
}
}
function collectNextResultResults() {
for (let i = 0; i < helperNames.length; i++) {
  let name = helperNames[i];
  try {
    if (name === "toArray") {
      iteratorPrototype[name].call({ next: function() { return 1; } });
    } else {
      iteratorPrototype[name].call({ next: function() { return 1; } }, function() { return true; });
    }
  } catch (error) {
    nextResultResults.push(name + ":" + (Object.getPrototypeOf(error) === other.TypeError.prototype) + ":" + (error instanceof other.TypeError) + ":" + (error instanceof TypeError));
  }
}
}
collectNextMethodResults();
collectNextResultResults();
let reduceEmpty = "none";
try {
  iteratorPrototype.reduce.call({ next: function() { return { done: true }; } }, function(a, b) { return a; });
} catch (error) {
  reduceEmpty = [(Object.getPrototypeOf(error) === other.TypeError.prototype), error instanceof other.TypeError, error instanceof TypeError].join(":");
}
[nextMethodResults.join(","), nextResultResults.join(","), reduceEmpty].join("|");
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("cross-realm eager Iterator results should use their defining realm");
    assert!(
            outcome.note.contains(
                "string(toArray:true:true:false,forEach:true:true:false,every:true:true:false,some:true:true:false,find:true:true:false,reduce:true:true:false|toArray:true:true:false,forEach:true:true:false,every:true:true:false,some:true:true:false,find:true:true:false,reduce:true:true:false|true:true:false)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_cross_realm_lazy_iterator_helpers_use_other_realm_type_error() {
    let outcome = engine()
            .run_script(
                r#"
let other = __scriptwCreateRealm().global;
let iteratorPrototype = other.Iterator.prototype;
let nextResults = [];
let returnResults = [];
let checks = [
  ["map", iteratorPrototype.map.call({ next: function() { return 1; } }, function(value) { return value; })],
  ["filter", iteratorPrototype.filter.call({ next: function() { return 1; } }, function(value) { return true; })],
  ["flatMap", iteratorPrototype.flatMap.call({ next: function() { return 1; } }, function(value) { return [value]; })],
  ["take", iteratorPrototype.take.call({ next: function() { return 1; } }, 1)],
  ["drop", iteratorPrototype.drop.call({ next: function() { return 1; } }, 0)]
];
for (let i = 0; i < checks.length; i++) {
  try {
    checks[i][1].next();
  } catch (error) {
    nextResults.push(
      checks[i][0] + ":" +
      (Object.getPrototypeOf(error) === other.TypeError.prototype) + ":" +
      (error instanceof other.TypeError) + ":" +
      (error instanceof TypeError)
    );
  }
}
for (let i = 0; i < checks.length; i++) {
  try {
    checks[i][1].return.call({});
  } catch (error) {
    returnResults.push(
      checks[i][0] + ":" +
      (Object.getPrototypeOf(error) === other.TypeError.prototype) + ":" +
      (error instanceof other.TypeError) + ":" +
      (error instanceof TypeError)
    );
  }
}
[nextResults.join(","), returnResults.join(",")].join("|");
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("cross-realm lazy Iterator helpers should use their defining realm");
    assert!(
            outcome.note.contains(
                "string(map:true:true:false,filter:true:true:false,flatMap:true:true:false,take:true:true:false,drop:true:true:false|map:true:true:false,filter:true:true:false,flatMap:true:true:false,take:true:true:false,drop:true:true:false)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_cross_realm_iterator_from_uses_other_realm_type_error() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm().global;
let C = other.Iterator;
let throwResults = [];
let nextPrimitive;
let returnPrimitive;
function recordThrow(label, thunk) {
  try {
    thunk();
  } catch (error) {
    throwResults.push(
      label + ":" +
      (Object.getPrototypeOf(error) === other.TypeError.prototype) + ":" +
      (error instanceof other.TypeError) + ":" +
      (error instanceof TypeError)
    );
  }
}
recordThrow("null", function() { C.from(null); });
recordThrow("method", function() {
  let value = {};
  value[Symbol.iterator] = 1;
  C.from(value);
});
recordThrow("methodResult", function() {
  let value = {};
  value[Symbol.iterator] = function() { return 1; };
  C.from(value);
});
recordThrow("nextMethod", function() {
  let value = { next: 1 };
  value[Symbol.iterator] = function() { return this; };
  C.from(value).next();
});
// WrapForValidIterator forwards the Call result without IteratorResult validation.
{
  let value = { next: function() { return 1; } };
  value[Symbol.iterator] = function() { return this; };
  nextPrimitive = C.from(value).next();
}
recordThrow("nextReceiver", function() {
  let value = { next: function() { return { done: true }; } };
  value[Symbol.iterator] = function() { return this; };
  C.from(value).next.call({});
});
recordThrow("returnMethod", function() {
  let value = {
    next: function() { return { done: false, value: 1 }; },
    return: 1
  };
  value[Symbol.iterator] = function() { return this; };
  C.from(value).return();
});
{
  let value = {
    next: function() { return { done: false, value: 1 }; },
    return: function() { return 1; }
  };
  value[Symbol.iterator] = function() { return this; };
  returnPrimitive = C.from(value).return();
}
recordThrow("returnReceiver", function() {
  let value = {
    next: function() { return { done: false, value: 1 }; },
    return: function() { return { done: true }; }
  };
  value[Symbol.iterator] = function() { return this; };
  C.from(value).return.call({});
});
let nextThis = "unset";
let result = { done: false, value: 42 };
let value = {
  next: function() {
    nextThis = this === value;
    return result;
  }
};
value[Symbol.iterator] = function() { return this; };
let wrapper = C.from(value);
let nextResult = wrapper.next();
[
  throwResults.join(","),
  nextThis,
  nextResult === result,
  typeof wrapper.next,
  nextPrimitive === 1,
  returnPrimitive === 1
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("cross-realm Iterator.from should use its defining realm");
    assert!(
            outcome.note.contains(
                "string(null:true:true:false,method:true:true:false,methodResult:true:true:false,nextMethod:true:true:false,nextReceiver:true:true:false,returnMethod:true:true:false,returnReceiver:true:true:false|true|true|function|true|true)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_cross_realm_borrowed_and_bound_builtin_throw_other_realm_type_error() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm();
let method = other.global.String.prototype.toString;
let bound = method.bind(null);
let direct = "missing";
let boundResult = "missing";

try {
  method.call(null);
} catch (error) {
  direct =
    (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
    (error instanceof other.global.TypeError) + ":" +
    (error instanceof TypeError);
}

try {
  bound();
} catch (error) {
  boundResult =
    (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
    (error instanceof other.global.TypeError) + ":" +
    (error instanceof TypeError);
}

direct + "|" + boundResult;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("borrowed and bound cross-realm builtins should throw in defining realm");
    assert!(
        outcome
            .note
            .contains("string(true:true:false|true:true:false)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_cross_realm_regexp_escape_throws_other_realm_type_error() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm();
let escape = other.global.RegExp.escape;
let bound = escape.bind(null);
let direct = "missing";
let boundResult = "missing";

try {
  escape(1);
} catch (error) {
  direct =
    (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
    (error instanceof other.global.TypeError) + ":" +
    (error instanceof TypeError);
}

try {
  bound(1);
} catch (error) {
  boundResult =
    (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
    (error instanceof other.global.TypeError) + ":" +
    (error instanceof TypeError);
}

direct + "|" + boundResult;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("borrowed and bound cross-realm RegExp.escape should throw in defining realm");
    assert!(
        outcome
            .note
            .contains("string(true:true:false|true:true:false)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_cross_realm_regexp_match_all_iterator_uses_defining_realm() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm();
let regexp = new other.global.RegExp("a", "g");
let iterator = other.global.String.prototype.matchAll.call("a", regexp);
// %RegExpStringIteratorPrototype% of each Realm (22.2.9.2), not an Array Iterator.
let otherRegExpStringIteratorPrototype = Object.getPrototypeOf(
  other.global.RegExp.prototype[Symbol.matchAll].call(new other.global.RegExp("a", "g"), "a")
);
let regExpStringIteratorPrototype = Object.getPrototypeOf(/a/g[Symbol.matchAll]("a"));
let iteratorPrototype = Object.getPrototypeOf(iterator);
let next = iteratorPrototype.next;
let direct = "missing";

try {
  next.call({});
} catch (error) {
  direct =
    (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
    (error instanceof other.global.TypeError) + ":" +
    (error instanceof TypeError);
}

let nextResult = next.call(iterator);
[
  iteratorPrototype === otherRegExpStringIteratorPrototype,
  iteratorPrototype === regExpStringIteratorPrototype,
  direct,
  Object.getPrototypeOf(nextResult) === other.global.Object.prototype,
  Object.getPrototypeOf(nextResult) === Object.prototype
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("cross-realm RegExp matchAll iterator should use defining realm intrinsics");
    assert!(
        outcome
            .note
            .contains("string(true|false|true:true:false|true|false)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_catches_regexp_test_abrupt_completions() {
    let outcome = engine()
        .run_script(
            r#"
var coercionThrow;
try {
  /a/.test({ toString: function() { throw "coercion"; } });
} catch (error) {
  coercionThrow = error;
}

var receiverThrow;
var ordinary = { test: RegExp.prototype.test };
try {
  ordinary.test("a");
} catch (error) {
  receiverThrow = error instanceof TypeError;
}

[coercionThrow, receiverThrow].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("RegExp test abrupt completions should reach the active catch");
    assert!(
        outcome.note.contains("string(coercion|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_evaluates_ordinary_computed_key_before_nullish_throw() {
    let outcome = engine()
        .run_script(
            r#"
let keyCalls = 0;
function stringKey() { keyCalls += 1; return "value"; }
let nullThrew = false;
try {
  null[stringKey()];
} catch (error) {
  nullThrew = error instanceof TypeError;
}
let undefinedThrew = false;
try {
  undefined[keyCalls += 1];
} catch (error) {
  undefinedThrew = error instanceof TypeError;
}
[nullThrew, undefinedThrew, keyCalls].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("ordinary computed keys should be evaluated before the nullish receiver check");
    assert!(
        outcome.note.contains("string(true|true|2)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_cross_realm_date_get_time_throws_other_realm_type_error() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm();
let method = other.global.Date.prototype.getTime;
let bound = method.bind({});
let direct = "missing";
let boundResult = "missing";

try {
  method.call({});
} catch (error) {
  direct =
    (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
    (error instanceof other.global.TypeError) + ":" +
    (error instanceof TypeError);
}

try {
  bound();
} catch (error) {
  boundResult =
    (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
    (error instanceof other.global.TypeError) + ":" +
    (error instanceof TypeError);
}

direct + "|" + boundResult;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect(
            "borrowed and bound cross-realm Date.prototype.getTime should throw in defining realm",
        );
    assert!(
        outcome
            .note
            .contains("string(true:true:false|true:true:false)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_cross_realm_bigint_methods_throw_other_realm_type_error() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm();
let names = ["toString", "toLocaleString", "valueOf"];
let results = [];

for (let i = 0; i < names.length; i++) {
  let method = other.global.BigInt.prototype[names[i]];
  let bound = method.bind(null);
  let direct = "missing";
  let boundResult = "missing";

  try {
    method.call(null);
  } catch (error) {
    direct =
      (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
      (error instanceof other.global.TypeError) + ":" +
      (error instanceof TypeError);
  }

  try {
    bound();
  } catch (error) {
    boundResult =
      (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
      (error instanceof other.global.TypeError) + ":" +
      (error instanceof TypeError);
  }

  results.push(names[i] + "=" + direct + "|" + boundResult);
}

results.join(",");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("borrowed and bound cross-realm BigInt methods should throw in defining realm");
    assert!(
            outcome.note.contains(
                "string(toString=true:true:false|true:true:false,toLocaleString=true:true:false|true:true:false,valueOf=true:true:false|true:true:false)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_cross_realm_number_methods_throw_other_realm_type_error() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm();
let names = ["toString", "toLocaleString", "valueOf"];
let results = [];

for (let i = 0; i < names.length; i++) {
  let method = other.global.Number.prototype[names[i]];
  let bound = method.bind(null);
  let direct = "missing";
  let boundResult = "missing";

  try {
    method.call(null);
  } catch (error) {
    direct =
      (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
      (error instanceof other.global.TypeError) + ":" +
      (error instanceof TypeError);
  }

  try {
    bound();
  } catch (error) {
    boundResult =
      (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
      (error instanceof other.global.TypeError) + ":" +
      (error instanceof TypeError);
  }

  results.push(names[i] + "=" + direct + "|" + boundResult);
}

results.join(",");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("borrowed and bound cross-realm Number methods should throw in defining realm");
    assert!(
            outcome.note.contains(
                "string(toString=true:true:false|true:true:false,toLocaleString=true:true:false|true:true:false,valueOf=true:true:false|true:true:false)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_cross_realm_boolean_methods_throw_other_realm_type_error() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm();
let names = ["toString", "valueOf"];
let results = [];

for (let i = 0; i < names.length; i++) {
  let method = other.global.Boolean.prototype[names[i]];
  let bound = method.bind(null);
  let direct = "missing";
  let boundResult = "missing";

  try {
    method.call(null);
  } catch (error) {
    direct =
      (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
      (error instanceof other.global.TypeError) + ":" +
      (error instanceof TypeError);
  }

  try {
    bound();
  } catch (error) {
    boundResult =
      (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
      (error instanceof other.global.TypeError) + ":" +
      (error instanceof TypeError);
  }

  results.push(names[i] + "=" + direct + "|" + boundResult);
}

results.join(",");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("borrowed and bound cross-realm Boolean methods should throw in defining realm");
    assert!(
            outcome.note.contains(
                "string(toString=true:true:false|true:true:false,valueOf=true:true:false|true:true:false)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_cross_realm_error_to_string_throws_other_realm_type_error() {
    let outcome = engine()
            .run_script(
                r#"
let other = __scriptwCreateRealm();
let method = other.global.Error.prototype.toString;
let bound = method.bind(null);
let direct = "missing";
let boundResult = "missing";

try {
  method.call(null);
} catch (error) {
  direct =
    (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
    (error instanceof other.global.TypeError) + ":" +
    (error instanceof TypeError);
}

try {
  bound();
} catch (error) {
  boundResult =
    (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
    (error instanceof other.global.TypeError) + ":" +
    (error instanceof TypeError);
}

direct + "|" + boundResult;
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("borrowed and bound cross-realm Error.prototype.toString should throw in defining realm");
    assert!(
        outcome
            .note
            .contains("string(true:true:false|true:true:false)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_cross_realm_string_methods_throw_other_realm_type_error() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm();
let names = [
  "at", "charAt", "concat", "endsWith", "includes", "indexOf", "isWellFormed",
  "match", "matchAll", "padEnd", "padStart", "repeat", "replace",
  "replaceAll", "search", "slice", "split", "startsWith", "toUpperCase",
  "toWellFormed", "trim", "trimEnd", "trimStart"
];
let results = [];

for (let i = 0; i < names.length; i++) {
  let method = other.global.String.prototype[names[i]];
  let bound = method.bind(null, "x");
  let direct = "missing";
  let boundResult = "missing";

  try {
    method.call(null, "x");
  } catch (error) {
    direct =
      (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
      (error instanceof other.global.TypeError) + ":" +
      (error instanceof TypeError);
  }

  try {
    bound();
  } catch (error) {
    boundResult =
      (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
      (error instanceof other.global.TypeError) + ":" +
      (error instanceof TypeError);
  }

  results.push(names[i] + "=" + direct + "|" + boundResult);
}

results.join(",");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("borrowed and bound cross-realm String methods should throw in defining realm");
    assert!(
            outcome.note.contains(
                "string(at=true:true:false|true:true:false,charAt=true:true:false|true:true:false,concat=true:true:false|true:true:false,endsWith=true:true:false|true:true:false,includes=true:true:false|true:true:false,indexOf=true:true:false|true:true:false,isWellFormed=true:true:false|true:true:false,match=true:true:false|true:true:false,matchAll=true:true:false|true:true:false,padEnd=true:true:false|true:true:false,padStart=true:true:false|true:true:false,repeat=true:true:false|true:true:false,replace=true:true:false|true:true:false,replaceAll=true:true:false|true:true:false,search=true:true:false|true:true:false,slice=true:true:false|true:true:false,split=true:true:false|true:true:false,startsWith=true:true:false|true:true:false,toUpperCase=true:true:false|true:true:false,toWellFormed=true:true:false|true:true:false,trim=true:true:false|true:true:false,trimEnd=true:true:false|true:true:false,trimStart=true:true:false|true:true:false)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_string_split_orders_and_catches_limit_coercion() {
    let source = "var order = []; var separator = { toString: function() { order.push('separator'); throw 'separator'; } }; var limit = { valueOf: function() { order.push('limit'); throw 'limit'; } }; var caught = 'none'; try { 'x y'.split(separator, limit); } catch (error) { caught = error; } var pieces = new String('one two three').split(/ /, 2); caught + '|' + order.join(',') + '|' + pieces.join(',');";
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("String.prototype.split should preserve coercion order and catches");
    assert!(
        outcome.note.contains("string(limit|limit|one,two)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_string_to_lower_case_handles_unicode_and_coercion_throws() {
    let source = r#"
let receiver = {
  toString() {
    throw "coercion";
  }
};
receiver.toLowerCase = String.prototype.toLowerCase;
let caught = "none";
try {
  receiver.toLowerCase();
} catch (error) {
  caught = error;
}
[
  "ABC".toLowerCase(),
  "\u0130".toLowerCase(),
  "ΟΣ".toLowerCase(),
  "ΟΣΑ".toLowerCase(),
  "A\u180EΣ".toLowerCase(),
  "ΟΣ".toLocaleLowerCase(),
  caught,
  "".toLowerCase().index === undefined
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("String.prototype.toLowerCase should implement Unicode casing and coercion");
    assert!(
        outcome
            .note
            .contains("string(abc|i̇|ος|οσα|a᠎ς|ος|coercion|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_string_to_upper_case_handles_unicode_and_coercion_throws() {
    let source = r#"
let receiver = {
  toString() {
    throw "coercion";
  }
};
receiver.toUpperCase = String.prototype.toUpperCase;
let caught = "none";
try {
  receiver.toUpperCase();
} catch (error) {
  caught = error;
}
[
  "straße".toUpperCase(),
  "\u0390".toUpperCase(),
  "\uD801\uDC28".toUpperCase(),
  "straße".toLocaleUpperCase(),
  caught
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("String.prototype.toUpperCase should implement Unicode casing and coercion");
    assert!(
        outcome
            .note
            .contains("string(STRASSE|Ϊ́|𐐀|STRASSE|coercion)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_supports_while_loop() {
    let outcome = engine()
        .run_script(
            "let i = 0; while (i < 3) { i = i + 1; } i;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should run while loop");
    assert!(outcome.note.contains("number(3"));
}

#[test]
fn wasm_backend_supports_do_while_loop() {
    let outcome = engine()
        .run_script(
            "let i = 0; do { i = i + 1; } while (i < 3); i;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should run do while loop");
    assert!(outcome.note.contains("number(3"));
}

#[test]
fn wasm_backend_supports_for_break_and_continue() {
    let outcome = engine()
            .run_script(
                "let i = 0; let sum = 0; for (; i < 5; i = i + 1) { if (i === 2) { continue; } if (i === 4) { break; } sum = sum + i; } sum;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("wasm backend should run for loop");
    assert!(outcome.note.contains("number(4"));
}

#[test]
fn wasm_backend_supports_switch_and_labels() {
    let outcome = engine()
            .run_script(
                "let x = 0; outer: while (x < 3) { x += 1; switch (x) { case 1: continue outer; case 2: debugger; break outer; default: x = 9; } } x;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("wasm backend should run switch and labels");
    assert!(outcome.note.contains("number(2"));
}

#[test]
fn wasm_backend_supports_default_in_middle_switch() {
    let outcome = engine()
            .run_script(
                "let x = 0; switch (3) { case 1: x = 1; break; default: x = 9; break; case 3: x = 3; } x;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("wasm backend should run default-in-middle switch");
    assert!(outcome.note.contains("number(3"));
}

#[test]
fn wasm_backend_throws_type_error_for_const_update() {
    // Rewritten from `wasm_backend_rejects_const_update_precisely`, which
    // pinned `unsupported in scriptw wasm-aot first slice: update of const
    // binding`. That refusal was wrong: 13.4.4.1 evaluates the operand,
    // coerces it with ToNumeric, and only then reaches the PutValue that
    // fails, so `const x = 1; x++` is a program that *runs* and throws a
    // TypeError. The old string made
    // `assert.throws(TypeError, function() { for (const i = 0; i < 1; i++) {} })`
    // a NotImplemented instead of a pass. Deleting this test rather than
    // rewriting it would have made the fix vacuous, so it now asserts the
    // throw, that it is catchable, and that the binding is unchanged.
    let outcome = engine()
            .run_script(
                "let log = ''; const x = 1; try { x++; } catch (e) { log = (e instanceof TypeError) ? 'TypeError' : e.name; } log + '|' + x;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("const update should compile and throw at run time");
    assert!(
        outcome.note.contains("string(TypeError|1)"),
        "{}",
        outcome.note
    );
}

#[test]
fn wasm_backend_sequences_const_compound_assignment_operand_before_throw() {
    // 13.15.2 step 1: the RHS is evaluated, and
    // ApplyStringOrNumericBinaryOperator is applied, *before* PutValue
    // fails. `immutable_binding_write` puts the applied result on the lhs
    // of a `Comma` for exactly this; a bare RuntimeThrow would drop the
    // `log = 'a'` and still pass a test that only checked the error.
    let outcome = engine()
            .run_script(
                "let log = ''; const x = 0; try { x += (log = log + 'a', 1); } catch (e) { log = log + (e instanceof TypeError ? 'T' : e.name); } log + '|' + x;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("const compound assignment should compile and throw at run time");
    assert!(outcome.note.contains("string(aT|0)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_short_circuits_const_logical_assignment_without_throwing() {
    // `||=` on a truthy const never reaches PutValue, so it is not an error
    // at all; `&&=` on the same binding does. Both used to be refused by
    // one `assignment to const binding` site, which is why this asserts the
    // pair rather than only the throwing half.
    let outcome = engine()
            .run_script(
                "const x = 1; let short = 'no'; x ||= 2; short = 'yes'; let threw = 'no'; try { x &&= 3; } catch (e) { threw = (e instanceof TypeError) ? 'yes' : e.name; } short + '|' + threw + '|' + x;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("logical assignment to a const should compile");
    assert!(
        outcome.note.contains("string(yes|yes|1)"),
        "{}",
        outcome.note
    );
}

#[test]
fn wasm_backend_runs_zero_iterations_for_undefined_for_in_target() {
    // 14.7.5.6 step 3.a: a nullish head is a break completion, so the loop
    // runs zero times and nothing throws. This used to be
    // `unsupported in scriptw wasm-aot first slice: for-in non-enumerable
    // target`, which is test262 `language/statements/for-in/S12.6.4_A1.js`.
    let outcome = engine()
        .run_script(
            "let ran = 'no'; for (var key in undefined) { ran = 'yes'; } ran + '|' + key;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("a for-in over undefined must compile");
    assert!(
        outcome.note.contains("string(no|undefined)"),
        "{}",
        outcome.note
    );
}

#[test]
fn wasm_backend_runs_for_in_head_effects_for_a_nullish_target() {
    // Anti-vacuity for the above: steps 1-2 of ForIn/OfHeadEvaluation still
    // evaluate the head. Returning `StatementIr::Empty` instead of the
    // lowered head would pass every test262 case in this family and lose
    // `bump()`, so the effect is asserted here rather than nowhere.
    let outcome = engine()
            .run_script(
                "let hits = 0; function bump() { hits = hits + 1; return 0; } let ran = 'no'; for (var k in (bump(), null)) { ran = 'yes'; } hits + '|' + ran + '|' + k;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("a for-in over a statically null head must compile");
    assert!(
        outcome.note.contains("string(1|no|undefined)"),
        "{}",
        outcome.note
    );
}

#[test]
fn wasm_backend_does_not_type_a_zero_iteration_for_in_key_as_a_string() {
    // Anti-vacuity for the two tests above. Both read the loop variable
    // through a string concatenation, which is tag-dispatched at run time,
    // so they pass whether or not the key is correctly TYPED. Arithmetic is
    // not tag-dispatched: `infer_var_binding_info_from_statement`'s
    // `ForInLoop` arm published a proven `String` for any `var` named as a
    // for-in key, `seed_script_global_var_properties` installed that as the
    // script global's `proven_present` info, and `combine_arithmetic`'s
    // `lhs_proves_string` test then lowered `key + 1` to
    // `ExprIr::StringConcat`. The program answered `"undefined1"` where the
    // spec requires `NaN`.
    //
    // `r !== r` is the oracle rather than `typeof`: it is true for exactly
    // `NaN` and false for every string, so neither answer can be reached by
    // accident.
    let outcome = engine()
        .run_script(
            "for (var key in null) { key; } var r = key + 1; r !== r;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("a for-in over null followed by arithmetic on the key must compile");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_supports_label_on_non_loop_statement() {
    let outcome = engine()
        .run_script(
            "let value = 0; label: if (true) { value = 1; break label; value = 2; } value;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("labels should target any statement");
    assert!(outcome.note.contains("number(1"), "note: {}", outcome.note);
}

#[test]
fn wasm_backend_supports_var_in_for_and_duplicate_var() {
    let outcome = engine()
        .run_script(
            "var sum = 0; for (var i = 0; i < 4; i++) { sum += i; } var sum; sum;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should support var in for");
    assert!(outcome.note.contains("number(6"));
}

#[test]
fn wasm_backend_runtime_throws_for_non_constructable_new_and_instanceof_tails() {
    for source in [
        "try { new (() => 1)(); } catch (e) { e.name; }",
        "try { let o = { f() {} }; new o.f(); } catch (e) { e.name; }",
        "try { let o = { get x() { return 1; } }; new o.x(); } catch (e) { e.name; }",
        "try { new print(); } catch (e) { e.name; }",
    ] {
        let outcome = engine()
            .run_script(
                source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .unwrap_or_else(|err| {
                panic!("non-constructable runtime case should run for `{source}`: {err:?}")
            });
        assert!(
            outcome.note.contains("string(TypeError)"),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_branches_from_catch_blocks_to_enclosing_loops() {
    let outcome = engine()
        .run_script(
            r#"
let continued = 0;
for (let index = 0; index < 3; index++) {
  try {
    throw index;
  } catch (error) {
    if (error < 2) continue;
  }
  continued++;
}
continued;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("catch-block continue should branch to the enclosing loop");
    assert!(outcome.note.contains("number(1)"), "note: {}", outcome.note);
}

#[test]
fn wasm_backend_propagates_aggregate_error_iterator_getter_throw() {
    let source = "function E(message) { this.message = message; } let it = { get [Symbol.iterator]() { throw new E(\"boom\"); } }; try { new AggregateError(it); } catch (e) { e instanceof E && e.constructor === E && e.message === \"boom\"; }";
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("AggregateError iterator getter throw should run: {err:?}"));
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_surfaces_uncaught_phase_twenty_three_throws() {
    for source in [
        "throw 1;",
        "class C {} C();",
        "class A {} class B extends A { constructor() { this.x = 1; super(); } } new B();",
        "class A {} class B extends A { constructor() {} } new B();",
    ] {
        let err = engine()
            .run_script(
                source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect_err("uncaught throw should surface as engine error");
        assert!(
            err.message().starts_with("uncaught throw: "),
            "source: {source}, err: {}",
            err.message()
        );
        if source.contains("extends A") {
            assert!(
                err.message()
                    .starts_with("uncaught throw: ReferenceError: "),
                "source: {source}, err: {}",
                err.message()
            );
        }
    }
}

#[test]
fn wasm_backend_map_group_by_closes_only_for_callback_abrupt_completion() {
    let outcome = engine()
        .run_script(
            r#"
function CallbackError() {}
function NextError() {}
function DoneError() {}
function ValueError() {}
var closeCount = 0;
function iterableWithNext(next) {
  return {
    [Symbol.iterator]: function() {
      return {
        next: next,
        return: function() {
          closeCount += 1;
          throw new TypeError("suppressed close failure");
        }
      };
    }
  };
}
var callbackError = new CallbackError();
var callbackIdentity = false;
try {
  Map.groupBy(iterableWithNext(function() {
    return { done: false, value: 1 };
  }), new Proxy(function() { throw callbackError; }, {}));
} catch (error) {
  callbackIdentity = error === callbackError;
}
var callbackClosed = closeCount === 1;
var nextIdentity = false;
var nextError = new NextError();
try {
  Map.groupBy(iterableWithNext(function() { throw nextError; }), function() {});
} catch (error) {
  nextIdentity = error === nextError;
}
var nextDidNotClose = closeCount === 1;
var doneIdentity = false;
var doneError = new DoneError();
try {
  Map.groupBy(iterableWithNext(function() {
    return { get done() { throw doneError; } };
  }), function() {});
} catch (error) {
  doneIdentity = error === doneError;
}
var doneDidNotClose = closeCount === 1;
var valueIdentity = false;
var valueError = new ValueError();
try {
  Map.groupBy(iterableWithNext(function() {
    return { done: false, get value() { throw valueError; } };
  }), function() {});
} catch (error) {
  valueIdentity = error === valueError;
}
var valueDidNotClose = closeCount === 1;
var nonObjectThrows = false;
try {
  Map.groupBy(iterableWithNext(function() { return 1; }), function() {});
} catch (error) {
  nonObjectThrows = error instanceof TypeError;
}
[
  callbackIdentity,
  callbackClosed,
  nextIdentity,
  nextDidNotClose,
  doneIdentity,
  doneDidNotClose,
  valueIdentity,
  valueDidNotClose,
  nonObjectThrows,
  closeCount
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Map.groupBy should apply IteratorClose only to callback abrupt completion");
    assert!(
        outcome
            .note
            .contains("string(true|true|true|true|true|true|true|true|true|1)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_map_constructs_from_entry_iterables() {
    let outcome = engine()
            .run_script(
                "var map = new Map([[1, 'a'], [2, 'b'], [1, 'c']]); var stringThrows = false; try { new Map('a'); } catch (error) { stringThrows = error instanceof TypeError; } [map.size, map.get(1), map.get(2), stringThrows].join('|');",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("Map should consume entry iterables and reject primitive entries");
    assert!(
        outcome.note.contains("string(2|c|b|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_map_captures_setter_before_acquiring_iterator() {
    let outcome = engine()
        .run_script(
            r#"
var events = [];
var intrinsicSet = Map.prototype.set;
class DerivedMap extends Map {}
Object.defineProperty(DerivedMap.prototype, "set", {
  configurable: true,
  get: function() {
    events.push("get set");
    return function(key, value) {
      events.push("set " + key + ":" + value);
      return intrinsicSet.call(this, key, value);
    };
  }
});
var index = 0;
var iterable = {};
Object.defineProperty(iterable, Symbol.iterator, {
  get: function() {
    events.push("get iterator");
    return function() {
      events.push("call iterator");
      return {
        get next() {
          events.push("get next");
          return function() {
            events.push("next");
            if (index === 2) return { done: true };
            index += 1;
            var entryIndex = index;
            return {
              done: false,
              value: {
                get 0() { events.push("get 0"); return entryIndex; },
                get 1() { events.push("get 1"); return entryIndex === 1 ? "a" : "b"; }
              }
            };
          };
        }
      };
    };
  }
});
var map = new DerivedMap(iterable);
events.join("|") + ":" + map.size;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Map should observe constructor iteration in specification order");
    assert!(
            outcome.note.contains(
                "string(get set|get iterator|call iterator|get next|next|get 0|get 1|set 1:a|next|get 0|get 1|set 2:b|next:2)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_map_closes_only_after_entry_or_set_abrupt_completions() {
    let outcome = engine()
        .run_script(
            r#"
function NextError() {}
function ValueError() {}
function FirstError() {}
function SecondError() {}
function SetError() {}
var closeCount = 0;
function iterableWithNext(next) {
  return {
    [Symbol.iterator]: function() {
      return {
        next: next,
        return: function() {
          closeCount += 1;
          throw new TypeError("suppressed close failure");
        }
      };
    }
  };
}
var nextErrorPreserved = false;
try {
  new Map(iterableWithNext(function() { throw new NextError(); }));
} catch (error) {
  nextErrorPreserved = error instanceof NextError;
}
var nextDidNotClose = closeCount === 0;
var valueErrorPreserved = false;
try {
  new Map(iterableWithNext(function() {
    return { done: false, get value() { throw new ValueError(); } };
  }));
} catch (error) {
  valueErrorPreserved = error instanceof ValueError;
}
var valueDidNotClose = closeCount === 0;
var nonObjectRejected = false;
try {
  new Map(iterableWithNext(function() { return { done: false, value: 1 }; }));
} catch (error) {
  nonObjectRejected = error instanceof TypeError;
}
var firstErrorPreserved = false;
try {
  new Map(iterableWithNext(function() {
    return { done: false, value: { get 0() { throw new FirstError(); } } };
  }));
} catch (error) {
  firstErrorPreserved = error instanceof FirstError;
}
var secondErrorPreserved = false;
try {
  new Map(iterableWithNext(function() {
    return { done: false, value: { 0: 1, get 1() { throw new SecondError(); } } };
  }));
} catch (error) {
  secondErrorPreserved = error instanceof SecondError;
}
var intrinsicSet = Map.prototype.set;
Map.prototype.set = function() { throw new SetError(); };
var setErrorPreserved = false;
try {
  new Map(iterableWithNext(function() {
    return { done: false, value: [1, 2] };
  }));
} catch (error) {
  setErrorPreserved = error instanceof SetError;
}
Map.prototype.set = intrinsicSet;
[nextErrorPreserved, nextDidNotClose, valueErrorPreserved, valueDidNotClose,
 nonObjectRejected, firstErrorPreserved, secondErrorPreserved, setErrorPreserved,
 closeCount].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Map should close only after entry processing or its captured setter throws");
    assert!(
        outcome
            .note
            .contains("string(true|true|true|true|true|true|true|true|4)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_map_for_each_observes_callback_contracts_and_mutations() {
    let source = r#"
var context = {};
var calls = [];
var map = new Map([["a", 1], ["b", 2], ["c", 3]]);
var callback = new Proxy(function(value, key, receiver) {
  calls.push(key + ":" + value + ":" + (receiver === map) + ":" + (this === context));
  if (key === "a") {
    map.delete("b");
    map.set("d", 4);
  }
  if (key === "c" && value === 3) {
    map.delete("c");
    map.set("c", 30);
  }
}, {});
var result = map.forEach(callback, context);
var cleared = new Map([["x", 1], ["y", 2]]);
var clearCalls = [];
cleared.forEach(function(value, key) {
  clearCalls.push(key);
  if (key === "x") {
    cleared.clear();
    cleared.set("z", 3);
  }
});
[result === undefined, calls.join(","), clearCalls.join(",")].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Map.prototype.forEach should observe callback and mutation semantics");
    assert!(
        outcome
            .note
            .contains("string(true|a:1:true:true,c:3:true:true,d:4:true:true,c:30:true:true|x,z)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_map_for_each_checks_brands_and_preserves_callback_throws() {
    let source = r#"
var brandThrows = false;
var callbackThrows = false;
try { Map.prototype.forEach.call({}, function() {}); } catch (error) {
  brandThrows = error instanceof TypeError;
}
try { new Map().forEach(null); } catch (error) {
  callbackThrows = error instanceof TypeError;
}
var expected = new Error("map callback");
var actual;
try {
  new Map([[1, 2]]).forEach(new Proxy(function() { throw expected; }, {}));
} catch (error) {
  actual = error;
}
[brandThrows, callbackThrows, actual === expected].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Map.prototype.forEach should enforce brands and preserve callback throws");
    assert!(
        outcome.note.contains("string(true|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_map_iterators_are_live_and_remain_exhausted() {
    let source = r#"
var map = new Map([["a", 1], ["b", 2]]);
var keys = map.keys();
var first = keys.next();
map.delete("b");
map.set("c", 3);
var second = keys.next();
var done = keys.next();
map.set("d", 4);
var remainsDone = keys.next();
var entry = map.entries().next().value;
[
  first.value === "a" && first.done === false,
  second.value === "c" && second.done === false,
  done.done === true,
  remainsDone.done === true,
  entry[0] === "a" && entry[1] === 1
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Map iterators should preserve insertion order and live mutation semantics");
    assert!(
        outcome.note.contains("string(true|true|true|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_map_iterators_have_spec_surface_and_brand_checks() {
    let source = r#"
var iterator = new Map([[1, 2]]).values();
var prototype = Object.getPrototypeOf(iterator);
var brandThrows = false;
try { prototype.next.call({}); } catch (error) { brandThrows = error instanceof TypeError; }
[
  Map.prototype.entries === Map.prototype[Symbol.iterator],
  iterator[Symbol.iterator]() === iterator,
  Object.getPrototypeOf(prototype)[Symbol.iterator].call(iterator) === iterator,
  Object.prototype.toString.call(iterator) === "[object Map Iterator]",
  brandThrows
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Map iterators should expose the dedicated iterator prototype surface");
    assert!(
        outcome.note.contains("string(true|true|true|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_set_predicates_close_early_iterators_and_preserve_realms_and_abrupt_values() {
    let outcome = engine()
            .run_script(
                r#"
var closeCount = 0;
function earlyIterator(value, returnMethod) {
  return {
    next: function() { return { done: false, value: value }; },
    return: returnMethod
  };
}
var disjoint = new Set([1, 2]).isDisjointFrom({
  size: 1,
  has: function() {},
  keys: function() { return earlyIterator(2, function() { closeCount += 1; return {}; }); }
});
var superset = new Set([1, 2]).isSupersetOf({
  size: 1,
  has: function() {},
  keys: function() { return earlyIterator(3, function() { closeCount += 1; return {}; }); }
});

var closeError = {};
var closeIdentity = false;
try {
  new Set([1]).isSupersetOf({
    size: 1,
    has: function() {},
    keys: function() { return earlyIterator(2, function() { throw closeError; }); }
  });
} catch (error) { closeIdentity = error === closeError; }

var nextError = {};
var nextIdentity = false;
var closeAfterNextThrow = 0;
try {
  new Set([1]).isSupersetOf({
    size: 1,
    has: function() {},
    keys: function() { return {
      next: function() { throw nextError; },
      return: function() { closeAfterNextThrow += 1; return {}; }
    }; }
  });
} catch (error) { nextIdentity = error === nextError; }

var hasError = {};
var hasIdentity = false;
try {
  new Set([1]).isSubsetOf({
    size: 1,
    has: function() { throw hasError; },
    keys: function() {}
  });
} catch (error) { hasIdentity = error === hasError; }

var closeResultTypeError = false;
try {
  new Set([1]).isSupersetOf({
    size: 1,
    has: function() {},
    keys: function() { return earlyIterator(2, function() { return 1; }); }
  });
} catch (error) { closeResultTypeError = error instanceof TypeError; }

var other = __scriptwCreateRealm().global;
var realmResult = other.Set.prototype.isSubsetOf.call(new other.Set([1]), new Set([1, 2]));
var brandRealm = false;
try { other.Set.prototype.isSubsetOf.call({}, new Set()); } catch (error) {
  brandRealm = error instanceof other.TypeError && !(error instanceof TypeError);
}
var sizeRangeRealm = false;
try {
  other.Set.prototype.isSupersetOf.call(new other.Set(), {
    size: -1,
    has: function() {},
    keys: function() {}
  });
} catch (error) { sizeRangeRealm = error instanceof other.RangeError && !(error instanceof RangeError); }
[
  disjoint,
  superset,
  closeCount,
  closeIdentity,
  nextIdentity,
  hasIdentity,
  closeAfterNextThrow,
  closeResultTypeError,
  realmResult,
  brandRealm,
  sizeRangeRealm
].join("|");
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("Set predicates should close early iterators and preserve abrupt values");
    assert!(
        outcome
            .note
            .contains("string(false|false|2|true|true|true|0|true|true|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_set_captures_adder_before_acquiring_iterator() {
    let outcome = engine()
        .run_script(
            r#"
var events = [];
var intrinsicAdd = Set.prototype.add;
class DerivedSet extends Set {}
Object.defineProperty(DerivedSet.prototype, "add", {
  configurable: true,
  get: function() {
    events.push("get add");
    return function(value) {
      events.push("add " + value);
      return intrinsicAdd.call(this, value);
    };
  }
});
var index = 0;
var iterable = {};
Object.defineProperty(iterable, Symbol.iterator, {
  get: function() {
    events.push("get iterator");
    return function() {
      events.push("call iterator");
      return {
        get next() {
          events.push("get next");
          return function() {
            events.push("next");
            if (index === 2) return { done: true };
            index += 1;
            return {
              done: false,
              get value() {
                events.push("value");
                return index;
              }
            };
          };
        }
      };
    };
  }
});
var set = new DerivedSet(iterable);
events.join("|") + ":" + set.size;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Set should observe constructor iteration in specification order");
    assert!(
            outcome.note.contains(
                "string(get add|get iterator|call iterator|get next|next|value|add 1|next|value|add 2|next:2)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_set_for_each_observes_callback_contracts_and_mutations() {
    let source = r#"
var context = {};
var calls = [];
var set = new Set(["a", "b", "c"]);
var callback = new Proxy(function(value, key, receiver) {
  calls.push(value + ":" + (key === value) + ":" + (receiver === set) + ":" + (this === context));
  if (value === "a") {
    set.delete("b");
    set.add("d");
  }
  if (value === "c") {
    set.delete("c");
    set.add("C");
  }
}, {});
var result = set.forEach(callback, context);
var cleared = new Set(["x", "y"]);
var clearCalls = [];
cleared.forEach(function(value) {
  clearCalls.push(value);
  if (value === "x") {
    cleared.clear();
    cleared.add("z");
  }
});
[result === undefined, calls.join(","), clearCalls.join(",")].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Set.prototype.forEach should observe callback and mutation semantics");
    assert!(
        outcome.note.contains(
            "string(true|a:true:true:true,c:true:true:true,d:true:true:true,C:true:true:true|x,z)"
        ),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_set_for_each_checks_brands_and_preserves_callback_throws() {
    let source = r#"
var brandThrows = false;
var callbackThrows = false;
try { Set.prototype.forEach.call({}, function() {}); } catch (error) {
  brandThrows = error instanceof TypeError;
}
try { new Set().forEach(null); } catch (error) {
  callbackThrows = error instanceof TypeError;
}
var expected = new Error("set callback");
var actual;
try {
  new Set([1]).forEach(new Proxy(function() { throw expected; }, {}));
} catch (error) {
  actual = error;
}
[brandThrows, callbackThrows, actual === expected].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Set.prototype.forEach should enforce brands and preserve callback throws");
    assert!(
        outcome.note.contains("string(true|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_set_iterators_are_live_and_remain_exhausted() {
    let source = r#"
var set = new Set(["a", "b"]);
var values = set.values();
var first = values.next();
set.delete("b");
set.add("c");
var second = values.next();
var done = values.next();
set.add("d");
var remainsDone = values.next();
var entry = set.entries().next().value;
[
  first.value === "a" && first.done === false,
  second.value === "c" && second.done === false,
  done.done === true,
  remainsDone.done === true,
  entry[0] === "a" && entry[1] === "a"
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Set iterators should preserve insertion order and live mutation semantics");
    assert!(
        outcome.note.contains("string(true|true|true|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_set_iterators_have_spec_surface_and_brand_checks() {
    let source = r#"
var iterator = new Set([1]).values();
var prototype = Object.getPrototypeOf(iterator);
var brandThrows = false;
try { prototype.next.call({}); } catch (error) { brandThrows = error instanceof TypeError; }
[
  Set.prototype.values === Set.prototype.keys,
  Set.prototype.values === Set.prototype[Symbol.iterator],
  iterator[Symbol.iterator]() === iterator,
  Object.getPrototypeOf(prototype)[Symbol.iterator].call(iterator) === iterator,
  Object.prototype.toString.call(iterator) === "[object Set Iterator]",
  brandThrows
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Set iterators should expose the dedicated iterator prototype surface");
    assert!(
        outcome
            .note
            .contains("string(true|true|true|true|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_collection_iterators_use_the_method_defining_realm() {
    let source = r#"
var other = __scriptwCreateRealm().global;
var mapIterator = other.Map.prototype.keys.call(new other.Map([[1, 2]]));
var setIterator = other.Set.prototype.values.call(new other.Set([1]));
[
  Object.getPrototypeOf(mapIterator) !== Object.getPrototypeOf(new Map().keys()),
  Object.getPrototypeOf(setIterator) !== Object.getPrototypeOf(new Set().values()),
  other.Map.prototype.entries === other.Map.prototype[other.Symbol.iterator],
  other.Set.prototype.values === other.Set.prototype.keys,
  other.Set.prototype.values === other.Set.prototype[other.Symbol.iterator],
  mapIterator.next().value === 1,
  setIterator.next().value === 1
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("collection iterators should use their method defining realm prototypes");
    assert!(
        outcome
            .note
            .contains("string(true|true|true|true|true|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_promise_reactions_run_after_synchronous_code_in_registration_order() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let settle;
            let promise = new Promise(function (resolve) {
                settle = resolve;
            });
            promise.then(function (value) { print("first:" + value); });
            promise.then(function (value) { print("second:" + value); });
            print("sync");
            settle(4);
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("pending Promise reactions should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync".to_string(),
            "first:4".to_string(),
            "second:4".to_string()
        ]
    );
}

#[test]
fn wasm_backend_drains_promise_jobs_after_top_level_throw_without_replacing_it() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            Promise.resolve(1).then(function () {
                print("job");
                throw new RangeError("secondary");
            });
            throw new TypeError("primary");
        "#;
    let error = engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect_err("the primary top-level throw must remain observable");
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["job".to_string(), "RangeError: secondary".to_string()],
        "a top-level throw must drain queued jobs and report their unhandled rejections"
    );
    assert!(
        error.message().contains("uncaught throw: TypeError")
            && error.message().contains("primary"),
        "the queued rejection must not replace the primary completion: {error:?}"
    );
    assert!(
        !error.message().contains("secondary"),
        "secondary job diagnostics must not overwrite the primary throw: {error:?}"
    );
}

#[test]
fn wasm_backend_async_arrows_preserve_lexical_captures_and_are_not_constructable() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            function make(prefix) {
                const expression = async delta =>
                    this.marker + arguments[0] + delta
                        + (new.target === undefined ? 1 : 100);
                const block = async delta => {
                    await Promise.resolve();
                    return this.marker + arguments[0] + delta
                        + (new.target === undefined ? 1 : 100);
                };
                return [expression, block];
            }
            const holder = { marker: 40 };
            const arrows = make.call(holder, 1);
            const expression = arrows[0];
            const block = arrows[1];
            print(expression.name);
            print(block.name);
            const expressionResult = expression(0);
            const blockResult = block(2);
            print(expressionResult instanceof Promise);
            print(blockResult instanceof Promise);
            try { new expression(); } catch (error) {
                print(error instanceof TypeError);
            }
            try { new block(); } catch (error) {
                print(error instanceof TypeError);
            }
            expressionResult.then(function (value) { print("expression:" + value); });
            blockResult.then(function (value) { print("block:" + value); });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async arrows should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "expression".to_string(),
            "block".to_string(),
            "true".to_string(),
            "true".to_string(),
            "true".to_string(),
            "true".to_string(),
            "sync".to_string(),
            "expression:42".to_string(),
            "block:44".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_for_await_string_iterates_unicode_code_points() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            async function collect() {
                let summaries = [];
                for await (const value of "a\uD834\uDF06b\uD834\uDF06\uDF06\uD834") {
                    summaries.push(
                        value.length + ":" + value.charCodeAt(0) + ":" + value.charCodeAt(1)
                    );
                }
                return summaries.join("|");
            }
            collect().then(function (summary) { print(summary); });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("string for-await-of should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["1:97:NaN|2:55348:57094|1:98:NaN|2:55348:57094|1:57094:NaN|1:55348:NaN".to_string()]
    );
}

#[test]
fn wasm_backend_for_await_string_awaits_sync_iterator_close_value() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let events = [];
            let stringIteratorPrototype = Object.getPrototypeOf(""[Symbol.iterator]());
            stringIteratorPrototype.return = function () {
                events.push("return:" + arguments.length);
                return {
                    value: Promise.resolve().then(function () {
                        events.push("close-value");
                    }),
                    done: true
                };
            };
            async function consume() {
                for await (const value of "ab") {
                    events.push("body:" + value);
                    break;
                }
                events.push("after");
            }
            consume().then(function () {
                events.push("settled");
                print(events.join(":"));
            });
            events.push("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("string for-await-of close should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync:body:a:return:0:close-value:after:settled".to_string()]
    );
}

#[test]
fn wasm_backend_for_await_string_prefers_async_iterator_with_primitive_receiver() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let calls = "";
            let iterator = {
                index: 0,
                next: function () {
                    calls += "n";
                    this.index = this.index + 1;
                    return Promise.resolve(this.index === 1
                        ? { value: "async", done: false }
                        : { value: undefined, done: true });
                }
            };
            String.prototype[Symbol.asyncIterator] = function () {
                "use strict";
                calls += this === "source" ? "a" : "w";
                return iterator;
            };
            String.prototype[Symbol.iterator] = function () {
                calls += "s";
                throw new Error("sync iterator must not be selected");
            };
            async function collect() {
                let result = "";
                for await (const value of "source") result += value;
                return result;
            }
            collect().then(function (result) { print(result + ":" + calls); });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("string async iterator preference should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["async:ann".to_string()]
    );
}

#[test]
fn wasm_backend_for_await_prefers_async_iterator_and_awaits_next_results() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let calls = "";
            let iterator = {
                index: 0,
                next: function () {
                    calls += "n";
                    this.index = this.index + 1;
                    return Promise.resolve(this.index === 1
                        ? { value: 9, done: false }
                        : {
                            get value() { throw new Error("done result value must not be read"); },
                            done: true
                        });
                }
            };
            let iterable = {};
            iterable[Symbol.asyncIterator] = function () {
                calls += "a";
                return iterator;
            };
            iterable[Symbol.iterator] = function () {
                calls += "s";
                throw new Error("sync iterator must not be selected");
            };
            async function collect() {
                let total = 0;
                for await (const value of iterable) total += value;
                return total;
            }
            collect().then(function (total) { print(total + ":" + calls); });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async iterator acquisition should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["9:ann".to_string()]
    );
}

#[test]
fn wasm_backend_for_await_dispatches_native_synchronous_next_throws_before_await() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let synchronousError = {};
            let rejectionError = {};
            let events = [];
            function asyncIterable(next) {
                let iterator = { next: next };
                let iterable = {};
                iterable[Symbol.asyncIterator] = function () { return iterator; };
                return iterable;
            }
            async function consumeSynchronousThrow() {
                try {
                    for await (const value of asyncIterable(function () {
                        throw synchronousError;
                    })) {}
                } catch (error) {
                    events.push(error === synchronousError ? "sync-caught" : "sync-wrong");
                }
            }
            async function consumeRejectedNext() {
                try {
                    for await (const value of asyncIterable(function () {
                        return Promise.reject(rejectionError);
                    })) {}
                } catch (error) {
                    events.push(error === rejectionError ? "reject-caught" : "reject-wrong");
                }
            }
            Promise.resolve().then(function () { events.push("queued"); });
            let synchronousResult = consumeSynchronousThrow();
            events.push("after-call");
            synchronousResult.then(function () {
                events.push("sync-settled");
                consumeRejectedNext().then(function () {
                    events.push("reject-settled");
                    print(events.join(":"));
                });
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("native async iterator next failures should execute: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync-caught:after-call:queued:sync-settled:reject-caught:reject-settled".to_string()]
    );
}

#[test]
fn wasm_backend_for_await_falls_back_only_for_nullish_async_iterator_methods() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            function syncIterable(asyncMethod) {
                let iterable = {};
                iterable[Symbol.asyncIterator] = asyncMethod;
                iterable[Symbol.iterator] = function () {
                    let done = false;
                    return {
                        next: function () {
                            if (done) return { done: true };
                            done = true;
                            return { value: Promise.resolve(3), done: false };
                        }
                    };
                };
                return iterable;
            }
            async function first(iterable) {
                for await (const value of iterable) return value;
            }
            async function rejectsNonCallable() {
                try {
                    for await (const value of syncIterable(1)) {}
                } catch (error) {
                    return error instanceof TypeError;
                }
                return false;
            }
            first(syncIterable(null)).then(function (fallbackValue) {
                rejectsNonCallable().then(function (rejected) {
                    print(fallbackValue + ":" + rejected);
                });
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async iterator fallback should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["3:true".to_string()]
    );
}

#[test]
fn wasm_backend_for_await_awaits_and_validates_async_iterator_close_results() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            function asyncIterable(closeResult) {
                let closeCalls = 0;
                let iterator = {
                    next: function () {
                        return Promise.resolve({ value: 1, done: false });
                    },
                    return: function () {
                        closeCalls += 1;
                        return Promise.resolve(closeResult);
                    }
                };
                let iterable = { closeCalls: function () { return closeCalls; } };
                iterable[Symbol.asyncIterator] = function () { return iterator; };
                return iterable;
            }
            async function closes(iterable) {
                for await (const value of iterable) break;
                return iterable.closeCalls();
            }
            async function rejectsPrimitiveClose(iterable) {
                try {
                    for await (const value of iterable) break;
                } catch (error) {
                    return error instanceof TypeError && iterable.closeCalls() === 1;
                }
                return false;
            }
            let valid = asyncIterable({});
            let invalid = asyncIterable(1);
            closes(valid).then(function (closeCalls) {
                rejectsPrimitiveClose(invalid).then(function (rejected) {
                    print(closeCalls + ":" + rejected);
                });
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async iterator close should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["1:true".to_string()]
    );
}

#[test]
fn wasm_backend_for_await_applies_async_close_get_method_abrupt_precedence() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let bodyError = {};
            let closeError = {};
            let closeGetterCalls = 0;
            function iterableWithThrowingReturn() {
                let iterator = {
                    next: function () { return { value: 1, done: false }; },
                    get return() {
                        closeGetterCalls += 1;
                        throw closeError;
                    }
                };
                let iterable = {};
                iterable[Symbol.asyncIterator] = function () { return iterator; };
                return iterable;
            }
            function iterableWithNonCallableReturn() {
                let iterator = {
                    next: function () { return { value: 1, done: false }; },
                    return: 1
                };
                let iterable = {};
                iterable[Symbol.asyncIterator] = function () { return iterator; };
                return iterable;
            }
            async function closeFailureWinsBreak() {
                try {
                    for await (const value of iterableWithThrowingReturn()) break;
                } catch (error) {
                    return error === closeError;
                }
                return false;
            }
            async function bodyFailureWinsClose() {
                try {
                    for await (const value of iterableWithThrowingReturn()) throw bodyError;
                } catch (error) {
                    return error === bodyError;
                }
                return false;
            }
            async function nonCallableCloseFailureWinsBreak() {
                try {
                    for await (const value of iterableWithNonCallableReturn()) break;
                } catch (error) {
                    return error instanceof TypeError;
                }
                return false;
            }
            async function bodyFailureWinsNonCallableClose() {
                try {
                    for await (const value of iterableWithNonCallableReturn()) throw bodyError;
                } catch (error) {
                    return error === bodyError;
                }
                return false;
            }
            closeFailureWinsBreak().then(function (closeWins) {
                bodyFailureWinsClose().then(function (bodyWins) {
                    nonCallableCloseFailureWinsBreak().then(function (nonCallableCloseWins) {
                        bodyFailureWinsNonCallableClose().then(function (nonCallableBodyWins) {
                            print(
                                closeWins + ":" +
                                bodyWins + ":" +
                                closeGetterCalls + ":" +
                                nonCallableCloseWins + ":" +
                                nonCallableBodyWins
                            );
                        });
                    });
                });
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async iterator GetMethod close should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["true:true:2:true:true".to_string()]
    );
}

#[test]
fn wasm_backend_for_await_sync_iterator_closes_on_break() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let closeCalls = 0;
            let closeArguments = -1;
            function iterableWith(value) {
                let iterator = {
                    next: function () { return { value: value, done: false }; },
                    return: function () {
                        closeCalls += 1;
                        closeArguments = arguments.length;
                        return { value: undefined, done: true };
                    }
                };
                let iterable = {};
                iterable[Symbol.iterator] = function () { return iterator; };
                return iterable;
            }
            async function breakAfterOne() {
                for await (const value of iterableWith(6)) break;
                return "break";
            }
            breakAfterOne().then(function (value) {
                print(value + ":" + closeCalls + ":" + closeArguments);
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("sync iterator close should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync".to_string(), "break:1:0".to_string()]
    );
}

#[test]
fn wasm_backend_for_await_sync_iterator_closes_on_return() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let closeCalls = 0;
            let closeArguments = -1;
            let iterator = {
                next: function () { return { value: 7, done: false }; },
                return: function () {
                    closeCalls += 1;
                    closeArguments = arguments.length;
                    return { value: undefined, done: true };
                }
            };
            let iterable = {};
            iterable[Symbol.iterator] = function () { return iterator; };
            async function returnFirst(iterable) {
                for await (const value of iterable) return value;
            }
            returnFirst(iterable).then(function (value) {
                print(value + ":" + closeCalls + ":" + closeArguments);
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("sync iterator return close should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync".to_string(), "7:1:0".to_string()]
    );
}

#[test]
fn wasm_backend_for_await_sync_iterator_preserves_throw_close_precedence() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let bodyError = {};
            let closeError = {};
            function iterableWithReturn(returnDescriptor) {
                let iterator = {
                    next: function () { return { value: 1, done: false }; }
                };
                Object.defineProperty(iterator, "return", returnDescriptor);
                let iterable = {};
                iterable[Symbol.iterator] = function () { return iterator; };
                return iterable;
            }
            async function thrownBodyWins() {
                try {
                    for await (const value of iterableWithReturn({
                        get: function () { throw closeError; }
                    })) throw bodyError;
                } catch (error) {
                    return error === bodyError;
                }
            }
            async function closeFailureWinsBreak() {
                try {
                    for await (const value of iterableWithReturn({
                        value: function () { throw closeError; }
                    })) break;
                } catch (error) {
                    return error === closeError;
                }
                return false;
            }
            thrownBodyWins().then(function (bodyWins) {
                closeFailureWinsBreak().then(function (closeWins) {
                    print(bodyWins && closeWins);
                });
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("sync iterator close precedence should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["true".to_string()]
    );
}

#[test]
fn wasm_backend_async_finalizers_override_pending_completions() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let rejects = async function () {
                try { return "early-return"; }
                finally {
                    await Promise.reject("override-reject");
                    print("unreachable-reject");
                }
            };
            let fulfills = async function () {
                try { await Promise.reject("early-reject"); }
                finally {
                    return await Promise.resolve("override-return");
                    print("unreachable-return");
                }
            };
            let catches = async function () {
                try { await Promise.reject("caught-reject"); }
                catch (error) {
                    await Promise.resolve();
                    return error;
                } finally {
                    await Promise.resolve();
                }
            };
            rejects().then(undefined, function (value) { print("rejected:" + value); });
            fulfills().then(function (value) { print("fulfilled:" + value); });
            catches().then(function (value) { print("caught:" + value); });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async finalizers should settle promises: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync".to_string(),
            "rejected:override-reject".to_string(),
            "fulfilled:override-return".to_string(),
            "caught:caught-reject".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_await_resumes_lexical_state_and_propagates_rejection() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            async function retained() {
                let value = 40;
                let awaited;
                awaited = await Promise.resolve(2);
                return value + awaited;
            }
            async function rejected() {
                await Promise.reject("bad");
                return "unreachable";
            }
            retained().then(function (value) { print("fulfilled:" + value); });
            rejected().then(undefined, function (reason) { print("rejected:" + reason); });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async await should resume through Promise jobs: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync".to_string(),
            "fulfilled:42".to_string(),
            "rejected:bad".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_composite_await_declarations_preserve_order_and_abrupt_completion() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            function record(label, value) {
                print(label);
                return value;
            }
            function stop() {
                print("stop");
                throw "abrupt";
            }
            function stopBeforeAwait() {
                print("stop-before-await");
                throw "before-await";
            }
            async function calculate() {
                const lexical =
                    record("lex-left", 10) +
                    await Promise.resolve(record("lex-await", 2)) *
                    record("lex-right", 3);
                print("lexical:" + lexical);
                var variable =
                    -(await Promise.resolve(record("var-await", 4))) +
                    record("var-right", 1);
                print("variable:" + variable);
            }
            async function failBeforeAwait() {
                const unreachable =
                    stopBeforeAwait() +
                    await Promise.resolve(record("skipped-await", 1));
                return unreachable;
            }
            async function fail() {
                const unreachable =
                    record("fail-left", 1) +
                    await Promise.resolve(record("fail-await", 2)) *
                    stop() +
                    await Promise.resolve(record("after-abrupt", 3));
                return unreachable;
            }
            calculate()
                .then(function () { return failBeforeAwait(); })
                .then(undefined, function (reason) {
                    print("rejected:" + reason);
                    return fail();
                })
                .then(undefined, function (reason) { print("rejected:" + reason); });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("composite await declarations should resume in source order: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "lex-left".to_string(),
            "lex-await".to_string(),
            "sync".to_string(),
            "lex-right".to_string(),
            "lexical:16".to_string(),
            "var-await".to_string(),
            "var-right".to_string(),
            "variable:-3".to_string(),
            "stop-before-await".to_string(),
            "rejected:before-await".to_string(),
            "fail-left".to_string(),
            "fail-await".to_string(),
            "stop".to_string(),
            "rejected:abrupt".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_awaited_var_redeclaration_uses_resumed_runtime_type() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            async function lengthAfterAwait(promise) {
                var value = "old";
                var value = await promise;
                return value.length;
            }
            lengthAfterAwait(Promise.resolve([1, 2])).then(function (length) {
                print("length:" + length);
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("awaited var redeclaration should use its resumed runtime type: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync".to_string(), "length:2".to_string()]
    );
}

#[test]
fn wasm_backend_nullish_optional_chain_skips_awaited_computed_key() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let baseCalls = 0;
            async function read() {
                let value = (baseCalls += 1, undefined)?.[
                    await Promise.reject("unreachable")
                ];
                return value === undefined;
            }
            read().then(function (skipped) {
                print("skipped:" + skipped + ":" + baseCalls);
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("nullish optional chain should skip its awaited key: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync".to_string(), "skipped:true:1".to_string()]
    );
}

#[test]
fn wasm_backend_async_generator_await_resumes_once_and_drains_queued_next_requests() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let prefixRuns = 0;
            let suffixRuns = 0;
            let release;
            let blocking = new Promise(function (resolve) { release = resolve; });
            async function* stream() {
                prefixRuns += 1;
                let value;
                value = await blocking;
                suffixRuns += 1;
                return value + 1;
            }
            let iterator = stream();
            let first = iterator.next();
            let second = iterator.next();
            first.then(function (result) {
                print("first:" + result.value + ":" + result.done + ":" + prefixRuns + ":" + suffixRuns);
            });
            second.then(function (result) {
                print("second:" + result.value + ":" + result.done);
            });
            print("sync:" + prefixRuns + ":" + suffixRuns);
            release(41);
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator Await should resume: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync:1:0".to_string(),
            "first:42:true:1:1".to_string(),
            "second:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_invalid_receivers_reject_intrinsic_promises() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            const other = __scriptwCreateRealm().global;
            async function* stream() {}
            function* syncStream() {}

            const methods = [
                stream.prototype.next,
                stream.prototype.return,
                stream.prototype.throw
            ];
            const receivers = [1, {}, function () {}, syncStream()];
            const rejections = [];
            let synchronous = true;
            let synchronousThrows = 0;
            let intrinsicPromises = 0;
            let rejected = 0;
            let fulfilled = 0;
            let asynchronousRejections = 0;
            let definingRealmTypeErrors = 0;

            function observe(method, receiver) {
                let promise;
                try {
                    promise = method.call(receiver);
                } catch (error) {
                    synchronousThrows += 1;
                    return;
                }
                if (promise instanceof Promise) intrinsicPromises += 1;
                rejections.push(promise.then(
                    function () {
                        fulfilled += 1;
                    },
                    function (error) {
                        rejected += 1;
                        if (!synchronous) asynchronousRejections += 1;
                        if (
                            Object.getPrototypeOf(error) === TypeError.prototype &&
                            error instanceof TypeError &&
                            !(error instanceof other.TypeError)
                        ) {
                            definingRealmTypeErrors += 1;
                        }
                    }
                ));
            }

            for (let methodIndex = 0; methodIndex < methods.length; methodIndex += 1) {
                for (let receiverIndex = 0; receiverIndex < receivers.length; receiverIndex += 1) {
                    observe(methods[methodIndex], receivers[receiverIndex]);
                }
            }

            synchronous = false;
            print("sync:" + synchronousThrows + ":" + rejections.length + ":" + rejected);
            Promise.all(rejections).then(function () {
                print(
                    "settled:" +
                    rejected + ":" +
                    fulfilled + ":" +
                    asynchronousRejections + ":" +
                    intrinsicPromises + ":" +
                    definingRealmTypeErrors
                );
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("invalid async-generator receivers should reject promises: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync:0:12:0".to_string(),
            "settled:12:0:12:12:12".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_iterator_async_dispose_awaits_return_and_rejects_abrupt_completion() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            async function* stream() {}
            const prototype = Object.getPrototypeOf(Object.getPrototypeOf(stream.prototype));
            const method = prototype[Symbol.asyncDispose];
            const other = __scriptwCreateRealm().global;
            const getterError = {};
            const callError = {};
            const rejection = {};
            const observations = [];
            let synchronous = true;
            let synchronousThrows = 0;
            let intrinsicPromises = 0;
            let asynchronousSettlements = 0;
            let fulfilledUndefined = 0;
            let rejectedByIdentity = 0;
            let definingRealmTypeErrors = 0;
            let returnCalls = 0;
            let returnArgumentCount = 0;
            let returnArgumentUndefined = 0;
            let returnThisPreserved = 0;

            function observe(receiver, expectedRejection) {
                let promise;
                try {
                    promise = method.call(receiver);
                } catch (error) {
                    synchronousThrows += 1;
                    return;
                }
                if (promise instanceof Promise) intrinsicPromises += 1;
                observations.push(promise.then(
                    function (value) {
                        if (!synchronous) asynchronousSettlements += 1;
                        if (value === undefined) fulfilledUndefined += 1;
                    },
                    function (error) {
                        if (!synchronous) asynchronousSettlements += 1;
                        if (error === expectedRejection) rejectedByIdentity += 1;
                        if (
                            expectedRejection === TypeError &&
                            Object.getPrototypeOf(error) === TypeError.prototype &&
                            error instanceof TypeError &&
                            !(error instanceof other.TypeError)
                        ) {
                            definingRealmTypeErrors += 1;
                        }
                    }
                ));
            }

            const successful = Object.create(prototype);
            successful.return = function (value) {
                returnCalls += 1;
                returnArgumentCount += arguments.length;
                if (value === undefined) returnArgumentUndefined += 1;
                if (this === successful) returnThisPreserved += 1;
                return Promise.resolve({ done: true });
            };
            observe(successful);
            observe(Object.create(prototype));

            const throwingGetter = Object.create(prototype);
            Object.defineProperty(throwingGetter, "return", {
                get: function () { throw getterError; }
            });
            observe(throwingGetter, getterError);

            const throwingCall = Object.create(prototype);
            throwingCall.return = function () { throw callError; };
            observe(throwingCall, callError);

            const rejectingCall = Object.create(prototype);
            rejectingCall.return = function () { return Promise.reject(rejection); };
            observe(rejectingCall, rejection);

            const nonCallable = Object.create(prototype);
            nonCallable.return = 1;
            observe(nonCallable, TypeError);
            observe(undefined, TypeError);

            synchronous = false;
            print("sync:" + synchronousThrows + ":" + intrinsicPromises);
            Promise.all(observations).then(function () {
                print(
                    "return:" +
                    returnCalls + ":" +
                    returnArgumentCount + ":" +
                    returnArgumentUndefined + ":" +
                    returnThisPreserved
                );
                print(
                    "settled:" +
                    asynchronousSettlements + ":" +
                    fulfilledUndefined + ":" +
                    rejectedByIdentity + ":" +
                    definingRealmTypeErrors
                );
            });
        "#;

    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("AsyncIterator asyncDispose should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync:0:7".to_string(),
            "return:1:0:1:1".to_string(),
            "settled:7:2:3:2".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_yields_settle_queued_requests_in_fifo_order() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let settled = 0;
            let prefixRuns = 0;
            async function* stream() {
                prefixRuns += 1;
                yield "first";
                yield "second";
            }
            let iterator = stream();
            let first = iterator.next();
            let second = iterator.next();
            let third = iterator.next();
            third.then(function (result) {
                settled += 1;
                print("third:" + settled + ":" + result.value + ":" + result.done + ":" + prefixRuns);
            });
            second.then(function (result) {
                settled += 1;
                print("second:" + settled + ":" + result.value + ":" + result.done);
            });
            first.then(function (result) {
                settled += 1;
                print("first:" + settled + ":" + result.value + ":" + result.done);
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("queued async-generator yields should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "first:1:first:false".to_string(),
            "second:2:second:false".to_string(),
            "third:3:undefined:true:1".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_executes_no_suspension_conditional_branches() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            async function* choose(flag) {
                let value = 0;
                if (flag) {
                    value = 1;
                    if (false) value = 2;
                } else {
                    value = 3;
                }
                print(value);
            }
            choose(true).next();
            choose(false).next();
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator ordinary conditionals should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["1".to_string(), "3".to_string()]
    );
}

#[test]
fn wasm_backend_async_generator_await_rejects_active_request_and_completes_queue() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let reason = {};
            async function* stream() {
                await Promise.reject(reason);
                return "unreachable";
            }
            let iterator = stream();
            iterator.next().then(undefined, function (error) {
                print("rejected:" + (error === reason));
            });
            iterator.next().then(function (result) {
                print("completed:" + result.value + ":" + result.done);
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("rejected async-generator Await should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync".to_string(),
            "rejected:true".to_string(),
            "completed:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_return_await_assimilates_thenable_once() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let getterCalls = 0;
            let thenable = {};
            Object.defineProperty(thenable, "then", {
                get: function () {
                    getterCalls += 1;
                    return function (resolve, reject) {
                        resolve(8);
                        reject(9);
                    };
                }
            });
            async function* stream() { return await thenable; }
            stream().next().then(function (result) {
                print("result:" + result.value + ":" + result.done + ":" + getterCalls);
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator return Await should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync".to_string(), "result:8:true:1".to_string()]
    );
}

#[test]
fn wasm_backend_async_generator_yield_statements_resolve_before_completion() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            async function* withoutValue() { yield; }
            async function* withValue() { yield 1; }
            let first = withoutValue();
            first.next().then(function (result) {
                print("without:first:" + result.value + ":" + result.done);
            });
            first.next().then(function (result) {
                print("without:second:" + result.value + ":" + result.done);
            });
            let second = withValue();
            second.next().then(function (result) {
                print("with:first:" + result.value + ":" + result.done);
            });
            second.next().then(function (result) {
                print("with:second:" + result.value + ":" + result.done);
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator yield statements should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "without:first:undefined:false".to_string(),
            "without:second:undefined:true".to_string(),
            "with:first:1:false".to_string(),
            "with:second:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_resumes_the_selected_yield_branch_without_retesting() {
    for (flag, expected_lines) in [
        ("true", ["yield:11:false", "return:13:true:1"]),
        ("false", ["yield:23:false", "return:27:true:1"]),
    ] {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let source = format!(
            r#"
                    let conditionCalls = 0;
                    function select(value) {{
                        conditionCalls += 1;
                        return value;
                    }}
                    async function* choose(flag) {{
                        var value = flag ? 10 : 20;
                        if (select(flag)) {{
                            value += 1;
                            yield value;
                            value += 2;
                        }} else {{
                            value += 3;
                            yield value;
                            value += 4;
                        }}
                        return value;
                    }}
                    let iterator = choose({flag});
                    iterator.next().then(function (result) {{
                        print("yield:" + result.value + ":" + result.done);
                    }});
                    iterator.next().then(function (result) {{
                        print("return:" + result.value + ":" + result.done + ":" + conditionCalls);
                    }});
                "#
        );
        engine_with_captured_prints(Arc::clone(&lines))
            .run_script(
                &source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .unwrap_or_else(|err| {
                panic!("async-generator conditional yield should run for {flag}: {err:?}")
            });
        assert_eq!(
            lines.lock().expect("capture mutex poisoned").as_slice(),
            expected_lines
        );
    }
}

#[test]
fn wasm_backend_async_generator_yield_assimilates_resolve_first_thenables_once() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let getterCalls = 0;
            let thenCalls = 0;
            let late = {};
            let thenable = {};
            Object.defineProperty(thenable, "then", {
                get: function () {
                    getterCalls += 1;
                    return function (resolve, reject) {
                        thenCalls += 1;
                        resolve("first");
                        reject(late);
                        resolve("last");
                        throw late;
                    };
                }
            });
            async function* stream() { yield thenable; }
            stream().next().then(function (result) {
                print("result:" + result.value + ":" + result.done + ":" + getterCalls + ":" + thenCalls);
            }, function () {
                print("unexpected rejection");
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("resolve-first thenable yield should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync".to_string(), "result:first:false:1:1".to_string(),]
    );
}

#[test]
fn wasm_backend_async_generator_yield_assimilates_reject_first_thenables_once() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let reason = {};
            let late = {};
            let thenable = {
                then: function (resolve, reject) {
                    reject(reason);
                    resolve("late");
                    reject(late);
                    throw late;
                }
            };
            let iterator = (async function*() { yield thenable; }());
            iterator.next().then(function () {
                print("unexpected fulfillment");
            }, function (rejected) {
                print("rejected:" + (rejected === reason));
                iterator.next().then(function (result) {
                    print("closed:" + result.value + ":" + result.done);
                });
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("reject-first thenable yield should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync".to_string(),
            "rejected:true".to_string(),
            "closed:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_yield_await_stages_the_resolved_value() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let thenable = {
                then(resolve) {
                    resolve("ready");
                }
            };
            async function* stream(value) { yield await value; }
            let iterator = stream(thenable);
            iterator.next().then(function (result) {
                print("first:" + result.value + ":" + result.done);
            });
            iterator.next().then(function (result) {
                print("second:" + result.value + ":" + result.done);
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator yield-await should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "first:ready:false".to_string(),
            "second:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_resumes_alternating_awaits_and_yields() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            async function* stream(first, second) {
                await first;
                yield "first";
                await second;
                yield "second";
                return "done";
            }
            let second = {
                then(resolve) {
                    resolve("second");
                }
            };
            let iterator = stream(Promise.resolve("first"), second);
            iterator.next()
                .then(function (result) {
                    print(result.value + ":" + result.done);
                    return iterator.next();
                })
                .then(function (result) {
                    print(result.value + ":" + result.done);
                    return iterator.next();
                })
                .then(function (result) {
                    print(result.value + ":" + result.done);
                });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("async-generator alternating suspensions should execute: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "first:false".to_string(),
            "second:false".to_string(),
            "done:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_await_loop_interleaves_and_retains_iteration_binding() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            const events = [];
            async function pushAwait(iteration) {
                events.push("await:" + iteration);
            }
            async function* callAsync() {
                for (let i = 0; i < 2; i++) {
                    await pushAwait(i);
                }
                return 0;
            }
            callAsync().next();
            new Promise(function (resolve) {
                events.push("promise:1");
                resolve();
            }).then(function () {
                events.push("promise:2");
            }).then(function () {
                print(events.join("|"));
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("async-generator Await loop should reuse its suspension state: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["await:0|promise:1|await:1|promise:2".to_string()]
    );
}

#[test]
fn wasm_backend_async_generator_yield_star_uses_null_async_method_sync_fallback() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let events = [];
            let delegate = {
                get [Symbol.asyncIterator]() {
                    events.push("get async");
                    return null;
                },
                get [Symbol.iterator]() {
                    events.push("get sync");
                    return function () {
                        events.push("call sync:" + (this === delegate) + ":" + arguments.length);
                        return {
                            get next() {
                                events.push("get next");
                                return function () {
                                    events.push("call next");
                                    return { value: "value", done: false };
                                };
                            }
                        };
                    };
                }
            };
            async function* outer() { yield* delegate; }
            outer().next().then(function (result) {
                events.push("result:" + result.value + ":" + result.done);
                print(events.join("|"));
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator sync fallback should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["get async|get sync|call sync:true:0|get next|call next|result:value:false".to_string()]
    );
}

#[test]
fn wasm_backend_async_generator_yield_star_rejects_sync_iterator_acquisition_failures() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let getReason = {};
            let callReason = {};
            let syncGetterCalls = 0;
            let getterFailure = {
                get [Symbol.asyncIterator]() { throw getReason; },
                get [Symbol.iterator]() {
                    syncGetterCalls += 1;
                    return function () { return {}; };
                }
            };
            let callFailure = {
                [Symbol.iterator]: function () { throw callReason; }
            };
            let resultFailure = {
                [Symbol.iterator]: function () { return 1; }
            };
            function rejectionFrom(delegate) {
                return (async function* () { yield* delegate; })().next();
            }
            rejectionFrom(getterFailure).then(undefined, function (error) {
                print("get:" + (error === getReason) + ":" + syncGetterCalls);
                return rejectionFrom(callFailure);
            }).then(undefined, function (error) {
                print("call:" + (error === callReason));
                return rejectionFrom(resultFailure);
            }).then(undefined, function (error) {
                print("result:" + (error.constructor === TypeError));
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("async-generator sync acquisition failures should reject: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "get:true:0".to_string(),
            "call:true".to_string(),
            "result:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_yield_star_delegates_each_result() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            async function* inner() { yield 1; }
            async function* outer() { yield* inner(); }
            let iterator = outer();
            iterator.next().then(function (result) {
                print("first:" + result.value + ":" + result.done);
            });
            iterator.next().then(function (result) {
                print("second:" + result.value + ":" + result.done);
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator delegation should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "first:1:false".to_string(),
            "second:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_yield_star_forwards_next_and_assigns_completion() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let log = [];
            let calls = 0;
            let delegate = {
                get [Symbol.asyncIterator]() {
                    log.push("get async iterator");
                    return function () {
                        let passedArguments = [...arguments];
                        log.push("call async iterator:" + (this === delegate) + ":" + passedArguments.length);
                        return this;
                    };
                },
                get next() {
                    log.push("get next:" + (this === delegate));
                    return function (value) {
                        let passedArguments = [...arguments];
                        calls += 1;
                        log.push("call next:" + (this === delegate) + ":" + passedArguments.length + ":" + passedArguments[0]);
                        let call = calls;
                        return {
                            get then() {
                                log.push("get then:" + call);
                                return function (resolve, reject) {
                                    let passedArguments = [...arguments];
                                    log.push("call then:" + call + ":" + (typeof passedArguments[0]) + ":" + (typeof passedArguments[1]));
                                    resolve({
                                        get done() {
                                            log.push("get done:" + call);
                                            return call !== 1;
                                        },
                                        get value() {
                                            log.push("get value:" + call);
                                            return call === 1 ? "first" : "complete";
                                        }
                                    });
                                };
                            }
                        };
                    };
                }
            };
            async function* outer() {
                log.push("before yield star");
                var completion = yield* delegate;
                log.push("after yield star:" + completion);
                return completion;
            }
            let iterator = outer();
            iterator.next("ignored").then(result => {
                print("first:" + result.value + ":" + result.done + "|" + log.join(","));
                return iterator.next("forwarded");
            }).then(result => {
                print("second:" + result.value + ":" + result.done + "|" + log.join(","));
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator delegated next should run: {err:?}"));
    assert_eq!(
            lines.lock().expect("capture mutex poisoned").as_slice(),
            &[
                "first:first:false|before yield star,get async iterator,call async iterator:true:0,get next:true,call next:true:1:undefined,get then:1,call then:1:function:function,get done:1,get value:1".to_string(),
                "second:complete:true|before yield star,get async iterator,call async iterator:true:0,get next:true,call next:true:1:undefined,get then:1,call then:1:function:function,get done:1,get value:1,call next:true:1:forwarded,get then:2,call then:2:function:function,get done:2,get value:2,after yield star:complete".to_string(),
            ]
        );
}

#[test]
fn wasm_backend_async_generator_yield_star_forwards_return_to_sync_iterators() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let returnCalls = 0;
            let delegate = {
                [Symbol.iterator]: function () { return this; },
                next: function () { return { value: "first", done: false }; },
                return: function (value) {
                    returnCalls += 1;
                    print("delegate:return:" + value);
                    return returnCalls === 1
                        ? { value: "continue", done: false }
                        : { value: "closed", done: true };
                }
            };
            async function* outer() { yield* delegate; }
            let iterator = outer();
            iterator.next().then(function (result) {
                print("first:" + result.value + ":" + result.done);
                return iterator.return("one");
            }).then(function (result) {
                print("second:" + result.value + ":" + result.done);
                return iterator.return("two");
            }).then(function (result) {
                print("third:" + result.value + ":" + result.done);
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator delegated return should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "first:first:false".to_string(),
            "delegate:return:one".to_string(),
            "second:continue:false".to_string(),
            "delegate:return:two".to_string(),
            "third:closed:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_catches_abrupt_delegated_return_value() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            const token = {};
            const delegate = {
                [Symbol.asyncIterator]() { return this; },
                next() { return { value: "open", done: false }; },
                return() {
                    return {
                        done: false,
                        get value() { throw token; }
                    };
                }
            };
            async function* outer() {
                let caught;
                try {
                    yield* delegate;
                } catch (error) {
                    caught = error;
                }
                return caught;
            }
            const iterator = outer();
            iterator.next()
                .then(function () { return iterator.return(); })
                .then(function (result) {
                    print((result.value === token) + ":" + result.done);
                });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("async-generator delegated return catch should execute: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["true:true".to_string()]
    );
}

#[test]
fn wasm_backend_async_generator_finally_preserves_return_across_yield() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            async function* values() {
                try {
                    yield "open";
                } finally {
                    yield "cleanup";
                }
            }
            const iterator = values();
            iterator.next()
                .then(function (result) {
                    print(result.value + ":" + result.done);
                    return iterator.return("closed");
                })
                .then(function (result) {
                    print(result.value + ":" + result.done);
                    return iterator.next();
                })
                .then(function (result) {
                    print(result.value + ":" + result.done);
                });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator finalizer should preserve return: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "open:false".to_string(),
            "cleanup:false".to_string(),
            "closed:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_finally_throw_overrides_return() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            var overrideReason = new Error("override");
            var values = async function*() {
                try {
                    yield "open";
                } finally {
                    throw overrideReason;
                    throw new Error("unreachable");
                }
            };
            const iterator = values();
            iterator.next()
                .then(function () {
                    return iterator.return("closed");
                })
                .then(function () {
                    print("fulfilled");
                }, function (reason) {
                    print("rejected:" + (reason === overrideReason));
                    print("message:" + reason.message);
                    return iterator.next();
                })
                .then(function (result) {
                    print("closed:" + result.value + ":" + result.done);
                });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator finalizer throw should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "rejected:true".to_string(),
            "message:override".to_string(),
            "closed:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_catches_return_await_errors() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            const token = new Error("broken promise");
            let caught;
            const brokenPromise = Promise.resolve(42);
            Object.defineProperty(brokenPromise, "constructor", {
                get() {
                    throw token;
                }
            });
            const values = async function*() {
                try {
                    yield "open";
                } catch (error) {
                    caught = error;
                    return "caught";
                }
            };
            const iterator = values();
            iterator.next()
                .then(function () {
                    return iterator.return(brokenPromise);
                })
                .then(function (result) {
                    print("caught:" + (caught === token));
                    print("result:" + result.value + ":" + result.done);
                }, function (reason) {
                    print("rejected:" + (reason === token));
                });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator return-await error should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["caught:true".to_string(), "result:caught:true".to_string()]
    );
}

#[test]
fn wasm_backend_async_generator_catch_finally_resumes_each_clause() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            const token = {};
            async function* values() {
                try {
                    yield Promise.reject(token);
                } catch (error) {
                    yield error;
                } finally {
                    yield "cleanup";
                }
            }
            const iterator = values();
            iterator.next()
                .then(function (result) {
                    print((result.value === token) + ":" + result.done);
                    return iterator.next();
                })
                .then(function (result) {
                    print(result.value + ":" + result.done);
                    return iterator.next();
                })
                .then(function (result) {
                    print(result.value + ":" + result.done);
                });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator catch/finally should resume: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "true:false".to_string(),
            "cleanup:false".to_string(),
            "undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_yield_star_awaits_return_before_delegate_return_lookup() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let events = [];
            let delegate = {
                [Symbol.asyncIterator]: function () { return this; },
                next: function () { return { done: false }; },
                get return() {
                    events.push("get return");
                    return undefined;
                }
            };
            async function* outer() {
                events.push("start");
                yield* delegate;
            }
            Promise.resolve(0)
                .then(function () { events.push("tick 1"); })
                .then(function () { events.push("tick 2"); })
                .then(function () { events.push("tick 3"); })
                .then(function () { print(events.join("|")); });
            let iterator = outer();
            iterator.next();
            iterator.return({
                get then() {
                    events.push("get then");
                    return undefined;
                }
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator return resumption should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["start|tick 1|get then|tick 2|get return|get then|tick 3".to_string()]
    );
}

#[test]
fn wasm_backend_async_generator_yield_awaits_return_before_next_promise_tick() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let events = [];
            async function* outer() {
                events.push("start");
                yield 123;
            }
            Promise.resolve(0)
                .then(function () { events.push("tick 1"); })
                .then(function () { events.push("tick 2"); })
                .then(function () { print(events.join("|")); });
            let iterator = outer();
            iterator.next();
            iterator.return({
                get then() {
                    events.push("get then");
                    return undefined;
                }
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator return resumption should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["start|tick 1|get then|tick 2".to_string()]
    );
}

#[test]
fn wasm_backend_async_generator_yield_star_forwards_throw_to_sync_iterators() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let throwCalls = 0;
            let delegate = {
                [Symbol.iterator]: function () { return this; },
                next: function () { return { value: "first", done: false }; },
                throw: function (value) {
                    throwCalls += 1;
                    print("delegate:throw:" + value);
                    return throwCalls === 1
                        ? { value: "continue", done: false }
                        : { value: "complete", done: true };
                }
            };
            async function* outer() {
                var completion = yield* delegate;
                return "outer:" + completion;
            }
            let iterator = outer();
            iterator.next().then(function (result) {
                print("first:" + result.value + ":" + result.done);
                return iterator.throw("one");
            }).then(function (result) {
                print("second:" + result.value + ":" + result.done);
                return iterator.throw("two");
            }).then(function (result) {
                print("third:" + result.value + ":" + result.done);
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator delegated throw should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "first:first:false".to_string(),
            "delegate:throw:one".to_string(),
            "second:continue:false".to_string(),
            "delegate:throw:two".to_string(),
            "third:outer:complete:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_yield_star_closes_before_missing_throw_type_error() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let delegate = {
                [Symbol.asyncIterator]: function () { return this; },
                next: function () { return { value: "first", done: false }; },
                get throw() {
                    print("get throw");
                    return null;
                },
                get return() {
                    print("get return");
                    return function () {
                        print("call return");
                        return {
                            then: function (resolve) {
                                print("await return");
                                resolve({ value: "closed", done: true });
                            }
                        };
                    };
                }
            };
            async function* outer() { yield* delegate; }
            let iterator = outer();
            iterator.next().then(function () {
                return iterator.throw("reason");
            }).then(function () {
                print("unexpected fulfillment");
            }, function (error) {
                print("rejected:" + (error.constructor === TypeError));
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator delegated close should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "get throw".to_string(),
            "get return".to_string(),
            "call return".to_string(),
            "await return".to_string(),
            "rejected:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_rejected_yield_closes_the_generator() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let reason = {};
            async function* stream() {
                yield Promise.reject(reason);
                yield "unreachable";
            }
            let iterator = stream();
            iterator.next().then(function () {
                print("unexpected");
            }, function (rejected) {
                print("rejected:" + (rejected === reason));
                iterator.next().then(function (result) {
                    print("closed:" + result.value + ":" + result.done);
                });
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator rejected yield should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "rejected:true".to_string(),
            "closed:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_for_await_forwards_rejected_yields() {
    for source_kind in ["async", "sync"] {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let source = if source_kind == "async" {
            r#"
                    let reason = {};
                    async function* source() { yield Promise.reject(reason); }
                    async function* stream() {
                        for await (let value of source()) { yield value; }
                    }
                    let iterator = stream();
                    iterator.next().then(function (result) {
                        print("unexpected:" + result.value + ":" + result.done);
                    }, function (rejected) {
                        print("rejected:" + (rejected === reason));
                        iterator.next().then(function (result) {
                            print("closed:" + result.value + ":" + result.done);
                        });
                    });
                "#
        } else {
            r#"
                    let reason = {};
                    let iterable = [Promise.reject(reason)];
                    async function* stream() {
                        for await (let value of iterable) { yield value; }
                    }
                    let iterator = stream();
                    iterator.next().then(function (result) {
                        print("unexpected:" + result.value + ":" + result.done);
                    }, function (rejected) {
                        print("rejected:" + (rejected === reason));
                        iterator.next().then(function (result) {
                            print("closed:" + result.value + ":" + result.done);
                        });
                    });
                "#
        };
        engine_with_captured_prints(Arc::clone(&lines))
            .run_script(
                source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .unwrap_or_else(|err| {
                panic!("async-generator for-await {source_kind} rejection should run: {err:?}")
            });
        assert_eq!(
            lines.lock().expect("capture mutex poisoned").as_slice(),
            &[
                "rejected:true".to_string(),
                "closed:undefined:true".to_string(),
            ],
            "{source_kind} iterator"
        );
    }
}

#[test]
fn wasm_backend_async_generator_sync_yield_star_awaits_rejected_values() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let reason = {};
            let iterable = [Promise.reject(reason)];
            async function* stream() { yield* iterable; }
            let iterator = stream();
            iterator.next().then(function (result) {
                print("unexpected:" + result.value + ":" + result.done);
            }, function (rejected) {
                print("rejected:" + (rejected === reason));
                iterator.next().then(function (result) {
                    print("closed:" + result.value + ":" + result.done);
                });
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("async-generator sync yield-star rejection should run: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "rejected:true".to_string(),
            "closed:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_for_await_does_not_forward_next_values() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let received;
            async function* source() {
                received = yield 1;
                print("source:" + received);
            }
            async function* stream() {
                for await (let value of source()) { yield value; }
            }
            let iterator = stream();
            iterator.next().then(function (result) {
                print("first:" + result.value + ":" + result.done);
                return iterator.next(99);
            }).then(function (result) {
                print("second:" + result.value + ":" + result.done);
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("async-generator for-await next should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "first:1:false".to_string(),
            "source:undefined".to_string(),
            "second:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_nested_yield_resumes_each_operand() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            async function* stream() { yield yield 1; }
            let iterator = stream();
            iterator.next().then(function (result) {
                print("first:" + result.value + ":" + result.done);
            });
            iterator.next().then(function (result) {
                print("second:" + result.value + ":" + result.done);
            });
            iterator.next().then(function (result) {
                print("third:" + result.value + ":" + result.done);
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("nested async-generator yields should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "first:1:false".to_string(),
            "second:undefined:false".to_string(),
            "third:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_newline_terminates_yield_operand() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            async function* stream() {
                yield
                1;
            }
            let iterator = stream();
            iterator.next().then(function (result) {
                print("first:" + result.value + ":" + result.done);
            });
            iterator.next().then(function (result) {
                print("second:" + result.value + ":" + result.done);
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("newline-terminated async-generator yield should run: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "first:undefined:false".to_string(),
            "second:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_return_closes_suspended_yield() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let resumed = false;
            async function* stream() {
                yield 1;
                resumed = true;
            }
            let iterator = stream();
            iterator.next().then(function () {
                iterator.return("sent-value").then(function (result) {
                    print("return:" + result.value + ":" + result.done + ":" + resumed);
                    iterator.next().then(function (closed) {
                        print("closed:" + closed.value + ":" + closed.done);
                    });
                });
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("return from suspended async-generator yield should run: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "return:sent-value:true:false".to_string(),
            "closed:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_return_awaits_value_from_suspended_yield() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let resumed = false;
            let release;
            let returned = new Promise(function (resolve) { release = resolve; });
            async function* stream() {
                yield 1;
                resumed = true;
            }
            let iterator = stream();
            iterator.next().then(function () {
                iterator.return(returned).then(function (result) {
                    print("return:" + result.value + ":" + result.done + ":" + resumed);
                    iterator.next().then(function (closed) {
                        print("closed:" + closed.value + ":" + closed.done);
                    });
                });
                release("unwrapped-value");
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("promised return from suspended async-generator yield should run: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "return:unwrapped-value:true:false".to_string(),
            "closed:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_throw_rejects_suspended_yield_by_identity() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let resumed = false;
            let reason = {};
            async function* stream() {
                yield 1;
                resumed = true;
            }
            let iterator = stream();
            iterator.next().then(function () {
                iterator.throw(reason).then(undefined, function (error) {
                    print("throw:" + (error === reason) + ":" + resumed);
                    iterator.next().then(function (closed) {
                        print("closed:" + closed.value + ":" + closed.done);
                    });
                });
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("throw into suspended async-generator yield should run: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "throw:true:false".to_string(),
            "closed:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_async_generator_throw_does_not_await_promise_reason() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let resumed = false;
            let reason = new Promise(function () {});
            async function* stream() {
                yield 1;
                resumed = true;
            }
            let iterator = stream();
            iterator.next().then(function () {
                iterator.throw(reason).then(undefined, function (error) {
                    print("throw:" + (error === reason) + ":" + resumed);
                    iterator.next().then(function (closed) {
                        print("closed:" + closed.value + ":" + closed.done);
                    });
                });
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("promise throw into suspended async-generator yield should run: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "throw:true:false".to_string(),
            "closed:undefined:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_promise_fulfillment_chains_through_default_and_callable_reactions() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            new Promise(function (resolve) { resolve(7); })
                .then()
                .then(function (value) { return value + 1; })
                .then(function (value) { print("value:" + value); });
            let rejected = new Promise(function (_, reject) { reject("bad"); });
            let propagated = rejected.then();
            propagated.then(undefined, function (reason) { print("reason:" + reason); });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("fulfilled Promise chain should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync".to_string(),
            "reason:bad".to_string(),
            "value:8".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_promise_thrown_reaction_rejects_the_chained_promise() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            new Promise(function (_, reject) { reject("bad"); })
                .then(undefined, function (reason) { throw reason + "!"; })
                .then(undefined, function (reason) { print("caught:" + reason); });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("thrown Promise reaction should reject: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["caught:bad!".to_string()]
    );
}

#[test]
fn wasm_backend_promise_catch_invokes_observable_then_once() {
    let source = r#"
            let catchMethod = Promise.prototype.catch;
            let receiver = {};
            let onRejected = function () {};
            let returnValue = {};
            let getCount = 0;
            let callCount = 0;
            let callWasExact = false;
            Object.defineProperty(receiver, "then", {
                get: function () {
                    getCount += 1;
                    return function (onFulfilled, rejectionHandler) {
                        callCount += 1;
                        callWasExact = this === receiver
                            && arguments.length === 2
                            && onFulfilled === undefined
                            && rejectionHandler === onRejected;
                        return returnValue;
                    };
                }
            });
            catchMethod.call(receiver, onRejected) === returnValue
                && getCount === 1
                && callCount === 1
                && callWasExact;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise catch observable invocation should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_catch_chains_rejections_asynchronously() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let promise = Promise.reject("bad");
            let chained = promise.catch(function (reason) {
                print("caught:" + reason);
                return 12;
            });
            chained.then(function (value) { print("value:" + value); });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise catch rejection chain should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync".to_string(),
            "caught:bad".to_string(),
            "value:12".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_promise_finally_preserves_settlement_and_awaits_cleanup() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            Promise.resolve("value")
                .finally(function () {
                    print("cleanup:value");
                    return Promise.resolve("ignored");
                })
                .then(function (value) { print("fulfilled:" + value); });
            Promise.reject("reason")
                .finally(function () { print("cleanup:reason"); })
                .catch(function (reason) { print("rejected:" + reason); });
            Promise.resolve("value")
                .finally(function () { throw "replacement"; })
                .catch(function (reason) { print("replacement:" + reason); });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise finally settlement should run: {err:?}"));
    let lines = lines.lock().expect("capture mutex poisoned");
    assert_eq!(lines.first(), Some(&"sync".to_string()));
    for expected in [
        "cleanup:value",
        "cleanup:reason",
        "replacement:replacement",
        "rejected:reason",
        "fulfilled:value",
    ] {
        assert!(
            lines.iter().any(|line| line == expected),
            "missing {expected:?}: {lines:?}"
        );
    }
}

#[test]
fn wasm_backend_promise_finally_invokes_observable_then() {
    let source = r#"
            let finallyMethod = Promise.prototype.finally;
            let receiver = { constructor: Promise };
            let onFinally = function () {};
            let returnValue = {};
            let thenFinally;
            let catchFinally;
            receiver.then = function (first, second) {
                thenFinally = first;
                catchFinally = second;
                return returnValue;
            };
            let callableResult = finallyMethod.call(receiver, onFinally);
            let marker = {};
            let nonCallableResult = finallyMethod.call({
                constructor: Promise,
                then: function (first, second) {
                    return first === marker && second === marker;
                }
            }, marker);
            callableResult === returnValue
                && thenFinally !== onFinally
                && catchFinally !== onFinally
                && typeof thenFinally === "function"
                && typeof catchFinally === "function"
                && nonCallableResult === true;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise finally observable invocation should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_then_uses_generic_custom_capabilities() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let returnedPromise = {};
            let constructorCalls = 0;
            function CapabilityConstructor(executor) {
                constructorCalls += 1;
                executor(
                    function (value) {
                        "use strict";
                        print("resolve:" + (this === undefined) + ":" + arguments.length + ":" + value);
                    },
                    function (reason) {
                        "use strict";
                        print("reject:" + (this === undefined) + ":" + arguments.length + ":" + (reason === expected));
                    }
                );
                return returnedPromise;
            }
            let constructor = {};
            constructor[Symbol.species] = CapabilityConstructor;
            let expected = {};

            let fulfilled = Promise.resolve(4);
            fulfilled.constructor = constructor;
            let fulfilledResult = fulfilled.then(function (value) { return value + 3; });

            let rejected = Promise.resolve(1);
            rejected.constructor = constructor;
            let rejectedResult = rejected.then(function () { throw expected; });

            print("returned:" + (fulfilledResult === returnedPromise));
            print("returned:" + (rejectedResult === returnedPromise));
            print("constructors:" + constructorCalls);
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("generic Promise capabilities should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "returned:true".to_string(),
            "returned:true".to_string(),
            "constructors:2".to_string(),
            "sync".to_string(),
            "resolve:true:1:7".to_string(),
            "reject:true:1:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_promise_then_rejects_invalid_capability_initialization() {
    let source = r#"
            let missingConstructor = {};
            missingConstructor[Symbol.species] = function () { return {}; };
            let missing = new Promise(function () {});
            missing.constructor = missingConstructor;

            let repeatedConstructor = {};
            repeatedConstructor[Symbol.species] = function (executor) {
                executor(function () {}, function () {});
                executor(function () {}, function () {});
                return {};
            };
            let repeated = new Promise(function () {});
            repeated.constructor = repeatedConstructor;

            let missingThrows = false;
            let repeatedThrows = false;
            try { missing.then(); } catch (error) {
                missingThrows = error instanceof TypeError;
            }
            try { repeated.then(); } catch (error) {
                repeatedThrows = error instanceof TypeError;
            }
            missingThrows && repeatedThrows;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("invalid Promise capabilities should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_resolve_validates_receiver_before_identity_shortcut() {
    let source = r#"
            let promise = new Promise(function () {});
            let receivers = [undefined, null, true, 1, "", Symbol()];
            let allThrow = true;
            for (let receiver of receivers) {
                promise.constructor = receiver;
                try {
                    Promise.resolve.call(receiver, promise);
                    allThrow = false;
                } catch (error) {
                    allThrow = allThrow && error instanceof TypeError;
                }
            }
            allThrow;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise.resolve receiver validation should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_with_resolvers_returns_standard_ordered_record() {
    let source = r#"
            let capability = Promise.withResolvers();
            let keys = Object.keys(capability);
            let promiseDescriptor = Object.getOwnPropertyDescriptor(capability, "promise");
            let resolveDescriptor = Object.getOwnPropertyDescriptor(capability, "resolve");
            let rejectDescriptor = Object.getOwnPropertyDescriptor(capability, "reject");
            let methodDescriptor = Object.getOwnPropertyDescriptor(Promise, "withResolvers");
            Object.getPrototypeOf(capability) === Object.prototype
                && keys.length === 3
                && keys[0] === "promise"
                && keys[1] === "resolve"
                && keys[2] === "reject"
                && promiseDescriptor.writable === true
                && promiseDescriptor.enumerable === true
                && promiseDescriptor.configurable === true
                && resolveDescriptor.writable === true
                && resolveDescriptor.enumerable === true
                && resolveDescriptor.configurable === true
                && rejectDescriptor.writable === true
                && rejectDescriptor.enumerable === true
                && rejectDescriptor.configurable === true
                && methodDescriptor.writable === true
                && methodDescriptor.enumerable === false
                && methodDescriptor.configurable === true
                && Promise.withResolvers.name === "withResolvers"
                && Promise.withResolvers.length === 0;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise.withResolvers ordered record should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_with_resolvers_rejects_invalid_receivers() {
    let source = r#"
            let receivers = [undefined, null, 86, "string", true, Symbol(), () => {}];
            let allThrow = true;
            for (let receiver of receivers) {
                try {
                    Promise.withResolvers.call(receiver);
                    allThrow = false;
                } catch (error) {
                    allThrow = allThrow && error instanceof TypeError;
                }
            }
            allThrow;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("Promise.withResolvers invalid receivers should run: {err:?}")
        });
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_try_forwards_arguments_with_undefined_this() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            Promise.try(function (first, second, third) {
                "use strict";
                let received = Array.prototype.slice.call(arguments);
                print("callback:" + (this === undefined) + ":" + received.length + ":" + received[0] + ":" + received[1] + ":" + received[2]);
                return first + second + third;
            }, 2, 3, 4).then(function (value) { print("resolved:" + value); });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise.try argument forwarding should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "callback:true:3:2:3:4".to_string(),
            "sync".to_string(),
            "resolved:9".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_promise_try_rejects_callback_abrupt_completions() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let marker = {};
            Promise.try(function () { throw marker; }).catch(function (reason) {
                print("thrown:" + (reason === marker));
            });
            Promise.try(null).catch(function (reason) {
                print("invalid:" + (reason instanceof TypeError));
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("Promise.try abrupt completion rejection should run: {err:?}")
        });
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync".to_string(),
            "thrown:true".to_string(),
            "invalid:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_promise_try_validates_receiver_before_callback() {
    let source = r#"
            let callbackCalls = 0;
            let callback = function () { callbackCalls += 1; };
            let receivers = [undefined, null, true, 1, "", Symbol(), {}, () => {}];
            let allThrow = true;
            for (let receiver of receivers) {
                try {
                    Promise.try.call(receiver, callback);
                    allThrow = false;
                } catch (error) {
                    allThrow = allThrow && error instanceof TypeError;
                }
            }
            // Only primitive receivers fail before Call. Both object receivers
            // reach the callback, then fail constructor validation.
            allThrow && callbackCalls === 2;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise.try receiver validation should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_static_settlement_schedules_reactions_after_synchronous_code() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            Promise.resolve(5).then(function (value) { print("resolved:" + value); });
            Promise.reject("bad").then(undefined, function (reason) {
                print("rejected:" + reason);
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise static settlement should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync".to_string(),
            "resolved:5".to_string(),
            "rejected:bad".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_promise_all_consumes_iterables_and_preserves_input_order() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let iterable = {};
            iterable[Symbol.iterator] = function () {
                let values = [Promise.resolve("first"), "second"];
                let index = 0;
                return {
                    next: function () {
                        if (index === values.length) return { done: true };
                        return { done: false, value: values[index++] };
                    }
                };
            };
            print("sync");
            Promise.all(iterable).then(function (values) {
                print(values.join(","));
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise.all iterable should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync".to_string(), "first,second".to_string()]
    );
}

#[test]
fn wasm_backend_promise_all_returns_a_derived_promise_instance() {
    let source = r#"
            class SubPromise extends Promise {}
            let instance = Promise.all.call(SubPromise, []);
            instance.constructor === SubPromise && instance instanceof SubPromise;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("derived Promise.all result should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_all_settled_uses_one_resolve_lookup_and_a_shared_element_guard() {
    let source = r#"
            let capabilityObject = {};
            let resolved;
            let rejected;
            let resolveGets = 0;
            let resolveCalls = 0;
            function CustomPromise(executor) {
                executor(
                    function (value) { resolved = value; },
                    function (reason) { rejected = reason; }
                );
                return capabilityObject;
            }
            Object.defineProperty(CustomPromise, "resolve", {
                configurable: true,
                get: function () {
                    resolveGets += 1;
                    return function (value) {
                        resolveCalls += 1;
                        return {
                            then: function (onFulfilled, onRejected) {
                                onFulfilled(value);
                                onRejected("late rejection");
                                onFulfilled("late fulfillment");
                            }
                        };
                    };
                }
            });
            let result = Promise.allSettled.call(CustomPromise, [1, 2]);
            result === capabilityObject
                && rejected === undefined
                && resolveGets === 1
                && resolveCalls === 2
                && resolved.length === 2
                && resolved[0].status === "fulfilled"
                && resolved[0].value === 1
                && resolved[1].status === "fulfilled"
                && resolved[1].value === 2;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("custom Promise.allSettled should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_all_settled_closes_only_iterators_left_open_by_abrupt_completion() {
    let source = r#"
            let resolveError = {};
            let nextError = {};
            let closedAfterResolveError = 0;
            let closedAfterNextError = 0;
            let rejectedAfterResolveError;
            let rejectedAfterNextError;
            function capabilityFor(setReason) {
                return function (executor) {
                    executor(function () {}, setReason);
                    return {};
                };
            }
            let ResolveThrowingPromise = capabilityFor(function (reason) {
                rejectedAfterResolveError = reason;
            });
            ResolveThrowingPromise.resolve = function () { throw resolveError; };
            let resolveThrowingIterable = {};
            resolveThrowingIterable[Symbol.iterator] = function () {
                return {
                    next: function () { return { done: false, value: 1 }; },
                    return: function () {
                        closedAfterResolveError += 1;
                        return {};
                    }
                };
            };
            Promise.allSettled.call(ResolveThrowingPromise, resolveThrowingIterable);

            let NextThrowingPromise = capabilityFor(function (reason) {
                rejectedAfterNextError = reason;
            });
            NextThrowingPromise.resolve = function (value) { return Promise.resolve(value); };
            let nextThrowingIterable = {};
            nextThrowingIterable[Symbol.iterator] = function () {
                return {
                    next: function () { throw nextError; },
                    return: function () {
                        closedAfterNextError += 1;
                        return {};
                    }
                };
            };
            Promise.allSettled.call(NextThrowingPromise, nextThrowingIterable);

            rejectedAfterResolveError === resolveError
                && rejectedAfterNextError === nextError
                && closedAfterResolveError === 1
                && closedAfterNextError === 0;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise.allSettled iterator closing should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_all_settled_returns_a_derived_empty_promise_without_species() {
    let source = r#"
            let speciesGets = 0;
            class SubPromise extends Promise {
                static get [Symbol.species]() {
                    speciesGets += 1;
                    throw new Error("species must not be read");
                }
            }
            let instance = Promise.allSettled.call(SubPromise, []);
            speciesGets === 0
                && instance.constructor === SubPromise
                && instance instanceof SubPromise;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("derived Promise.allSettled should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_all_settled_keyed_uses_one_resolve_lookup_and_shared_guards() {
    let source = r#"
            let capabilityObject = {};
            let resolved;
            let rejected;
            let resolveGets = 0;
            let resolveCalls = 0;
            function CustomPromise(executor) {
                executor(
                    function (value) { resolved = value; },
                    function (reason) { rejected = reason; }
                );
                return capabilityObject;
            }
            Object.defineProperty(CustomPromise, "resolve", {
                get: function () {
                    resolveGets += 1;
                    return function (value) {
                        resolveCalls += 1;
                        return {
                            then: function (onFulfilled, onRejected) {
                                if (value === "bad") {
                                    onRejected("failure");
                                    onFulfilled("late fulfillment");
                                } else {
                                    onFulfilled(value);
                                    onRejected("late rejection");
                                }
                            }
                        };
                    };
                }
            });
            let result = Promise.allSettledKeyed.call(CustomPromise, {
                first: "good",
                second: "bad"
            });
            result === capabilityObject
                && rejected === undefined
                && resolveGets === 1
                && resolveCalls === 2
                && Object.getPrototypeOf(resolved) === null
                && Object.keys(resolved).join(",") === "first,second"
                && resolved.first.status === "fulfilled"
                && resolved.first.value === "good"
                && resolved.second.status === "rejected"
                && resolved.second.reason === "failure";
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise.allSettledKeyed should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_all_keyed_rechecks_enumerability_and_rejects_abrupt_gets() {
    let source = r#"
            let error = {};
            let rejected;
            let events = [];
            function CustomPromise(executor) {
                executor(function () {}, function (reason) { rejected = reason; });
                return {};
            }
            CustomPromise.resolve = function (value) {
                events.push("resolve:" + value);
                return { then: function (onFulfilled) { onFulfilled(value); } };
            };
            let target = { skipped: 1, throwing: 2 };
            let promises = new Proxy(target, {
                ownKeys: function () {
                    events.push("ownKeys");
                    return ["skipped", "throwing"];
                },
                getOwnPropertyDescriptor: function (target, key) {
                    events.push("descriptor:" + key);
                    if (key === "skipped") return undefined;
                    return Object.getOwnPropertyDescriptor(target, key);
                },
                get: function (_, key) {
                    events.push("get:" + key);
                    throw error;
                }
            });
            Promise.allKeyed.call(CustomPromise, promises);
            rejected === error
                && events.join(",") ===
                    "ownKeys,descriptor:skipped,descriptor:throwing,get:throwing";
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise.allKeyed abrupt get should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_any_fulfills_first_and_aggregates_ordered_rejections() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            Promise.any([
                Promise.reject("ignored"),
                Promise.resolve("winner"),
                Promise.resolve("later")
            ]).then(function (value) { print("fulfilled:" + value); });
            Promise.any([
                Promise.reject("first"),
                Promise.reject("second")
            ]).catch(function (error) {
                let descriptor = Object.getOwnPropertyDescriptor(error, "errors");
                print(
                    "rejected:" + (error instanceof AggregateError) + ":" +
                    error.errors.join(",") + ":" +
                    descriptor.writable + ":" + descriptor.enumerable + ":" + descriptor.configurable
                );
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise.any settlement should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync".to_string(),
            "fulfilled:winner".to_string(),
            "rejected:true:first,second:true:false:true".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_promise_any_reject_element_is_fresh_and_idempotent() {
    let source = r#"
            let capabilityObject = {};
            let resolved;
            let rejected;
            let resolveGets = 0;
            let rejectFunctions = [];
            function CustomPromise(executor) {
                executor(
                    function (value) { resolved = value; },
                    function (reason) { rejected = reason; }
                );
                return capabilityObject;
            }
            Object.defineProperty(CustomPromise, "resolve", {
                get: function () {
                    resolveGets += 1;
                    return function (value) {
                        return {
                            then: function (_, onRejected) {
                                rejectFunctions.push(onRejected);
                                onRejected(value);
                                onRejected("late");
                            }
                        };
                    };
                }
            });
            let result = Promise.any.call(CustomPromise, ["first", "second"]);
            result === capabilityObject
                && resolved === undefined
                && rejected instanceof AggregateError
                && rejected.errors.join(",") === "first,second"
                && resolveGets === 1
                && rejectFunctions.length === 2
                && rejectFunctions[0] !== rejectFunctions[1];
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("custom Promise.any should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_any_rejects_empty_iterables_without_species() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let speciesGets = 0;
            class SubPromise extends Promise {
                static get [Symbol.species]() {
                    speciesGets += 1;
                    return Promise;
                }
            }
            let result = Promise.any.call(SubPromise, []);
            print(
                "result:" + (result instanceof SubPromise) + ":" +
                (result.constructor === SubPromise) + ":" + speciesGets
            );
            result.catch(function (error) {
                print("empty:" + (error instanceof AggregateError) + ":" + error.errors.length);
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("empty Promise.any should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["result:true:true:0".to_string(), "empty:true:0".to_string(),]
    );
}

#[test]
fn wasm_backend_promise_race_keeps_an_empty_iterable_pending() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            Promise.race([]).then(
                function () { print("settled"); },
                function () { print("settled"); }
            );
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("empty Promise.race should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync".to_string()]
    );
}

#[test]
fn wasm_backend_promise_race_preserves_first_settlement() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let resolveFirst;
            let rejectSecond;
            let first = new Promise(function (resolve) { resolveFirst = resolve; });
            let second = new Promise(function (_, reject) { rejectSecond = reject; });
            Promise.race([first, second]).then(
                function (value) { print("resolved:" + value); },
                function (reason) { print("rejected:" + reason); }
            );
            rejectSecond("second");
            resolveFirst("first");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise.race first settlement should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["rejected:second".to_string()]
    );
}

#[test]
fn wasm_backend_promise_static_methods_propagate_capability_errors() {
    let source = r#"
            let expected = new Error("capability");
            function ResolveThrows(executor) {
                return new Promise(function () {
                    executor(function () { throw expected; }, function () {});
                });
            }
            function RejectThrows(executor) {
                return new Promise(function () {
                    executor(function () {}, function () { throw expected; });
                });
            }
            let resolveError;
            let rejectError;
            try { Promise.resolve.call(ResolveThrows, 1); } catch (error) {
                resolveError = error;
            }
            try { Promise.reject.call(RejectThrows, 1); } catch (error) {
                rejectError = error;
            }
            resolveError === expected && rejectError === expected;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise capability errors should propagate: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_promise_thenable_jobs_are_asynchronous_and_settle_once() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let thenable = {
                then: function (resolve, reject) {
                    print("then");
                    resolve(7);
                    reject("late");
                    throw new Error("later");
                }
            };
            let promise = Promise.resolve(thenable);
            promise.then(
                function (value) { print("value:" + value); },
                function (reason) { print("reason:" + reason); }
            );
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Promise thenable job should run: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "sync".to_string(),
            "then".to_string(),
            "value:7".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_promise_resolution_rejects_abrupt_then_getters() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let expected = new Error("getter");
            let thenable = {};
            Object.defineProperty(thenable, "then", {
                get: function () { throw expected; }
            });
            let promise = Promise.resolve(thenable);
            promise.then(undefined, function (reason) {
                print(reason === expected);
            });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("abrupt then getter should reject: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["true".to_string()]
    );
}

#[test]
fn wasm_backend_promise_resolution_rejects_self_resolution() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let resolve;
            let promise = new Promise(function (onFulfilled) {
                resolve = onFulfilled;
            });
            promise.then(undefined, function (reason) {
                print(reason instanceof TypeError);
            });
            resolve(promise);
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("self-resolution should reject: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["true".to_string()]
    );
}

#[test]
fn wasm_backend_promise_reactions_assimilate_returned_thenables() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let promise = Promise.resolve(1);
            let chained = promise.then(function () {
                return { then: function (resolve) { resolve(9); } };
            });
            chained.then(function (value) { print("value:" + value); });
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("returned thenable should be assimilated: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["value:9".to_string()]
    );
}

#[test]
fn wasm_backend_resumes_zero_suspension_generators_lazily_and_once() {
    let source = r#"
            let ran = false;
            function* lazy() {
                ran = true;
                return 42;
            }
            const iterator = lazy();
            const lazyBeforeNext = !ran;
            const first = iterator.next();
            const second = iterator.next();
            lazyBeforeNext && ran
                && first.value === 42 && first.done === true
                && second.value === undefined && second.done === true
                && Object.getPrototypeOf(iterator) === lazy.prototype;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("zero-suspension generator should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_resumes_linear_generators_at_each_yield() {
    let source = r#"
            let effects = "";
            function* sequence() {
                yield 1;
                effects += "a";
                yield 2;
                effects += "b";
                return yield 3;
            }
            const firstIterator = sequence();
            const secondIterator = sequence();
            const first = firstIterator.next(99);
            const independent = secondIterator.next();
            const second = firstIterator.next(7);
            const third = firstIterator.next(8);
            const returned = firstIterator.next(9);
            const completed = firstIterator.next();
            function* activation(parameter) {
                let local = parameter;
                yield local;
                local += arguments[0];
                yield local;
                return local;
            }
            const activationIterator = activation(4);
            const activationFirst = activationIterator.next();
            const activationSecond = activationIterator.next();
            const activationReturn = activationIterator.next();
            let sent;
            function* injection() {
                sent = yield 5;
                yield sent;
            }
            const injectionIterator = injection();
            const injectionFirst = injectionIterator.next(100);
            const injectionSecond = injectionIterator.next(6);
            first.value === 1 && first.done === false
                && independent.value === 1 && independent.done === false
                && second.value === 2 && second.done === false
                && third.value === 3 && third.done === false
                && returned.value === 9 && returned.done === true
                && completed.value === undefined && completed.done === true
                && effects === "ab"
                && activationFirst.value === 4 && activationFirst.done === false
                && activationSecond.value === 8 && activationSecond.done === false
                && activationReturn.value === 8 && activationReturn.done === true
                && injectionFirst.value === 5 && injectionFirst.done === false
                && injectionSecond.value === 6 && injectionSecond.done === false
                && sent === 6;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("linear generator should resume: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_resumes_discarded_generator_expressions() {
    let source = r#"
            function* grouping() { (yield 1); }
            function* array() { [yield 2]; }
            function* block() { { yield 3; } }
            function* comma() { yield 4, yield 5; }
            function* conditional() { (yield 6) ? yield 7 : yield 8; }
            function* omitted() { [yield]; }

            const groupingIterator = grouping();
            const groupingYield = groupingIterator.next();
            const groupingDone = groupingIterator.next();
            const arrayIterator = array();
            const arrayYield = arrayIterator.next();
            const arrayDone = arrayIterator.next();
            const blockIterator = block();
            const blockYield = blockIterator.next();
            const blockDone = blockIterator.next();
            const commaIterator = comma();
            const commaFirst = commaIterator.next();
            const commaSecond = commaIterator.next(40);
            const commaDone = commaIterator.next(50);
            const truthyIterator = conditional();
            const truthyCondition = truthyIterator.next();
            const truthyBranch = truthyIterator.next(true);
            const truthyDone = truthyIterator.next();
            const falsyIterator = conditional();
            const falsyCondition = falsyIterator.next();
            const falsyBranch = falsyIterator.next(false);
            const falsyDone = falsyIterator.next();
            const omittedIterator = omitted();
            const omittedYield = omittedIterator.next();
            const omittedDone = omittedIterator.next();

            groupingYield.value === 1 && groupingYield.done === false
                && groupingDone.value === undefined && groupingDone.done === true
                && arrayYield.value === 2 && arrayYield.done === false
                && arrayDone.value === undefined && arrayDone.done === true
                && blockYield.value === 3 && blockYield.done === false
                && blockDone.value === undefined && blockDone.done === true
                && commaFirst.value === 4 && commaFirst.done === false
                && commaSecond.value === 5 && commaSecond.done === false
                && commaDone.value === undefined && commaDone.done === true
                && truthyCondition.value === 6 && truthyCondition.done === false
                && truthyBranch.value === 7 && truthyBranch.done === false
                && truthyDone.value === undefined && truthyDone.done === true
                && falsyCondition.value === 6 && falsyCondition.done === false
                && falsyBranch.value === 8 && falsyBranch.done === false
                && falsyDone.value === undefined && falsyDone.done === true
                && omittedYield.value === undefined && omittedYield.done === false
                && omittedDone.value === undefined && omittedDone.done === true;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("discarded generator expressions should resume: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_exposes_generator_expression_names() {
    let source = r#"
            const inferred = function* () {};
            const anonymous = [function* () {}][0];
            const explicit = function* named() {};
            const inferredDescriptor = Object.getOwnPropertyDescriptor(inferred, "name");
            const anonymousDescriptor = Object.getOwnPropertyDescriptor(anonymous, "name");
            const explicitDescriptor = Object.getOwnPropertyDescriptor(explicit, "name");

            inferredDescriptor.value === "inferred"
                && anonymousDescriptor.value === ""
                && explicitDescriptor.value === "named"
                && inferredDescriptor.writable === false
                && anonymousDescriptor.enumerable === false
                && explicitDescriptor.configurable === true;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("generator names should compile: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_initializes_generator_default_parameters_in_tdz_order() {
    let source = r#"
            const selfRead = function* (value = value) {};
            const laterRead = function* (value = later, later) {};
            const priorRead = function* (value = 7, later = value) { yield later; };
            const destructuredLaterRead = function* ([value = later, later]) {};
            const destructuredPriorRead = function* ([value = 11, later = value]) { yield later; };

            let selfReadThrows = false;
            let laterReadThrows = false;
            let destructuredLaterReadThrows = false;
            try { selfRead(); } catch (error) { selfReadThrows = error instanceof ReferenceError; }
            try { laterRead(); } catch (error) { laterReadThrows = error instanceof ReferenceError; }
            try { destructuredLaterRead([]); } catch (error) {
                destructuredLaterReadThrows = error instanceof ReferenceError;
            }
            const priorResult = priorRead().next();
            const destructuredPriorResult = destructuredPriorRead([]).next();

            selfReadThrows
                && laterReadThrows
                && destructuredLaterReadThrows
                && priorResult.value === 7
                && priorResult.done === false
                && destructuredPriorResult.value === 11
                && destructuredPriorResult.done === false;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("generator parameter TDZ should compile: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_honors_unscopables_for_generator_with_bindings() {
    let source = r#"
            globalThis.excluded = 9;
            globalThis.included = 10;
            globalThis.probePassed = false;
            globalThis[Symbol.unscopables] = { excluded: true };

            const probe = function* (input) {
                let observedHoistedVar;
                let observedExcluded;
                let observedIncluded;
                with (globalThis) {
                    observedHoistedVar = excluded;
                }
                var excluded = input;
                with (globalThis) {
                    observedExcluded = excluded;
                    observedIncluded = included;
                    excluded = 2;
                    included = 11;
                }
                globalThis.probePassed = observedHoistedVar === undefined
                    && observedExcluded === 1
                    && observedIncluded === 10
                    && excluded === 2
                    && globalThis.excluded === 9
                    && globalThis.included === 11;
            };

            probe(1).next();

            let count = 0;
            let observations = {};
            var v = 1;
            globalThis[Symbol.unscopables].v = true;
            count++;
            var callCount = 0;
            const activationProbe = function* (input) {
                count++;
                with (globalThis) {
                    count++;
                    observations.initial = v;
                }
                count++;
                var v = input;
                with (globalThis) {
                    count++;
                    observations.initialized = v;
                    v = 20;
                }
                observations.assigned = v;
                observations.global = globalThis.v;
                callCount++;
            };
            activationProbe(10).next();
            count++;

            globalThis.probePassed
                && observations.initial === undefined
                && observations.initialized === 10
                && observations.assigned === 20
                && observations.global === 1
                && callCount === 1
                && count === 6;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("generator unscopables should compile: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_enforces_named_generator_expression_binding_immutability() {
    for source in [
        r#"
                var readParameter, setParameter, readBody, setBody;
                var generator = function* binding(
                    unused = (
                        readParameter = function() { return binding; },
                        setParameter = function() { binding = null; }
                    )
                ) {
                    readBody = function() { return binding; };
                    setBody = function() { binding = null; };
                };
                generator().next();
                setParameter();
                setBody();
                readParameter() === generator && readBody() === generator;
            "#,
        r#"
                "use strict";
                var readParameter, setParameter, readBody, setBody;
                var generator = function* binding(
                    unused = (
                        readParameter = function() { return binding; },
                        setParameter = function() { binding = null; }
                    )
                ) {
                    readBody = function() { return binding; };
                    setBody = function() { binding = null; };
                };
                generator().next();
                var parameterThrew = false;
                var bodyThrew = false;
                try { setParameter(); } catch (error) {
                    parameterThrew = error instanceof TypeError;
                }
                try { setBody(); } catch (error) {
                    bodyThrew = error instanceof TypeError;
                }
                parameterThrew
                    && bodyThrew
                    && readParameter() === generator
                    && readBody() === generator;
            "#,
    ] {
        let outcome = engine()
            .run_script(
                source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .unwrap_or_else(|err| panic!("named generator binding should compile: {err:?}"));
        assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
    }
}

#[test]
fn wasm_backend_separates_generator_parameter_and_body_var_environments() {
    let outcome = engine()
        .run_script(
            r#"
                    var x = "outside";
                    var readParameter;
                    var readBody;
                    (function* (
                        unused = readParameter = function() { return x; }
                    ) {
                        var x = "inside";
                        readBody = function() { return x; };
                    }().next());
                    readParameter() === "outside" && readBody() === "inside";
                "#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("generator parameter environment should compile: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_destructures_nested_generator_parameters() {
    let outcome = engine()
        .run_script(
            r#"
                    var explicit;
                    var defaulted;
                    var f = function* (
                        [{ x, renamed: y }],
                        { values: [z] }
                    ) {
                        explicit = [x, y, z];
                    };
                    var g = function* (
                        [{ x } = { x: 4 }] = [],
                        { values: [y] } = { values: [5] }
                    ) {
                        defaulted = [x, y];
                    };
                    f([{ x: 1, renamed: 2 }], { values: [3] }).next();
                    g().next();
                    (explicit[0] === 1 ? 1 : 0)
                        + (explicit[1] === 2 ? 2 : 0)
                        + (explicit[2] === 3 ? 4 : 0)
                        + (defaulted[0] === 4 ? 8 : 0)
                        + (defaulted[1] === 5 ? 16 : 0);
                "#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| {
            panic!("nested generator parameter patterns should compile: {err:?}")
        });
    assert!(outcome.note.contains("number(31)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_injects_return_and_throw_into_suspended_generators() {
    let source = r#"
            let returnEffects = "";
            function* returnSequence() {
                returnEffects += "a";
                yield 1;
                returnEffects += "b";
                yield 2;
            }
            const returned = returnSequence();
            const returnFirst = returned.next();
            const returnResult = returned.return(9);
            const returnCompleted = returned.next();

            const marker = { marker: true };
            let thrownValue;
            function* throwSequence() {
                yield 3;
                yield 4;
            }
            const thrown = throwSequence();
            const throwFirst = thrown.next();
            try { thrown.throw(marker); } catch (error) { thrownValue = error; }
            const throwCompleted = thrown.next();

            (returnFirst.value === 1 && returnFirst.done === false ? 1 : 0)
                + (returnResult.value === 9 && returnResult.done === true ? 2 : 0)
                + (returnCompleted.value === undefined && returnCompleted.done === true ? 4 : 0)
                + (returnEffects === "a" ? 8 : 0)
                + (throwFirst.value === 3 && throwFirst.done === false ? 16 : 0)
                + (thrownValue === marker ? 32 : 0)
                + (thrownValue && thrownValue.marker === true ? 64 : 0)
                + (throwCompleted.value === undefined ? 128 : 0)
                + (throwCompleted.done === true ? 256 : 0);
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("abrupt generator resume should compile: {err:?}"));
    assert!(outcome.note.contains("number(511)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_preserves_abrupt_completions_across_yielding_generator_handlers() {
    let source = r#"
            const caughtMarker = { caught: true };
            function* caught() {
                try {
                    yield 1;
                } catch (error) {
                    yield error;
                    return 2;
                }
            }
            const caughtIterator = caught();
            const caughtFirst = caughtIterator.next();
            const caughtThrow = caughtIterator.throw(caughtMarker);
            const caughtReturn = caughtIterator.next();

            let returnEffects = "";
            function* returned() {
                try {
                    yield 3;
                } finally {
                    returnEffects += "f";
                    yield 4;
                    returnEffects += "g";
                }
            }
            const returnedIterator = returned();
            const returnedFirst = returnedIterator.next();
            const returnedFinally = returnedIterator.return(5);
            const returnedResult = returnedIterator.next();

            const thrownMarker = { thrown: true };
            function* thrown() {
                try {
                    yield 6;
                } finally {
                    yield 7;
                }
            }
            const thrownIterator = thrown();
            const thrownFirst = thrownIterator.next();
            const thrownFinally = thrownIterator.throw(thrownMarker);
            let thrownResult;
            try {
                thrownIterator.next();
            } catch (error) {
                thrownResult = error;
            }

            const overrideMarker = { override: true };
            function* overridden() {
                try {
                    yield 8;
                } finally {
                    throw overrideMarker;
                }
            }
            const overriddenIterator = overridden();
            const overriddenFirst = overriddenIterator.next();
            let overrideResult;
            try {
                overriddenIterator.return(9);
            } catch (error) {
                overrideResult = error;
            }
            const overriddenCompleted = overriddenIterator.next();

            function* nested() {
                try {
                    try {
                        yield 10;
                    } finally {
                        yield 11;
                    }
                } finally {
                    yield 12;
                }
            }
            const nestedIterator = nested();
            const nestedFirst = nestedIterator.next();
            const nestedInnerFinally = nestedIterator.return(13);
            const nestedOuterFinally = nestedIterator.next();
            const nestedResult = nestedIterator.next();

            caughtFirst.value === 1 && caughtFirst.done === false
                && caughtThrow.value === caughtMarker && caughtThrow.done === false
                && caughtReturn.value === 2 && caughtReturn.done === true
                && returnedFirst.value === 3 && returnedFirst.done === false
                && returnedFinally.value === 4 && returnedFinally.done === false
                && returnedResult.value === 5 && returnedResult.done === true
                && returnEffects === "fg"
                && thrownFirst.value === 6 && thrownFirst.done === false
                && thrownFinally.value === 7 && thrownFinally.done === false
                && thrownResult === thrownMarker
                && overriddenFirst.value === 8 && overriddenFirst.done === false
                && overrideResult === overrideMarker
                && overriddenCompleted.value === undefined && overriddenCompleted.done === true
                && nestedFirst.value === 10 && nestedFirst.done === false
                && nestedInnerFinally.value === 11 && nestedInnerFinally.done === false
                && nestedOuterFinally.value === 12 && nestedOuterFinally.done === false
                && nestedResult.value === 13 && nestedResult.done === true;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("structured generator handlers should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_resumes_generators_inside_simple_loops_without_replaying_work() {
    let source = r#"
            let forEffects = "";
            function* forSequence() {
                for (var i = 0; i < 3; i++) {
                    forEffects += "b";
                    yield i;
                    forEffects += "a";
                }
                return i;
            }
            const forIterator = forSequence();
            const forFirst = forIterator.next();
            const forSecond = forIterator.next();
            const forThird = forIterator.next();
            const forReturn = forIterator.next();

            function* whileSequence() {
                let i = 0;
                while (i < 2) {
                    yield i;
                    i++;
                }
                return i;
            }
            const whileIterator = whileSequence();
            const whileFirst = whileIterator.next();
            const whileSecond = whileIterator.next();
            const whileReturn = whileIterator.next();

            forFirst.value === 0 && forFirst.done === false
                && forSecond.value === 1 && forSecond.done === false
                && forThird.value === 2 && forThird.done === false
                && forReturn.value === 3 && forReturn.done === true
                && forEffects === "bababa"
                && whileFirst.value === 0 && whileFirst.done === false
                && whileSecond.value === 1 && whileSecond.done === false
                && whileReturn.value === 2 && whileReturn.done === true;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("loop generator should resume: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_resumes_the_selected_generator_branch_without_replaying_its_prefix() {
    let source = r#"
            let effects = "";
            function* choose(flag) {
                if (flag) {
                    effects += "T";
                    yield 1;
                    effects += "t";
                } else {
                    effects += "F";
                    yield 2;
                    effects += "f";
                }
                return 3;
            }
            const truthy = choose(true);
            const falsy = choose(false);
            const truthyYield = truthy.next();
            const falsyYield = falsy.next();
            const truthyReturn = truthy.next();
            const falsyReturn = falsy.next();

            function* maybe(flag) {
                if (flag) yield 4;
                return 5;
            }
            const skipped = maybe(false).next();

            function* conditional(flag) {
                yield flag ? 6 : 7;
                return 8;
            }
            const conditionalIterator = conditional(false);
            const conditionalYield = conditionalIterator.next();
            const conditionalReturn = conditionalIterator.next();

            truthyYield.value === 1 && truthyYield.done === false
                && falsyYield.value === 2 && falsyYield.done === false
                && truthyReturn.value === 3 && truthyReturn.done === true
                && falsyReturn.value === 3 && falsyReturn.done === true
                && skipped.value === 5 && skipped.done === true
                && conditionalYield.value === 7 && conditionalYield.done === false
                && conditionalReturn.value === 8 && conditionalReturn.done === true
                && effects === "TFtf";
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("branch generator should resume: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_preserves_generator_captures_across_yield() {
    let source = r#"
            let lexicalLet = { source: "let" };
            const lexicalConst = { source: "const" };
            var scriptVar = { source: "var" };
            const parameterValue = { source: "parameter" };

            function* captureLet() { yield 0; return lexicalLet; }
            function* captureConst() { yield 0; return lexicalConst; }
            function* captureVar() { yield 0; return scriptVar; }
            function* captureParameter(value) { yield 0; return value; }

            var letIterator = captureLet();
            var constIterator = captureConst();
            var varIterator = captureVar();
            var parameterIterator = captureParameter(parameterValue);
            letIterator.next();
            constIterator.next();
            varIterator.next();
            parameterIterator.next();

            letIterator.next().value === lexicalLet
                && constIterator.next().value === lexicalConst
                && varIterator.next().value === scriptVar
                && parameterIterator.next().value === parameterValue;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("generator captures should resume: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_forwards_generator_delegation_protocol_completions() {
    let source = r#"
            var nextGets = 0;
            var valueGets = 0;
            var normalFinal;
            var nextArgs = [];
            var firstResult = Object.defineProperty({}, "value", {
                get() { valueGets++; return 1; }
            });
            var normalStep = 0;
            var normalIterable = {
                [Symbol.iterator]() {
                    return {
                        get next() {
                            nextGets++;
                            return function (value) {
                                nextArgs.push(value);
                                normalStep++;
                                if (normalStep === 1) return firstResult;
                                return { done: true, value: 9 };
                            };
                        }
                    };
                }
            };
            function* normal() { normalFinal = yield* normalIterable; }
            var normalIterator = normal();
            var normalFirst = normalIterator.next(99);
            var normalDone = normalIterator.next(7);

            var returnedRaw = { done: false, value: 5 };
            var returnReceived;
            var returnNextReceived;
            var returnIterable = {
                [Symbol.iterator]() {
                    return {
                        next(value) {
                            returnNextReceived = value;
                            return value === 6
                                ? { done: true, value: 8 }
                                : { done: false, value: 1 };
                        },
                        return(value) {
                            returnReceived = value;
                            return returnedRaw;
                        }
                    };
                }
            };
            function* returned() { return yield* returnIterable; }
            var returnedIterator = returned();
            returnedIterator.next();
            var returnedYield = returnedIterator.return(4);
            var returnedDone = returnedIterator.next(6);

            var thrownRaw = { done: false, value: 12 };
            var throwReceived;
            var throwFinal;
            var throwStep = 0;
            var throwIterable = {
                [Symbol.iterator]() {
                    return {
                        next() {
                            throwStep++;
                            return throwStep === 1
                                ? { done: false, value: 10 }
                                : { done: true, value: 13 };
                        },
                        throw(value) {
                            throwReceived = value;
                            return thrownRaw;
                        }
                    };
                }
            };
            function* thrown() { throwFinal = yield* throwIterable; }
            var thrownIterator = thrown();
            thrownIterator.next();
            var thrownYield = thrownIterator.throw(11);
            var thrownDone = thrownIterator.next();

            var closeArgs = -1;
            var missingThrowCaught = false;
            var missingThrowIterable = {
                [Symbol.iterator]() {
                    return {
                        next() { return { done: false }; },
                        return() {
                            closeArgs = arguments.length;
                            return {};
                        }
                    };
                }
            };
            function* missingThrow() {
                try { yield* missingThrowIterable; }
                catch (error) { missingThrowCaught = error instanceof TypeError; }
            }
            var missingThrowIterator = missingThrow();
            missingThrowIterator.next();
            var missingThrowDone = missingThrowIterator.throw(14);

            var nonObjectCaught = false;
            var nonObjectIterable = {
                [Symbol.iterator]() { return { next() { return 1; } }; }
            };
            function* nonObject() {
                try { yield* nonObjectIterable; }
                catch (error) { nonObjectCaught = error instanceof TypeError; }
            }
            var nonObjectDone = nonObject().next();

            normalFirst === firstResult
                && normalFirst.done === undefined
                && valueGets === 0
                && nextGets === 1
                && nextArgs.length === 2
                && nextArgs[0] === undefined
                && nextArgs[1] === 7
                && normalFinal === 9
                && normalDone.value === undefined && normalDone.done === true
                && returnReceived === 4
                && returnedYield === returnedRaw
                && returnNextReceived === 6
                && returnedDone.value === 8 && returnedDone.done === true
                && throwReceived === 11
                && thrownYield === thrownRaw
                && throwFinal === 13
                && thrownDone.value === undefined && thrownDone.done === true
                && closeArgs === 0
                && missingThrowCaught
                && missingThrowDone.done === true
                && nonObjectCaught
                && nonObjectDone.done === true;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("generator delegation should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_generator_return_throw_and_brand_checks_match_iterator_semantics() {
    let source = r#"
            let ranReturn = false;
            function* returned() { ranReturn = true; }
            const returnedIterator = returned();
            const returnedResult = returnedIterator.return(7);

            let ranThrow = false;
            function* thrown() { ranThrow = true; }
            const thrownIterator = thrown();
            let thrownValue = 0;
            try { thrownIterator.throw(9); } catch (error) { thrownValue = error; }

            let incompatibleReceiverThrows = false;
            try { returnedIterator.next.call({}); } catch (error) {
                incompatibleReceiverThrows = error instanceof TypeError;
            }

            let reentrantIterator;
            function* reentrant() { reentrantIterator.next(); }
            reentrantIterator = reentrant();
            let reentrantThrows = false;
            try { reentrantIterator.next(); } catch (error) {
                reentrantThrows = error instanceof TypeError;
            }
            const afterReentrancy = reentrantIterator.next();

            !ranReturn && returnedResult.value === 7 && returnedResult.done === true
                && !ranThrow && thrownValue === 9 && incompatibleReceiverThrows
                && reentrantThrows && afterReentrancy.done === true;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("generator control methods should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_iterator_concat_is_eager_about_methods_and_lazy_about_iterators() {
    let source = r#"
            let events = [];
            function source(name, values) {
                return {
                    get [Symbol.iterator]() {
                        events.push("get " + name);
                        return function() {
                            events.push("open " + name);
                            let index = 0;
                            return {
                                next() {
                                    if (index === values.length) return { done: true };
                                    return { done: false, value: values[index++] };
                                }
                            };
                        };
                    }
                };
            }
            let iterator = Iterator.concat(source("a", [1]), source("b", [2]));
            let eager = events.join(",") === "get a,get b";
            let first = iterator.next();
            let second = iterator.next();
            let done = iterator.next();
            eager
                && events.join(",") === "get a,get b,open a,open b"
                && first.value === 1 && !first.done
                && second.value === 2 && !second.done
                && done.done;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Iterator.concat should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_iterator_zip_keyed_reads_options_before_sources_and_padding_after_them() {
    let source = r#"
            const events = [];
            const iterablesTarget = {
                left: [1],
                right: [2, 3]
            };
            const iterables = new Proxy(iterablesTarget, {
                ownKeys(target) {
                    events.push("ownKeys");
                    return Reflect.ownKeys(target);
                },
                getOwnPropertyDescriptor(target, key) {
                    events.push("descriptor " + key);
                    return Reflect.getOwnPropertyDescriptor(target, key);
                },
                get(target, key, receiver) {
                    events.push("get " + key);
                    return Reflect.get(target, key, receiver);
                }
            });
            const padding = new Proxy({ left: 9 }, {
                get(target, key, receiver) {
                    events.push("padding " + key);
                    return Reflect.get(target, key, receiver);
                }
            });
            const options = {
                get mode() {
                    events.push("mode");
                    return "longest";
                },
                get padding() {
                    events.push("padding");
                    return padding;
                }
            };

            const iterator = Iterator.zipKeyed(iterables, options);
            const constructionOrder = events.join(",") ===
                "mode,padding,ownKeys,descriptor left,get left,descriptor right,get right,padding left,padding right";
            const first = iterator.next();
            const second = iterator.next();
            const done = iterator.next();

            constructionOrder
                && first.value.left === 1
                && first.value.right === 2
                && !first.done
                && second.value.left === 9
                && second.value.right === 3
                && !second.done
                && done.done;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Iterator.zipKeyed ordering should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_iterator_zip_keyed_closes_acquired_iterators_in_reverse_order() {
    let source = r#"
            const events = [];
            const original = new Error("original");
            function iterator(name) {
                return {
                    next() {
                        return { done: true };
                    },
                    return() {
                        events.push(name);
                        throw new Error("ignored " + name);
                    }
                };
            }
            const bad = {
                get [Symbol.iterator]() {
                    throw original;
                }
            };
            let caught;
            try {
                Iterator.zipKeyed({
                    first: iterator("first"),
                    second: iterator("second"),
                    bad
                });
            } catch (error) {
                caught = error;
            }

            caught === original && events.join(",") === "second,first";
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("Iterator.zipKeyed closing should run: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}
