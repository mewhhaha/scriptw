use super::*;

#[test]
fn observed_wasm_primitives_preserve_number_and_bigint_data() {
    let engine = engine();
    let negative_zero = engine
        .observe_script("-0;", CompileOptions::default(), RunOptions::default())
        .expect("negative zero should complete normally");
    assert_eq!(
        negative_zero.completion,
        ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(-0.0)))
    );

    let bigint = engine
        .observe_script(
            "123456789012345678901234567890n;",
            CompileOptions::default(),
            RunOptions::default(),
        )
        .expect("heap BigInt should complete normally");
    assert_eq!(
        bigint.completion,
        ObservedCompletion::Normal(ObservedJsValue::BigInt(
            ObservedBigInt::parse_canonical_decimal("123456789012345678901234567890".into())
                .expect("expected BigInt should be canonical")
        ))
    );

    let lone_surrogate = engine
        .observe_script(
            "'a\\ud800b';",
            CompileOptions::default(),
            RunOptions::default(),
        )
        .expect("WTF-8 lone surrogate should remain observable as a UTF-16 unit");
    assert_eq!(
        lone_surrogate.completion,
        ObservedCompletion::Normal(ObservedJsValue::String(
            vec![0x0061, 0xd800, 0x0062].into_boxed_slice()
        ))
    );
}

/// `Engine::emit_wasm` must carry the backend's `debug_dump` out to its
/// caller.
///
/// This is the regression [`Artifact::debug_dump`] exists to prevent, and
/// nothing else pins it. `emit_wasm_on_current_thread` used to build an
/// `Artifact` with nowhere to put the dump and drop it on the floor, which
/// is the path both `scriptw build wasm` and the Test262 wasm-aot backend take
/// — so `SCRIPTW_WASM_TRACE_DUMP` printed nothing on exactly the two
/// commands anyone would use it from, and `SCRIPTW_EMIT_SIZE_REPORT`
/// silently reported nothing for a whole batch of size work. A test in
/// `scriptw-aot-wasm` cannot catch that: the dump was correct where it was
/// produced and lost one layer up.
///
/// The attribution line is asserted rather than mere non-emptiness, so that
/// truncating the dump to a placeholder does not pass.
#[test]
fn emit_wasm_carries_the_backend_debug_dump_to_its_caller() {
    let engine = engine();
    let unit = engine
        .compile_script("var x = 1 + 1;", CompileOptions::default())
        .expect("script compile should succeed");
    let artifact = engine.emit_wasm(&unit).expect("wasm emit should succeed");

    assert!(
        !artifact.debug_dump.is_empty(),
        "the wasm backend always produces a self-description; an empty dump \
             here means it was dropped between `scriptw_aot_wasm::emit` and the \
             `Artifact`"
    );
    for prefix in [
        "largest emitted function: ",
        "most locals in an emitted function: ",
        "emitted code bytes: ",
    ] {
        assert!(
            artifact
                .debug_dump
                .lines()
                .any(|line| line.starts_with(prefix)),
            "debug_dump is missing {prefix:?}:\n{}",
            artifact.debug_dump
        );
    }
}

#[test]
fn wasm_backend_atanh_retains_endpoint_accuracy_and_coercion_effects() {
    configure_compilation_jobs(1).unwrap();
    for directive in ["", "'use strict';"] {
        let source = format!(
            r#"
{directive}
var points = [
  [-0.9999983310699463, -6.998237084679027],
  [-0.9999978542327881, -6.87257975132917],
  [0.9928233623504639, 2.8132383539094192],
  [0.3, 0.3095196042031117],
  [0.00001, 0.000010000000000333334],
  [1e-300, 1e-300]
];
for (var [input, expected] of points) {{
  if (Math.abs(Math.atanh(input) - expected) > Math.abs(expected) * Number.EPSILON)
    throw 'atanh lost endpoint or small-value accuracy';
}}
var gets = 0, calls = 0, marker = {{}};
var value = {{ get valueOf() {{ gets++; return function () {{ calls++; return -0.9999983310699463; }}; }} }};
if (Math.abs(Math.atanh(value) + 6.998237084679027) > 2e-15 || gets !== 1 || calls !== 1)
  throw 'atanh conversion was repeated or discarded';
try {{ Math.atanh({{ valueOf() {{ throw marker; }} }}); throw 'missing abrupt conversion'; }}
catch (error) {{ if (error !== marker) throw error; }}
Object.is(Math.atanh(-0), -0) && Math.atanh(1) === Infinity && Math.atanh(-1) === -Infinity &&
Number.isNaN(Math.atanh(2)) && Number.isNaN(Math.atanh(-2)) && Number.isNaN(Math.atanh(NaN));
"#
        );
        let outcome = Engine::new(RealmBuilder::new().build())
            .run_script(
                &source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(30_000),
                    ..RunOptions::default()
                },
            )
            .expect("accurate atanh executes through the ordinary Wasm host import");
        assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
        assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
    }
}

#[test]
fn wasm_backend_math_random_uses_the_injected_realm_provider() {
    let random = DeterministicHostRandom {
        next_word: AtomicU64::new(0),
        step: 1_u64 << 63,
    };
    let engine = Engine::new(
        RealmBuilder::new()
            .with_host_random(Box::new(random))
            .build(),
    );
    let outcome = engine
        .run_script(
            r#"
let coerced = 0;
const ignored = { valueOf() { coerced = 1; throw new Error("coerced"); } };
Math.random(ignored) === 0 && coerced === 0 && Math.random() === 0.5;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Math.random should use the injected realm provider");

    assert!(
        outcome.note.contains("boolean(true)"),
        "unexpected result: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_characterization_matrix_locks_public_surface_and_outcomes() {
    let cases = [
        (
            "arithmetic",
            "let x = 40; const y = 2; x + y;",
            "number(42",
            ValueKind::Number,
        ),
        (
            "string-concat",
            "let left = 'hel'; let right = 'lo'; left + right;",
            "string(hello)",
            ValueKind::String,
        ),
        (
            "objects",
            "let o = { x: 1 }; o.y = 2; o.x + o.y;",
            "number(3",
            ValueKind::Number,
        ),
        (
            "caught-throw",
            "try { throw new TypeError('boom'); } catch (e) { e.name; }",
            "string(TypeError)",
            ValueKind::String,
        ),
    ];

    for (label, source, expected_note, expected_kind) in cases {
        let engine = engine();
        let unit = engine
            .compile_script(source, CompileOptions::default())
            .unwrap_or_else(|err| panic!("{label} should compile: {err:?}"));
        let artifact = engine
            .emit_wasm(&unit)
            .unwrap_or_else(|err| panic!("{label} should emit wasm: {err:?}"));
        let (imports, exports) = wasm_import_export_names(&artifact.bytes);
        // Heap-backed programs import the current typed GC host boundary.
        // Shared R functions remain named imports, admitted against that
        // artifact's actual exports rather than a retired manual-heap ABI.
        let host_imports: Vec<_> = imports
            .iter()
            .filter(|name| name.starts_with("scriptw_host::"))
            .map(String::as_str)
            .collect();
        assert_eq!(
            host_imports,
            [
                "scriptw_host::agent_broadcast_resource",
                "scriptw_host::agent_call",
                "scriptw_host::agent_can_suspend",
                "scriptw_host::agent_receive_resource",
                "scriptw_host::byte_array_allocate",
                "scriptw_host::collect_gc",
                "scriptw_host::finalization_cell_allocate",
                "scriptw_host::finalization_cell_remove",
                "scriptw_host::finalization_next_registry",
                "scriptw_host::finalization_target_alive",
                "scriptw_host::finalization_token_matches",
                "scriptw_host::finalization_track",
                "scriptw_host::intl_provider_call",
                "scriptw_host::math_acos",
                "scriptw_host::math_acosh",
                "scriptw_host::math_asin",
                "scriptw_host::math_asinh",
                "scriptw_host::math_atan",
                "scriptw_host::math_atan2",
                "scriptw_host::math_atanh",
                "scriptw_host::math_cbrt",
                "scriptw_host::math_cos",
                "scriptw_host::math_cosh",
                "scriptw_host::math_exp",
                "scriptw_host::math_expm1",
                "scriptw_host::math_log",
                "scriptw_host::math_log10",
                "scriptw_host::math_log1p",
                "scriptw_host::math_log2",
                "scriptw_host::math_sin",
                "scriptw_host::math_sinh",
                "scriptw_host::math_tan",
                "scriptw_host::math_tanh",
                "scriptw_host::monotonic_clock_nanos",
                "scriptw_host::notify_async_waiters",
                "scriptw_host::number_pow",
                "scriptw_host::print_line_utf8",
                "scriptw_host::private_memory",
                "scriptw_host::random_f64",
                "scriptw_host::register_async_waiter",
                "scriptw_host::reject_runtime_semantics",
                "scriptw_host::report_cleanup_exception",
                "scriptw_host::shared_buffer_allocate",
                "scriptw_host::shared_buffer_base",
                "scriptw_host::shared_buffer_grow",
                "scriptw_host::shared_buffer_growable",
                "scriptw_host::shared_buffer_length",
                "scriptw_host::shared_buffer_maximum",
                "scriptw_host::shared_buffer_wait",
                "scriptw_host::shared_memory",
                "scriptw_host::sleep_nanos",
                "scriptw_host::system_time_zone_snapshot",
                "scriptw_host::wall_clock_millis",
                "scriptw_host::weak_job_finish",
                "scriptw_host::weak_map_allocate",
                "scriptw_host::weak_map_delete",
                "scriptw_host::weak_map_get",
                "scriptw_host::weak_map_has",
                "scriptw_host::weak_map_set",
                "scriptw_host::weak_ref_allocate",
                "scriptw_host::weak_ref_deref",
                "scriptw_host::weak_set_add",
                "scriptw_host::weak_set_allocate",
                "scriptw_host::weak_set_delete",
                "scriptw_host::weak_set_has",
            ],
            "{label} host imports: {host_imports:?}"
        );
        let runtime = artifact
            .runtime
            .as_ref()
            .unwrap_or_else(|| panic!("{label} requires a linked runtime"));
        assert_eq!(
            runtime.0.source_compilation_profile(),
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable
        );
        assert_eq!(
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(&artifact.bytes)
                .unwrap(),
            runtime.0.source_compilation_profile()
        );
        let (runtime_imports, runtime_exports) = wasm_import_export_names(runtime.bytes());
        assert_eq!(
            runtime_imports, host_imports,
            "{label} R and P must share the exact fixed host boundary"
        );
        let runtime_exports: std::collections::BTreeSet<_> = runtime_exports.into_iter().collect();
        for import in imports
            .iter()
            .filter(|name| !name.starts_with("scriptw_host::"))
        {
            let name = import
                .strip_prefix("scriptw_runtime::")
                .unwrap_or_else(|| panic!("{label} unexpected import namespace: {import}"));
            assert!(
                runtime_exports.contains(name),
                "{label} import {name} must be exported by its exact linked R"
            );
        }
        assert_eq!(
            exports,
            [
                "main",
                "memory",
                scriptw_aot_wasm::EMBEDDING_BYTES,
                scriptw_aot_wasm::EMBEDDING_BYTES_READ,
                scriptw_aot_wasm::EMBEDDING_CALL,
                scriptw_aot_wasm::EMBEDDING_CANCEL,
                scriptw_aot_wasm::EMBEDDING_CHECKPOINT,
                scriptw_aot_wasm::EMBEDDING_PROMISE,
                scriptw_aot_wasm::EMBEDDING_READ,
                scriptw_aot_wasm::EMBEDDING_STRING,
                "scriptw.program_init",
            ],
            "{label} program exports"
        );
        assert!(
            !runtime_exports.contains("main"),
            "{label} runtime must not own the program entry"
        );
        for export in [
            "memory",
            WASM_THROW_ERROR_NAME_EXPORT,
            // Diagnostic strings are rooted GC exports; a missing export
            // must remain a named artifact failure.
            WASM_THROW_ERROR_MESSAGE_EXPORT,
            WASM_THROW_ERROR_CONSTRUCTOR_NAME_EXPORT,
        ] {
            assert!(
                runtime_exports.contains(export),
                "{label} runtime exports: {runtime_exports:?}"
            );
        }

        let outcome = engine
            .run_compiled_unit(
                &unit,
                source,
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .unwrap_or_else(|err| panic!("{label} should run: {err:?}"));
        assert!(
            outcome.note.contains(expected_note),
            "{label} note: {}",
            outcome.note
        );

        let (_, tag, completion_kind, _) = run_wasm_raw(source);
        assert_eq!(tag.value_kind(), expected_kind, "{label} result kind");
        assert_eq!(completion_kind, 0, "{label} completion kind");
    }

    // Scalar arithmetic on literals is admitted by the runtime-free proof:
    // no Realm bootstrap, so only the unconditional control/GC imports remain.
    let scalar = "40 + 2;";
    let scalar_engine = engine();
    let unit = scalar_engine
        .compile_script(scalar, CompileOptions::default())
        .expect("scalar arithmetic should compile");
    let artifact = scalar_engine
        .emit_wasm(&unit)
        .expect("scalar arithmetic should emit wasm");
    assert!(artifact.runtime.is_none());
    assert_eq!(
        scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(&artifact.bytes).unwrap(),
        scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable
    );
    let (imports, exports) = wasm_import_export_names(&artifact.bytes);
    assert_eq!(
        imports,
        [
            "scriptw_host::agent_can_suspend",
            "scriptw_host::byte_array_allocate",
            "scriptw_host::collect_gc",
            "scriptw_host::reject_runtime_semantics",
        ],
        "runtime-free imports: {imports:?}"
    );
    assert_eq!(
        exports,
        [
            "main",
            "memory",
            WASM_THROW_ERROR_CONSTRUCTOR_NAME_EXPORT,
            WASM_THROW_ERROR_MESSAGE_EXPORT,
            WASM_THROW_ERROR_NAME_EXPORT,
        ],
        "runtime-free exports: {exports:?}"
    );
    let outcome = scalar_engine
        .run_compiled_unit(
            &unit,
            scalar,
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("scalar arithmetic should run");
    assert!(outcome.note.contains("number(42"), "note: {}", outcome.note);

    let err = engine()
        .run_script(
            "throw new TypeError('boom');",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect_err("uncaught throw should stay observable");
    assert!(
        err.message()
            .contains("uncaught throw: TypeError: wasm-aot completion: object"),
        "error: {err}"
    );
    // The handle alone is not a diagnosis. This assertion is the enforcement
    // half of the throw-message repair: without it the improvement is
    // something a reader observes once and a later refactor removes without
    // any test noticing, which is exactly how an address-only completion became the
    // shared signature of ~2,488 unrelated failures.
    assert!(
        err.message().contains("boom"),
        "an uncaught throw must carry the thrown error's message, not just its address: {err}"
    );

    // ...and a runtime-thrown error, whose message the runtime supplies
    // rather than the script. This is the same repair seen from the other
    // side: before it, `message` was defined from the error's `name`, so
    // there was no message to report even in principle.
    let runtime_err = engine()
        .run_script(
            "null.x;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect_err("reading a property of null should throw");
    assert!(
        runtime_err
            .message()
            .contains("uncaught throw: TypeError: "),
        "error: {runtime_err}"
    );
    assert!(
        runtime_err
            .message()
            // The selected GetV operation owns Realm-bound ToObject's
            // complete throw, including its native diagnostic message.
            .contains("Cannot convert undefined or null to object"),
        "a runtime-thrown error must report its own message: {runtime_err}"
    );
}

#[test]
fn wasm_backend_renders_a_heap_bigint_normal_completion() {
    let outcome = engine()
        .run_script(
            "const buffer = new ArrayBuffer(8); const view = new DataView(buffer); \
                 for (let index = 0; index < 8; index++) view.setUint8(index, 255); \
                 view.getBigUint64(0);",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("heap BigInt normal completion should run");

    assert_eq!(
        outcome.note,
        "wasm-aot completion: bigint(18446744073709551615n)"
    );
}

#[test]
fn wasm_backend_parses_decimal_exponents_with_combined_scale() {
    let outcome = engine()
            .run_script(
                "JSON.parse('1.1e-1') === 0.11 && parseFloat('1.1e-1') === 0.11 && parseFloat('1.23e1') === 12.3;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("decimal exponent parse should run");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_rounds_runtime_decimal_spans_exactly() {
    let outcome = engine()
        .run_script(
            r#"
var large = "23456789012E66";
var largeFraction = "1.234567890E+34";
var halfway = "1.00000000000000011102230246251565404236316680908203125";
var aboveHalfway = "1.00000000000000011102230246251565404236316680908203126";
var minimum = "4.9406564584124654e-324";
var maximum = "1.7976931348623157e308";
var overflow = "1.7976931348623159e308";
JSON.parse(large) === 2.3456789012e76 &&
JSON.parse(largeFraction) === 1.234567890e34 &&
JSON.parse("23456789012E66") === 2.3456789012e76 &&
JSON.parse("1.234567890E+34") === 1.234567890e34 &&
parseFloat(halfway + "suffix") === 1 &&
Number(aboveHalfway) === 1.0000000000000002 &&
JSON.parse(minimum, null) === 5e-324 &&
parseFloat(maximum) === 1.7976931348623157e308 &&
Number(overflow) === Infinity;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("runtime decimal conversion should run");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );

    for source in [
        "JSON.parse('-2e-324', null);",
        "parseFloat('-2e-324');",
        "Number('-2e-324');",
    ] {
        let (payload, tag, completion, _) = run_wasm_raw(source);
        assert_eq!(tag.value_kind(), ValueKind::Number, "source: {source}");
        assert_eq!(completion, 0, "source: {source}");
        assert_eq!(payload as u64, (-0.0f64).to_bits(), "source: {source}");
    }
}

#[test]
fn wasm_backend_rounds_runtime_decimal_boundaries_exactly() {
    let cases = [
        ("JSON.parse('1e-4000');", 0.0f64.to_bits()),
        ("JSON.parse('2e-324');", 0.0f64.to_bits()),
        ("JSON.parse('3e-324');", 1),
        ("JSON.parse('2.225073858507201e-308');", (1_u64 << 52) - 1),
        (
            "JSON.parse('2.2250738585072012e-308');",
            f64::MIN_POSITIVE.to_bits(),
        ),
        ("JSON.parse('1.7976931348623157e308');", f64::MAX.to_bits()),
        ("JSON.parse('1.7976931348623158e308');", f64::MAX.to_bits()),
        (
            "JSON.parse('1.7976931348623159e308');",
            f64::INFINITY.to_bits(),
        ),
        ("JSON.parse('1e309');", f64::INFINITY.to_bits()),
    ];

    for (source, expected_bits) in cases {
        let (payload, tag, completion, _) = run_wasm_raw(source);
        assert_eq!(tag.value_kind(), ValueKind::Number, "source: {source}");
        assert_eq!(completion, 0, "source: {source}");
        assert_eq!(payload as u64, expected_bits, "source: {source}");
    }
}

#[test]
fn wasm_backend_json_parse_structural_validator_accepts_nested_keywords() {
    let outcome = engine()
            .run_script(
                "function valid(text) { try { JSON.parse(text); return true; } catch (e) { return false; } } function invalid(text) { try { JSON.parse(text); return false; } catch (e) { return e instanceof SyntaxError; } } var zero = '{\"a\":0}'; var leading = '{\"a\":013}'; var trailing = '{\"a\":true} \"extra\"'; var doubleColon = '{\"a\"::true}'; valid('{\"a\":true}') && valid('{\"a\":false}') && valid('{\"a\":null}') && valid(zero) && invalid('{\"a\":tru}') && invalid('{\"a\":fals}') && invalid('{\"a\":nul}') && invalid('{\"a\":true,}') && invalid(leading) && invalid(trailing) && invalid(doubleColon);",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("JSON.parse nested keyword validation should run");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_parse_enforces_number_grammar_inside_structures() {
    let outcome = engine()
            .run_script(
                "function valid(text) { try { JSON.parse(text); return true; } catch (e) { return false; } } function invalid(text) { try { JSON.parse(text); return false; } catch (e) { return e instanceof SyntaxError; } } var accepted = ['0', '-0', '10', '0.5', '-12.75', '1e2', '1E+2', '1e-2', '[0,-0,10,0.5,1e2]', '{\"n\":-12.75e+2}', '{\"\\\\u0123\":5}', '[\".1\",\"00\",\"1e+\"]']; var rejected = ['-', '+', '00', '01', '1.', '1.e1', '.1', '1e', '1e+', '1e-', '[00]', '{\"n\":013}', '[1.]', '[1.e2]', '[.1]', '[1e]', '[1e+]', '[1e-]', '[0x14]']; var correct = true; for (var i = 0; i < accepted.length; i++) correct = correct && valid(accepted[i]); for (var i = 0; i < rejected.length; i++) correct = correct && invalid(rejected[i]); correct;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("JSON.parse number grammar validation should run");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_parse_rejects_structural_trailing_commas() {
    let outcome = engine()
            .run_script(
                "function valid(text) { try { JSON.parse(text); return true; } catch (e) { return false; } } function invalid(text) { try { JSON.parse(text); return false; } catch (e) { return e instanceof SyntaxError; } } valid('[]') && valid('[1]') && valid('{}') && valid('{\"a\":1}') && invalid('[1,]') && invalid('[\"a\",]') && invalid('{,}') && invalid('{\"a\":1,}') && invalid('[{,}]') && invalid('[[1,]]') && invalid('[{\"a\":\"b\",}]');",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("JSON.parse trailing-comma validation should run");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_parse_materializes_dynamic_composites() {
    let outcome = engine()
            .run_script(
                r#"
var text = '{"array":[null,true,false,-0,12.5e1,"line\\n\\u20ac","\\uD834\\uDF06","\\uD834"],"duplicate":1,' +
  '"__proto__":{"safe":true},"duplicate":2}';
var value = JSON.parse(text, null);
value.array.length === 8 &&
  value.array[0] === null && value.array[1] === true && value.array[2] === false &&
  1 / value.array[3] === -Infinity && value.array[4] === 125 &&
  value.array[5] === "line\n\u20ac" && value.duplicate === 2 &&
  value.array[6] === String.fromCharCode(0xD834, 0xDF06) &&
  value.array[7] === String.fromCharCode(0xD834) &&
  Object.keys(value.array).join(",") === "0,1,2,3,4,5,6,7" &&
  Object.keys(value).join(",") === "array,duplicate,__proto__" &&
  Object.getPrototypeOf(value) === Object.prototype &&
  Object.prototype.hasOwnProperty.call(value, "__proto__") &&
  value.__proto__.safe === true;
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("dynamic JSON arrays and objects should materialize");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_parse_uses_an_unbounded_dynamic_nesting_stack() {
    let outcome = engine()
        .run_script(
            r#"
var text = "0";
for (var i = 0; i < 160; i++) text = "[" + text + "]";
var value = JSON.parse(text);
for (var j = 0; j < 160; j++) value = value[0];
value === 0;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("dynamic JSON nesting should not be limited by validator bit masks");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_wasm_aot_finishes_near_timeout_deadline_is_not_falsely_killed() {
    // A case that legitimately finishes just under its deadline must
    // still be reported as a success, not spuriously classified as a
    // timeout because it happened to run close to the bound. Loop a
    // bounded, deterministic amount of work rather than sleeping (Wasm
    // has no sleep primitive here); this keeps the check fast while
    // still exercising many epoch-check back-edges before returning
    // normally, well inside a generous timeout bound.
    let outcome = engine()
        .run_script(
            "var sum = 0; for (var i = 0; i < 200000; i++) { sum += i; } sum;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("a legitimately-finishing case must not be falsely killed by the timeout");
    assert!(outcome.note.contains("number("), "note: {}", outcome.note);
}

#[test]
fn wasm_backend_json_parse_reviver_uses_root_wrapper_this() {
    let outcome = engine()
            .run_script(
                "var wrapper; JSON.parse('2', function() { wrapper = this; }); var isEnumerable = Function.prototype.call.bind(Object.prototype.propertyIsEnumerable); function enumCheck(obj, name) { var seen = false; for (var key in obj) { if (key === name) { seen = true; break; } } return seen && Object.prototype.hasOwnProperty.call(obj, name) && isEnumerable(obj, name); } function configurableCheck(obj, name) { delete obj[name]; return !Object.prototype.hasOwnProperty.call(obj, name); } typeof wrapper === 'object' && Object.getPrototypeOf(wrapper) === Object.prototype && Object.getOwnPropertyNames(wrapper).length === 1 && wrapper[''] === 2 && enumCheck(wrapper, '') && configurableCheck(wrapper, '');",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("JSON.parse reviver wrapper should run");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_parse_dynamic_reviver_walks_post_order_with_exact_sources() {
    let outcome = engine()
        .run_script(
            r#"
var calls = "";
var sources = "";
function parse(text) {
  return JSON.parse(text, function(key, value, context) {
    calls += (calls === "" ? "" : ",") + key;
    sources += (sources === "" ? "" : ",") +
      (Object.prototype.hasOwnProperty.call(context, "source") ? context.source : "-");
    return value;
  });
}
var result = parse(' [1.0,{"x":"two"}] ');
calls === "0,x,1," && sources === '1.0,"two",-,-' &&
  result[0] === 1 && result[1].x === "two";
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("dynamic JSON reviver traversal should run");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_parse_dynamic_reviver_deletes_and_replaces_properties() {
    let outcome = engine()
        .run_script(
            r#"
function parse(text) {
  return JSON.parse(text, function(key, value) {
    if (key === "drop") return undefined;
    if (key === "0") return value + 10;
    if (key === "") return { wrapped: value };
    return value;
  });
}
var result = parse('{"keep":[1,2],"drop":3}');
result.wrapped.keep[0] === 11 && result.wrapped.keep[1] === 2 &&
  !Object.prototype.hasOwnProperty.call(result.wrapped, "drop");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("dynamic JSON reviver deletion and replacement should run");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_parse_dynamic_reviver_observes_forward_holder_mutation() {
    let outcome = engine()
        .run_script(
            r#"
var replacement = { deep: { marker: 2 } };
var calls = "";
function parse(text) {
  return JSON.parse(text, function(key, value) {
    calls += (calls === "" ? "" : ",") + key;
    if (key === "0") this[1] = replacement;
    return value;
  });
}
var result = parse('[0,{"old":1}]');
calls === "0,marker,deep,1," && result[1] === replacement &&
  result[1].deep === replacement.deep && result[1].deep.marker === 2;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("dynamic JSON reviver forward holder mutation should run");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_parse_dynamic_reviver_propagates_abrupt_completion() {
    let outcome = engine()
        .run_script(
            r#"
var calls = 0;
var objectSentinel = {};
var primitiveSentinel = "stop";
function parseObjectThrow(text) {
  return JSON.parse(text, function(key, value) {
    calls += 1;
    if (key === "0") throw objectSentinel;
    return value;
  });
}
function parsePrimitiveThrow(text) {
  return JSON.parse(text, function(key, value) {
    calls += 1;
    if (key === "0") throw primitiveSentinel;
    return value;
  });
}
var objectIdentityPreserved = false;
try { parseObjectThrow("[1,2]"); } catch (error) {
  objectIdentityPreserved = error === objectSentinel;
}
var primitiveIdentityPreserved = false;
try { parsePrimitiveThrow("[1,2]"); } catch (error) {
  primitiveIdentityPreserved = error === primitiveSentinel;
}
objectIdentityPreserved && primitiveIdentityPreserved && calls === 2;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("dynamic JSON reviver throw should propagate");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_raw_json_preserves_source_and_uses_an_unforgeable_frozen_brand() {
    let outcome = engine()
        .run_script(
            r#"
var raw = JSON.rawJSON("1e400");
var escaped = JSON.rawJSON('"\\u0061"');
var descriptor = Object.getOwnPropertyDescriptor(raw, "rawJSON");
var proxy = new Proxy(raw, {});
var inherited = Object.create(raw);
var fake = { rawJSON: "0" };
var mutationThrows = false;
try { Object.defineProperty(raw, "rawJSON", { value: "0" }); } catch (error) {
  mutationThrows = error instanceof TypeError;
}
[
  Object.getPrototypeOf(raw) === null,
  Object.isFrozen(raw),
  Object.isExtensible(raw),
  descriptor.value,
  descriptor.writable,
  descriptor.enumerable,
  descriptor.configurable,
  JSON.isRawJSON(raw),
  JSON.isRawJSON(proxy),
  JSON.isRawJSON(inherited),
  JSON.isRawJSON(fake),
  JSON.stringify(raw),
  JSON.stringify(escaped),
  JSON.stringify({ raw: raw }),
  JSON.stringify({ 42: JSON.rawJSON("37") }),
  JSON.stringify([escaped, raw]),
  JSON.stringify(proxy),
  mutationThrows,
  raw.rawJSON
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("JSON raw wrappers should preserve source and remain unforgeable");
    assert!(
            outcome.note.contains(
                "string(true|true|false|1e400|false|true|false|true|false|false|false|1e400|\"\\u0061\"|{\"raw\":1e400}|{\"42\":37}|[\"\\u0061\",1e400]|{\"rawJSON\":\"1e400\"}|true|1e400)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_json_raw_json_validates_primitive_text_after_coercion_in_its_realm() {
    let outcome = engine()
        .run_script(
            r#"
var invalidTexts = [
  "", " true", "true ", "\t0", "0\n", "{}", "[]", "undefined", "NaN",
  "Infinity", "-Infinity", "01", "1.", "1e", "1e+", "--1", "+1", ".1",
  "truefalse", "nul", "\"unterminated", "\"bad\\x\"", "\"ok\"x"
];
var syntaxErrors = 0;
for (var i = 0; i < invalidTexts.length; i++) {
  try { JSON.rawJSON(invalidTexts[i]); } catch (error) {
    if (error instanceof SyntaxError) syntaxErrors += 1;
  }
}
var coercions = 0;
var coerced = JSON.rawJSON({
  toString: function() { coercions += 1; return "42"; }
});
var sentinel = {};
var abruptIdentity = false;
try {
  JSON.rawJSON({ toString: function() { throw sentinel; } });
} catch (error) {
  abruptIdentity = error === sentinel;
}
var symbolTypeError = false;
try { JSON.rawJSON(Symbol("1")); } catch (error) {
  symbolTypeError = error instanceof TypeError;
}
var other = __scriptwCreateRealm().global;
var otherSyntaxRealm = false;
try { other.JSON.rawJSON("[]"); } catch (error) {
  otherSyntaxRealm = Object.getPrototypeOf(error) === other.SyntaxError.prototype;
}
var otherTypeRealm = false;
var otherRawJSON = other.JSON.rawJSON;
try { otherRawJSON(Symbol("1")); } catch (error) {
  otherTypeRealm = Object.getPrototypeOf(error) === other.TypeError.prototype;
}
[
  syntaxErrors,
  invalidTexts.length,
  coercions,
  coerced.rawJSON,
  abruptIdentity,
  symbolTypeError,
  otherSyntaxRealm,
  otherTypeRealm,
  JSON.isRawJSON(other.JSON.rawJSON("1")),
  other.JSON.isRawJSON(JSON.rawJSON("2"))
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("JSON.rawJSON should validate primitive text after observable coercion");
    assert!(
        outcome
            .note
            .contains("string(23|23|1|42|true|true|true|true|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_compiles_aot_known_direct_eval_source() {
    // `eval("1")` was the slice `wasm_emit_reports_unsupported_slice_precisely`
    // locked as unsupported until the prepared direct-eval registry gave
    // AOT-known sources their caller-environment seam
    // (docs/rust-rewrite/contracts/prepared-direct-eval.md). The source is
    // compiled ahead of time; no evaluator is emitted.
    for (source, expected) in [
        ("eval(\"1\");", "number(1)"),
        (
            "var x = 5; function f() { var x = 7; return eval(\"x + 1\"); } f();",
            "number(8)",
        ),
    ] {
        let engine = engine();
        let unit = engine
            .compile_script(source, CompileOptions::default())
            .expect("script compile should succeed");
        engine
            .emit_wasm(&unit)
            .unwrap_or_else(|err| panic!("AOT-known direct eval should emit `{source}`: {err:?}"));
        let outcome = engine
            .run_compiled_unit(
                &unit,
                source,
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .unwrap_or_else(|err| panic!("AOT-known direct eval should run `{source}`: {err:?}"));
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_eval_no_source_branch_preserves_values_calls_and_argument_order() {
    let outcome = engine()
        .run_script(
            r#"
let object = { marker: 1 };
let boxedString = new String("1 + 1");
let symbol = Symbol("eval marker");
let order = "";
function mark(value, label) {
  order += label;
  return value;
}
function fail() {
  throw 9;
}
function replacement(value) {
  return value + 1;
}
function returnsNumber() {
  return 1;
}
function dispatch(useEval, value) {
  let selected = useEval ? eval : replacement;
  return selected(value);
}
function dispatchNoArg(useEval) {
  let selected = useEval ? eval : returnsNumber;
  return selected();
}
function dispatchUndefined(useEval) {
  let selected = useEval ? eval : returnsNumber;
  return selected(undefined);
}
let indirect = eval;
let callable = eval(function(value) { return value + 1; });
let abrupt = false;
try {
  eval(1, fail());
} catch (error) {
  abrupt = error === 9;
}
[
  eval() === undefined,
  eval(7) === 7,
  eval(true) === true,
  eval(null) === null,
  eval(object) === object,
  eval(boxedString) === boxedString,
  indirect(symbol) === symbol,
  callable(4) === 5,
  eval(mark(object, "a"), mark(0, "b")) === object,
  order === "ab",
  dispatch(true, 4) === 4,
  dispatch(false, 4) === 5,
  dispatchNoArg(true) === undefined,
  dispatchNoArg(false) === 1,
  dispatchUndefined(true) === undefined,
  dispatchUndefined(false) === 1,
  abrupt
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("non-String eval should use the ordinary builtin pass-through branch");
    assert!(
            outcome.note.contains(
                "string(true|true|true|true|true|true|true|true|true|true|true|true|true|true|true|true|true)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
#[cfg(not(feature = "spec-exec-oracle"))]
fn spec_exec_backend_errors_clearly_when_oracle_feature_is_off() {
    // Product/release builds compile without the `spec-exec-oracle`
    // feature, so selecting the interpreter oracle backend must fail
    // loudly instead of silently falling back to Wasm-AOT or panicking.
    let err = engine()
        .run_script(
            "1 + 1;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::SpecExec,
                ..RunOptions::default()
            },
        )
        .expect_err("spec-exec must not be usable without the spec-exec-oracle feature");
    assert!(err.message().contains("spec-exec-oracle"));
}

#[test]
#[cfg(feature = "spec-exec-oracle")]
fn spec_exec_backend_runs_as_developer_oracle_when_feature_enabled() {
    let outcome = engine()
        .run_script(
            "1 + 1;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::SpecExec,
                ..RunOptions::default()
            },
        )
        .expect("spec exec oracle should run a simple script when explicitly enabled");
    assert_eq!(outcome.backend_used, ExecutionBackend::SpecExec);
}

#[test]
fn wasm_backend_runs_supported_script() {
    let outcome = engine()
        .run_script(
            "let x = 40; const y = 2; x + y;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should run supported script");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(outcome.note.contains("number(42"));
}

#[test]
fn wasm_backend_supports_annexb_date_legacy_methods() {
    let outcome = engine()
            .run_script(
                r#"
var total = 0;
if (new Date(1899, 0).getYear() !== -1) throw "getYear 1899";
if (new Date(1970, 0).getYear() !== 70) throw "getYear 1970";
if (new Date({}).getYear() === new Date({}).getYear()) throw "invalid getYear";
var d = new Date(1970, 1, 2, 3, 4, 5);
var expected = new Date(1971, 1, 2, 3, 4, 5).valueOf();
if (d.setYear(71) !== expected) throw "setYear relative";
if (d.valueOf() !== expected) throw "setYear value";
d = new Date(1970, 0);
d.setYear(2000);
if (d.getFullYear() !== 2000) throw "setYear absolute";
d = new Date(0);
if (d.setYear(NaN) === d.setYear(NaN)) throw "setYear NaN";
d = new Date(0);
if (d.setYear() === d.setYear()) throw "setYear undefined";
var threw = 0;
try { Date.prototype.getYear.call({}); } catch (e) { if (e.name === "TypeError") threw += 1; }
try { Date.prototype.setYear.call(null, 1); } catch (e) { if (e.name === "TypeError") threw += 1; }
if (threw !== 2) throw "receiver TypeError";
if (Date.prototype.toGMTString !== Date.prototype.toUTCString) throw "GMT alias";
if (Date.prototype.getYear.length !== 0 || Date.prototype.setYear.length !== 1) throw "length";
if (Date.prototype.getYear.name !== "getYear" || Date.prototype.setYear.name !== "setYear") throw "name";
262;
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("wasm backend should support AnnexB Date legacy methods");
    assert!(outcome.note.contains("number(262"));
}

#[test]
fn wasm_backend_normalizes_fixed_time_zones_and_replaces_zoned_date_time_zone() {
    let outcome = engine()
        .run_script(
            r#"
function same(actual, expected, label) {
  if (actual !== expected) throw label;
}

var reference = new Temporal.ZonedDateTime(988786472987654321n, "UTC");
for (var entry of [
  ["utc", "UTC"],
  ["+01", "+01:00"],
  ["+0130", "+01:30"],
  ["-0650", "-06:50"],
  ["2021-08-19T17:30Z", "UTC"],
  ["2021-08-19T17:30-07:00", "-07:00"],
  ["2021-08-19T17:30[UTC]", "UTC"],
  ["2021-08-19T17:30-12:12[+01:46]", "+01:46"],
  ["2016-12-31T23:59:60Z", "UTC"]
]) {
  var changed = reference.withTimeZone(entry[0]);
  same(changed.timeZoneId, entry[1], "normalized " + entry[0]);
  same(changed.epochNanoseconds, reference.epochNanoseconds, "epoch " + entry[0]);
  same(changed.calendarId, reference.calendarId, "calendar " + entry[0]);
  if (changed === reference) throw "fresh object " + entry[0];
}

var donor = new Temporal.ZonedDateTime(1n, "+0130");
Object.defineProperty(donor, "timeZoneId", {
  get: function () { throw "observable timeZoneId access"; }
});
same(reference.withTimeZone(donor).timeZoneId, "+01:30", "branded argument slot");

class CustomZonedDateTime extends Temporal.ZonedDateTime {}
var custom = new CustomZonedDateTime(0n, "UTC");
var replaced = custom.withTimeZone("-08");
same(Object.getPrototypeOf(replaced), Temporal.ZonedDateTime.prototype, "intrinsic prototype");
same(replaced.timeZoneId, "-08:00", "subclass normalized");

same(
  reference.withTimeZone("+0330").equals(reference.withTimeZone("+03:30")),
  true,
  "equivalent offsets"
);
same(
  reference.withTimeZone("1994-11-05T13:15:30Z").equals(reference),
  true,
  "date-time UTC"
);

var epoch = new Temporal.ZonedDateTime(0n, "+00");
same(epoch.timeZoneId, "+00:00", "constructor normalization");
same(epoch.equals({
  year: 1970,
  month: 1,
  day: 1,
  timeZone: "+0000"
}), true, "property bag offset normalization");
same(new Temporal.ZonedDateTime(0n, "+01:30").equals({
  year: 1970,
  month: 1,
  day: 1,
  hour: 1,
  minute: 30,
  timeZone: donor
}), true, "property bag branded time zone");

for (var invalid of [
  "2021-08-19T17:30",
  "+01:30:01",
  "2021-08-19T17:30-07:00:00",
  "-000000-01-01T00:00Z",
  "NoSuch/Zone"
]) {
  var rangeError = false;
  try { reference.withTimeZone(invalid); } catch (error) {
    rangeError = error instanceof RangeError;
  }
  if (!rangeError) throw "range " + invalid;
}

var withTimeZone = Temporal.ZonedDateTime.prototype.withTimeZone;
same(withTimeZone.name, "withTimeZone", "name");
same(withTimeZone.length, 1, "length");
var descriptor = Object.getOwnPropertyDescriptor(
  Temporal.ZonedDateTime.prototype,
  "withTimeZone"
);
if (!descriptor.writable || descriptor.enumerable || !descriptor.configurable) {
  throw "descriptor";
}
var errors = 0;
try { withTimeZone.call({}, "UTC"); } catch (error) {
  if (error instanceof TypeError) errors += 1;
}
try { reference.withTimeZone({}); } catch (error) {
  if (error instanceof TypeError) errors += 1;
}
try { new withTimeZone("UTC"); } catch (error) {
  if (error instanceof TypeError) errors += 1;
}
same(errors, 3, "errors");
262;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should normalize and replace fixed Temporal time zones");
    assert!(outcome.note.contains("number(262"));
}

#[test]
fn wasm_backend_supports_date_core_time_values_and_timezone_offset() {
    let outcome = engine()
            .run_script(
                r#"
var beforeNow = Date.now();
var afterNow = Date.now();
if (beforeNow < 946684800000 || beforeNow > 4102444800000) throw "Date.now range";
if (afterNow < beforeNow) throw "Date.now nondecreasing";
if (new Date(6.54321).valueOf() !== 6) throw "positive TimeClip";
if (new Date(-6.54321).valueOf() !== -6) throw "negative TimeClip";
if (new Date(-0).valueOf() !== 0) throw "negative zero TimeClip";
if (1 / new Date(-0).valueOf() !== Infinity) throw "positive zero TimeClip";
if (new Date(Infinity).valueOf() === new Date(Infinity).valueOf()) throw "Infinity TimeClip";
if (new Date(-Infinity).valueOf() === new Date(-Infinity).valueOf()) throw "-Infinity TimeClip";
if (new Date(2016, 0, 1, 0, 0, 0, -1).getFullYear() !== 2015) throw "ms underflow";
if (new Date(2016, 11, 31, 23, 59, 59, 1000).getFullYear() !== 2017) throw "ms overflow";
if (new Date(0).getTimezoneOffset() !== 0) throw "timezone offset";
if (new Date(NaN).getTimezoneOffset() === new Date(NaN).getTimezoneOffset()) throw "invalid timezone offset";
var threw = 0;
try { Date.prototype.getTimezoneOffset.call({}); } catch (e) { if (e.name === "TypeError") threw += 1; }
try { new Date().getTimezoneOffset.prototype; } catch (e) {}
if (threw !== 1) throw "timezone receiver";
if (Date.prototype.getTimezoneOffset.length !== 0) throw "timezone length";
if (Date.prototype.getTimezoneOffset.name !== "getTimezoneOffset") throw "timezone name";
var constructThrew = 0;
try { new Date.prototype.getTimezoneOffset(); } catch (e) { if (e.name === "TypeError") constructThrew = 1; }
if (constructThrew !== 1) throw "timezone construct";
262;
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("wasm backend should support Date core time values");
    assert!(outcome.note.contains("number(262"));
}

#[test]
fn wasm_backend_keeps_date_now_body_available_across_control_flow() {
    let outcome = engine()
            .run_script(
                "var first = Date.now(); if (false) 1; var second = Date.now(); if (first < 946684800000 || second < first) throw 'Date.now'; 262;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("Date.now should not be replaced by the deferred builtin stub");
    assert!(outcome.note.contains("number(262"));
}

#[test]
fn wasm_backend_supports_date_component_getters() {
    let outcome = engine()
            .run_script(
                r#"
function check(date, year, month, day, weekDay, hour, minute, second, ms, label) {
  if (date.getUTCFullYear() !== year) throw label + " utc year";
  if (date.getUTCMonth() !== month) throw label + " utc month";
  if (date.getUTCDate() !== day) throw label + " utc date";
  if (date.getUTCDay() !== weekDay) throw label + " utc day";
  if (date.getUTCHours() !== hour) throw label + " utc hours";
  if (date.getUTCMinutes() !== minute) throw label + " utc minutes";
  if (date.getUTCSeconds() !== second) throw label + " utc seconds";
  if (date.getUTCMilliseconds() !== ms) throw label + " utc ms";
  if (date.getFullYear() !== year) throw label + " local year";
  if (date.getMonth() !== month) throw label + " local month";
  if (date.getDate() !== day) throw label + " local date";
  if (date.getDay() !== weekDay) throw label + " local day";
  if (date.getHours() !== hour) throw label + " local hours";
  if (date.getMinutes() !== minute) throw label + " local minutes";
  if (date.getSeconds() !== second) throw label + " local seconds";
  if (date.getMilliseconds() !== ms) throw label + " local ms";
}
check(new Date(0), 1970, 0, 1, 4, 0, 0, 0, 0, "epoch");
check(new Date(-1), 1969, 11, 31, 3, 23, 59, 59, 999, "negative");
check(new Date(951868799999), 2000, 1, 29, 2, 23, 59, 59, 999, "leap boundary");
check(new Date(951868800000), 2000, 2, 1, 3, 0, 0, 0, 0, "march boundary");
var invalid = new Date(NaN);
if (invalid.getUTCMonth() === invalid.getUTCMonth()) throw "invalid utc month";
if (invalid.getDate() === invalid.getDate()) throw "invalid local date";
var threw = 0;
try { Date.prototype.getUTCMonth.call({}); } catch (e) { if (e.name === "TypeError") threw += 1; }
try { Date.prototype.getMilliseconds.call(null); } catch (e) { if (e.name === "TypeError") threw += 1; }
if (threw !== 2) throw "receiver TypeError";
function meta(fn, name) {
  if (fn.length !== 0) throw name + " length";
  if (fn.name !== name) throw name + " name";
  var constructThrew = 0;
  try { new fn(); } catch (e) { if (e.name === "TypeError") constructThrew = 1; }
  if (constructThrew !== 1) throw name + " construct";
}
meta(Date.prototype.getUTCFullYear, "getUTCFullYear");
meta(Date.prototype.getUTCMonth, "getUTCMonth");
meta(Date.prototype.getUTCDate, "getUTCDate");
meta(Date.prototype.getUTCDay, "getUTCDay");
meta(Date.prototype.getUTCHours, "getUTCHours");
meta(Date.prototype.getUTCMinutes, "getUTCMinutes");
meta(Date.prototype.getUTCSeconds, "getUTCSeconds");
meta(Date.prototype.getUTCMilliseconds, "getUTCMilliseconds");
meta(Date.prototype.getMonth, "getMonth");
meta(Date.prototype.getDate, "getDate");
meta(Date.prototype.getDay, "getDay");
meta(Date.prototype.getHours, "getHours");
meta(Date.prototype.getMinutes, "getMinutes");
meta(Date.prototype.getSeconds, "getSeconds");
meta(Date.prototype.getMilliseconds, "getMilliseconds");
262;
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("wasm backend should support Date component getters");
    assert!(outcome.note.contains("number(262"));
}

#[test]
fn wasm_backend_supports_date_component_setters() {
    let outcome = engine()
            .run_script(
                r#"
function same(a, b, label) {
  if (a.valueOf() !== b.valueOf()) throw label;
}
var d = new Date(0);
if (d.setUTCFullYear(2000, 1, 29) !== 951782400000) throw "setUTCFullYear return";
if (d.getUTCFullYear() !== 2000 || d.getUTCMonth() !== 1 || d.getUTCDate() !== 29) throw "setUTCFullYear value";
d = new Date(0);
if (d.setUTCMonth(12) !== 31536000000) throw "month overflow";
if (d.getUTCFullYear() !== 1971 || d.getUTCMonth() !== 0 || d.getUTCDate() !== 1) throw "month overflow value";
d = new Date(2000, 0, 1);
d.setUTCDate(0);
if (d.getUTCFullYear() !== 1999 || d.getUTCMonth() !== 11 || d.getUTCDate() !== 31) throw "date underflow";
d = new Date(0);
d.setUTCHours(1, 2, 3, 4);
if (d.getUTCHours() !== 1 || d.getUTCMinutes() !== 2 || d.getUTCSeconds() !== 3 || d.getUTCMilliseconds() !== 4) throw "setUTCHours";
d.setUTCMinutes(10);
if (d.getUTCHours() !== 1 || d.getUTCMinutes() !== 10 || d.getUTCSeconds() !== 3 || d.getUTCMilliseconds() !== 4) throw "setUTCMinutes default";
d.setUTCSeconds(60);
if (d.getUTCMinutes() !== 11 || d.getUTCSeconds() !== 0) throw "seconds overflow";
d.setUTCMilliseconds(-1);
if (d.getUTCSeconds() !== 59 || d.getUTCMilliseconds() !== 999) throw "ms underflow";

same(new Date(new Date(0).setFullYear(2001, 2, 4)), new Date(new Date(0).setUTCFullYear(2001, 2, 4)), "local full year");
same(new Date(new Date(0).setMonth(5, 6)), new Date(new Date(0).setUTCMonth(5, 6)), "local month");
same(new Date(new Date(0).setDate(7)), new Date(new Date(0).setUTCDate(7)), "local date");
same(new Date(new Date(0).setHours(8, 9, 10, 11)), new Date(new Date(0).setUTCHours(8, 9, 10, 11)), "local hours");
same(new Date(new Date(0).setMinutes(12, 13, 14)), new Date(new Date(0).setUTCMinutes(12, 13, 14)), "local minutes");
same(new Date(new Date(0).setSeconds(15, 16)), new Date(new Date(0).setUTCSeconds(15, 16)), "local seconds");
same(new Date(new Date(0).setMilliseconds(17)), new Date(new Date(0).setUTCMilliseconds(17)), "local ms");

d = new Date(NaN);
if (d.setUTCFullYear(2000) !== 946684800000) throw "invalid full year return";
if (d.getUTCFullYear() !== 2000 || d.getUTCMonth() !== 0 || d.getUTCDate() !== 1) throw "invalid full year value";
d = new Date(NaN);
if (d.setUTCMonth(0) === d.setUTCMonth(0)) throw "invalid month stays NaN";
if (d.valueOf() === d.valueOf()) throw "invalid month stored";

var order = "";
d = new Date(0);
try {
  Date.prototype.setUTCFullYear.call({}, { valueOf: function() { order += "x"; return 1; } });
} catch (e) { if (e.name === "TypeError") order += "t"; }
if (order !== "t") throw "receiver before coercion";

d = new Date(2000, 0, 31);
d.setUTCMonth({ valueOf: function() { d.setUTCDate(1); return 1; } });
if (d.getUTCFullYear() !== 2000 || d.getUTCMonth() !== 2 || d.getUTCDate() !== 2) throw "old value defaults";

order = "";
d = new Date(0);
d.setUTCHours(
  { valueOf: function() { order += "h"; return 1; } },
  { valueOf: function() { order += "m"; return 2; } },
  { valueOf: function() { order += "s"; return 3; } },
  { valueOf: function() { order += "n"; return 4; } }
);
if (order !== "hmsn") throw "coercion order";

var threw = 0;
try { Date.prototype.setUTCDate.call(null, 1); } catch (e) { if (e.name === "TypeError") threw += 1; }
try { new Date.prototype.setUTCSeconds(); } catch (e) { if (e.name === "TypeError") threw += 1; }
if (threw !== 2) throw "errors";
function meta(fn, name, length) {
  if (fn.length !== length) throw name + " length";
  if (fn.name !== name) throw name + " name";
  var desc = Object.getOwnPropertyDescriptor(Date.prototype, name);
  if (desc.enumerable || !desc.writable || !desc.configurable) throw name + " descriptor";
}
meta(Date.prototype.setUTCFullYear, "setUTCFullYear", 3);
meta(Date.prototype.setUTCMonth, "setUTCMonth", 2);
meta(Date.prototype.setUTCDate, "setUTCDate", 1);
meta(Date.prototype.setUTCHours, "setUTCHours", 4);
meta(Date.prototype.setUTCMinutes, "setUTCMinutes", 3);
meta(Date.prototype.setUTCSeconds, "setUTCSeconds", 2);
meta(Date.prototype.setUTCMilliseconds, "setUTCMilliseconds", 1);
meta(Date.prototype.setFullYear, "setFullYear", 3);
meta(Date.prototype.setMonth, "setMonth", 2);
meta(Date.prototype.setDate, "setDate", 1);
meta(Date.prototype.setHours, "setHours", 4);
meta(Date.prototype.setMinutes, "setMinutes", 3);
meta(Date.prototype.setSeconds, "setSeconds", 2);
meta(Date.prototype.setMilliseconds, "setMilliseconds", 1);
262;
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("wasm backend should support Date component setters");
    assert!(outcome.note.contains("number(262"));
}

#[test]
fn wasm_backend_supports_date_utc_and_decimal_exponent_to_number() {
    let outcome = engine()
            .run_script(
                r#"
function same(actual, expected, label) {
  if (actual !== expected) throw label;
}
same(Date.UTC(1970), 0, "year only");
same(Date.UTC(1970, 0, 1, 0, 0, 0, 1), 1, "full args");
same(Date.UTC(1970, 12, 1), 31536000000, "month overflow");
same(Date.UTC(1970, 0, 0), -86400000, "date underflow");
same(Date.UTC(0, 0, 1), -2208988800000, "year remap");
if (Date.UTC() === Date.UTC()) throw "missing year NaN";
if (Date.UTC(1970, NaN) === Date.UTC(1970, NaN)) throw "NaN arg";
same(Date.UTC(275760, 8, 13), 8640000000000000, "clip max");
if (Date.UTC(275760, 8, 14) === Date.UTC(275760, 8, 14)) throw "clip overflow";
var order = "";
Date.UTC({ valueOf: function() { order += "y"; return 1970; } }, { valueOf: function() { order += "m"; return 0; } });
if (order !== "ym") throw "coercion order";
var symbolThrew = 0;
try { Date.UTC(Symbol("x")); } catch (e) { symbolThrew = 1; }
if (symbolThrew !== 1) throw "symbol";
if (Date.UTC.length !== 7 || Date.UTC.name !== "UTC") throw "metadata";
var desc = Object.getOwnPropertyDescriptor(Date, "UTC");
if (desc.enumerable || !desc.writable || !desc.configurable) throw "descriptor";
var constructThrew = 0;
try { new Date.UTC(); } catch (e) { constructThrew = 1; }
if (constructThrew !== 1) throw "construct";

same(Number("   +00200.000E-0002\t"), 2, "trimmed exponent");
same(Number("1e3"), 1000, "positive exponent");
same(Number("1E-3"), 0.001, "negative exponent");
if (Number("not a number") === Number("not a number")) throw "malformed text";
if (Number("1e") === Number("1e")) throw "missing exponent digits";
if (Number("1e+") === Number("1e+")) throw "missing signed exponent digits";
if (Number("1e-") === Number("1e-")) throw "missing negative exponent digits";
if (Number("e1") === Number("e1")) throw "missing significand";
var d = new Date(0);
same(d.setTime("   +00200.000E-0002\t"), 2, "setTime exponent");
d = new Date(0);
same(d.setUTCMilliseconds("   +00200.000E-0002\t"), 2, "setter exponent");
d = new Date(0);
if (d.setYear("not a number") === d.setYear("not a number")) throw "setYear malformed return";
if (d.valueOf() === d.valueOf()) throw "setYear malformed stores NaN";
262;
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("wasm backend should support Date.UTC and decimal exponent ToNumber");
    assert!(outcome.note.contains("number(262"));
}

#[test]
fn wasm_backend_supports_remainder() {
    let outcome = engine()
        .run_script(
            "7 % 3;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should run remainder");
    assert!(outcome.note.contains("number(1"));
}

#[test]
fn wasm_backend_supports_assignment_and_if() {
    let outcome = engine()
        .run_script(
            "let x = 0; if (!x) { x = 5; } x;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should run assignment and if");
    assert!(outcome.note.contains("number(5"));
}

#[test]
fn wasm_backend_coercive_subtraction_preserves_dynamic_numeric_kind() {
    let outcome = engine()
            .run_script(
                "function subtract(left, right) { return left - right; } subtract(7, 2) === 5 && subtract(7n, 2n) === 5n;",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("wasm backend should subtract dynamic Number and BigInt arguments");
    assert!(
        outcome.note.contains("boolean(true"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_outlined_to_numeric_preserves_kind_order_and_abrupt_identity() {
    let source = r#"
            let order = "";
            const left = {
                [Symbol.toPrimitive]: function () {
                    order += "L";
                    return 2n;
                }
            };
            const right = {
                [Symbol.toPrimitive]: function () {
                    order += "R";
                    return 3n;
                }
            };
            const bigintResult = left ** right;

            let mixedThrows = false;
            try {
                left ** {
                    [Symbol.toPrimitive]: function () {
                        order += "M";
                        return 2;
                    }
                };
            } catch (error) {
                mixedThrows = true;
            }

            const marker = {};
            let preservesIdentity = false;
            try {
                ({
                    [Symbol.toPrimitive]: function () {
                        order += "T";
                        throw marker;
                    }
                }) ** {
                    [Symbol.toPrimitive]: function () {
                        order += "U";
                        return 1n;
                    }
                };
            } catch (error) {
                preservesIdentity = error === marker;
            }

            function exponentiate(base, exponent) {
                return base ** exponent;
            }
            order + "|" +
                bigintResult + "|" +
                mixedThrows + "|" +
                preservesIdentity + "|" +
                exponentiate(3, 2) + "|" +
                exponentiate(2n, 3n);
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
        .expect("outlined ToNumeric should preserve dynamic coercion semantics");
    assert!(
        outcome.note.contains("string(LRLMT|8|true|true|9|8"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_runs_repeated_dynamic_to_numeric_through_public_engine_path() {
    let mut source = String::from("let result;");
    for _ in 0..40 {
        source.push_str(
            r#"
                    result = ({
                        [Symbol.toPrimitive]: function () {
                            return 2n;
                        },
                        valueOf: function () {
                            throw new Error("unexpected valueOf");
                        },
                        toString: function () {
                            throw new Error("unexpected toString");
                        }
                    }) ** 1n;
                "#,
        );
    }
    source.push_str("result;");

    let outcome = engine()
        .run_script(
            &source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("repeated dynamic ToNumeric sites should compile and execute");
    assert!(outcome.note.contains("bigint(2n"), "note: {}", outcome.note);
}

#[test]
fn wasm_backend_supports_direct_recursion() {
    let outcome = engine()
        .run_script(
            "function up(n) { if (n === 0) { return 0; } return up(n - 1) + 1; } up(3);",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should run recursion");
    assert!(outcome.note.contains("number(3"));
}

#[test]
fn wasm_backend_supports_missing_heap_reads() {
    let object_outcome = engine()
        .run_script(
            "let o = {}; o.x;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should return undefined for missing property");
    assert!(object_outcome.note.contains("undefined("));

    let array_outcome = engine()
        .run_script(
            "let a = [1]; a[9];",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should return undefined for missing index");
    assert!(array_outcome.note.contains("undefined("));
}

#[test]
fn wasm_backend_supports_method_calls_and_this() {
    let method_outcome = engine()
        .run_script(
            "function inc(x) { return x + 1; } let o = { f: inc }; o.f(2);",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should run method call");
    assert!(method_outcome.note.contains("number(3"));

    let bracket_outcome = engine()
        .run_script(
            "function inc(x) { return x + 1; } let o = { f: inc }; o[\"f\"](2);",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should run bracket method call");
    assert!(bracket_outcome.note.contains("number(3"));

    let this_outcome = engine()
        .run_script(
            "function getX() { return this.x; } let o = { x: 3, f: getX }; o.f();",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should run method call with this");
    assert!(this_outcome.note.contains("number(3"));
}

#[test]
fn wasm_backend_evaluates_compound_method_receiver_once() {
    let source = r#"
            let baseCalls = 0;
            function makeReceiver() {
                baseCalls += 1;
                return {
                    value: baseCalls,
                    method: function () { return this.value; }
                };
            }
            let result = makeReceiver().method();
            baseCalls === 1 && result === 1;
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
        .expect("wasm backend should evaluate a compound method receiver once");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_loose_equality_evaluates_operands_once() {
    let source = r#"
            let calls = 0;
            let assigned;
            function nextValue() {
                calls += 1;
                return calls === 1 ? "report" : {};
            }
            let equalsNull = (assigned = nextValue()) == null;
            calls === 1 && assigned === "report" && equalsNull === false;
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
        .expect("wasm backend should evaluate loose equality operands once");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_loose_equality_matches_dynamic_null_and_undefined() {
    let source = r#"
            function nullable(useNull) {
                return useNull ? null : {};
            }
            function optional(useUndefined) {
                return useUndefined ? undefined : {};
            }
            nullable(true) == optional(true);
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
        .expect("wasm backend should loosely equate dynamic null and undefined");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_evaluates_method_base_key_and_arguments_in_order_once() {
    let source = r#"
            let order = "";
            let receiver;
            function makeReceiver() {
                order += "base";
                receiver = {
                    method: function (value) {
                        order += "call";
                        return this === receiver && value === "argument";
                    }
                };
                return receiver;
            }
            function methodKey() {
                order += ",key";
                return "method";
            }
            function argument() {
                order += ",argument";
                return "argument";
            }
            let result = makeReceiver()[methodKey()](argument());
            result && order === "base,key,argumentcall";
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
        .expect("wasm backend should preserve computed method call evaluation order");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_evaluates_no_method_arguments_after_abrupt_computed_key() {
    let source = r#"
            let order = "";
            let expected = new Error("key");
            function makeReceiver() {
                order += "base";
                return { method: function () {} };
            }
            function methodKey() {
                order += ",key";
                throw expected;
            }
            function argument() {
                order += ",argument";
            }
            let caught;
            try {
                makeReceiver()[methodKey()](argument());
            } catch (error) {
                caught = error;
            }
            caught === expected && order === "base,key";
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
        .expect("wasm backend should stop a method call after an abrupt computed key");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_grows_heap_beyond_old_fixed_cap() {
    let source = format!(
        "let o = {{}}; {} o.k64;",
        (0..65)
            .map(|index| format!("o[\"k{index}\"] = {index};"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let outcome = engine()
        .run_script(
            &source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should grow heap");
    assert!(outcome.note.contains("number(64"));
}

#[test]
fn wasm_backend_create_realm_allocates_distinct_global_and_intrinsics_per_realm() {
    let outcome = engine()
        .run_script(
            r#"
let first = __scriptwCreateRealm();
let second = __scriptwCreateRealm();
[
  first === second,
  first.global === second.global,
  first.global.Object === second.global.Object,
  first.global.Object.prototype === second.global.Object.prototype,
  Object.getPrototypeOf(first.global.Object) === Object.getPrototypeOf(second.global.Object)
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("each synthetic realm should allocate a distinct global and intrinsic graph");
    assert!(
        outcome
            .note
            .contains("string(false|false|false|false|false)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_create_realm_exposes_bigint_static_methods() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm().global;
[
  other.BigInt.asIntN === BigInt.asIntN,
  typeof other.BigInt.asIntN,
  typeof other.BigInt.asUintN,
  other.BigInt.asIntN(4, 15n).toString(),
  other.BigInt.asUintN(4, -1n).toString()
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("synthetic realm BigInt constructor should expose static methods");
    assert!(
        outcome
            .note
            .contains("string(false|function|function|-1|15)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_create_realm_exposes_number_static_methods() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm().global;
[
  other.Number.isInteger === Number.isInteger,
  typeof other.Number.isInteger,
  typeof other.Number.isSafeInteger,
  typeof other.Number.isFinite,
  typeof other.Number.isNaN,
  typeof other.Number.parseInt,
  typeof other.Number.parseFloat,
  other.Number.isInteger(1),
  other.Number.isInteger(1.5),
  other.Number.isSafeInteger(9007199254740991),
  other.Number.isSafeInteger(9007199254740992),
  other.Number.isFinite(3),
  other.Number.isFinite(Infinity),
  other.Number.isNaN(NaN),
  other.Number.isNaN("NaN"),
  other.Number.parseInt("ff", 16),
  other.Number.parseFloat("1.25x"),
  typeof other.parseInt,
  typeof other.parseFloat,
  other.parseInt("10", 2),
  other.parseFloat("1.5e1"),
  other.Number.MAX_SAFE_INTEGER,
  other.Number.MIN_SAFE_INTEGER,
  other.Number.POSITIVE_INFINITY === Infinity,
  other.Number.NEGATIVE_INFINITY === -Infinity,
  other.Number.EPSILON > 0 && other.Number.EPSILON < 1,
  other.Number.MIN_VALUE > 0,
  other.Number.MAX_VALUE > 1,
  other.Number.NaN === other.Number.NaN,
  Object.getOwnPropertyDescriptor(other.Number, "MAX_SAFE_INTEGER").writable,
  Object.getOwnPropertyDescriptor(other.Number, "MAX_SAFE_INTEGER").enumerable,
  Object.getOwnPropertyDescriptor(other.Number, "MAX_SAFE_INTEGER").configurable
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("synthetic realm Number constructor should expose static methods");
    assert!(
            outcome.note.contains(
                "string(false|function|function|function|function|function|function|true|false|true|false|true|false|true|false|255|1.25|function|function|2|15|9007199254740991|-9007199254740991|true|true|true|true|true|false|false|false|false)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_create_realm_exposes_error_static_methods() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm().global;
let error = new other.Error("m");
let typeError = new other.TypeError("m");
[
  other.Error.isError === Error.isError,
  typeof other.Error.isError,
  other.Error.isError(error),
  other.Error.isError(typeError),
  other.Error.isError(new Error("m")),
  other.Error.isError({}),
  other.Error.isError("m")
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("synthetic realm Error constructor should expose static methods");
    assert!(
        outcome
            .note
            .contains("string(false|function|true|true|true|false|false)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_cross_realm_stored_builtin_retains_defining_realm() {
    let outcome = engine()
        .run_script(
            r#"
let other = __scriptwCreateRealm();
let method = other.global.String.prototype.toString;
let holder = { method: method };
let array = [method];
let results = [];

for (let i = 0; i < 2; i++) {
  let stored = i === 0 ? holder.method : array[0];
  try {
    stored.call(null);
    results.push("missing");
  } catch (error) {
    results.push(
      (Object.getPrototypeOf(error) === other.global.TypeError.prototype) + ":" +
      (error instanceof other.global.TypeError) + ":" +
      (error instanceof TypeError)
    );
  }
}

results.join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("stored cross-realm builtins should retain their defining realm");
    assert!(
        outcome
            .note
            .contains("string(true:true:false|true:true:false)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_let_binding_tracks_tag_after_cross_kind_assignment() {
    let outcome = engine()
        .run_script(
            r#"
let value = null;
value = { nested: { answer: 42 } };
value.nested.answer;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("mutable lexical bindings should retain the assigned runtime tag");
    assert!(
        outcome.note.contains("number(42)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_distinguishes_ordinary_and_optional_computed_nullish_order() {
    let outcome = engine()
        .run_script(
            r#"
let keyCalls = 0;
function key() { keyCalls += 1; return "value"; }
let caughtTypeError = false;
try {
  null[key()];
} catch (error) {
  caughtTypeError = error instanceof TypeError;
}
let skipped = null?.[key()];
let tailCaughtTypeError = false;
try {
  ({ value: null })?.value[key()];
} catch (error) {
  tailCaughtTypeError = error instanceof TypeError;
}
[caughtTypeError, skipped === undefined, tailCaughtTypeError, keyCalls].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("ordinary and optional nullish property access should preserve key ordering");
    assert!(
        outcome.note.contains("string(true|true|true|2)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_tail_calls_tagged_templates_without_growing_the_wasm_stack() {
    let outcome = engine()
        .run_script(
            r#"
"use strict";
function recurse(_, remaining) {
  if (remaining === 0) return "finished";
  return recurse`${remaining - 1}`;
}
recurse(null, 20000);
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("tail-position tagged calls should not exhaust the Wasm stack");
    assert!(
        outcome.note.contains("string(finished)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_tail_calls_nested_return_expressions_without_growing_the_wasm_stack() {
    let outcome = engine()
        .run_script(
            r#"
"use strict";
function conditionalComma(remaining) {
  return remaining === 0 ? "conditional" : (0, conditionalComma(remaining - 1));
}
function logicalAnd(remaining) {
  return remaining !== 0 && logicalAnd(remaining - 1);
}
function logicalOr(remaining) {
  return remaining === 0 || logicalOr(remaining - 1);
}
function coalesce(remaining) {
  return (remaining === 0 ? "coalesce" : null) ?? coalesce(remaining - 1);
}
[
  conditionalComma(20000),
  logicalAnd(20000),
  logicalOr(20000),
  coalesce(20000)
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("nested return-position calls should not exhaust the Wasm stack");
    assert!(
        outcome
            .note
            .contains("string(conditional|false|true|coalesce)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_numeric_update_coerces_captured_boolean_binding() {
    let outcome = engine()
        .run_script(
            "let value = true; function increment() { return ++value; } increment();",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("numeric update should coerce the captured boolean binding");
    assert!(outcome.note.contains("number(2"), "note: {}", outcome.note);
}

#[test]
fn wasm_backend_numeric_update_preserves_dynamic_bigint() {
    let outcome = engine()
            .run_script(
                "function increment(value) { let previous = value++; return previous === 4n && value === 5n; } increment({ value: 4n }.value);",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("numeric update should preserve a dynamic BigInt binding");
    assert!(
        outcome.note.contains("boolean(true"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_supports_arrow_lexical_this() {
    let outcome = engine()
            .run_script(
                "function make() { return () => this.x; } let o = { x: 3, f: make }; let g = o.f(); g();",
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("arrow lexical this should run");
    assert!(outcome.note.contains("number(3"));
}

#[test]
fn wasm_backend_supports_default_rest_and_arguments_core() {
    for (source, expected, label) in [
        (
            "function f(x = 1) { return x; } f();",
            "number(1",
            "default param basic",
        ),
        (
            "function f(x, y = x + 1) { return y; } f(2);",
            "number(3",
            "default param from earlier param",
        ),
        (
            "let f = (x = 1) => x + 1; f();",
            "number(2",
            "arrow default param",
        ),
        (
            "function third(...xs) { return xs[2]; } third(1, 2, 3);",
            "number(3",
            "rest param element",
        ),
        (
            "function len(...xs) { return xs.length; } len(1, 2, 3);",
            "number(3",
            "rest param length",
        ),
        (
            "function f(a, b) { return arguments.length; } f(1, 2, 3);",
            "number(3",
            "arguments length",
        ),
        (
            "function f() { return arguments[1]; } f(1, 2, 3);",
            "number(2",
            "arguments indexed read",
        ),
        (
            "function f(x) { arguments[0] = 3; return x; } f(1);",
            "number(3",
            "mapped arguments write to param",
        ),
        (
            "function f(x) { x = 4; return arguments[0]; } f(1);",
            "number(4",
            "mapped param write to arguments",
        ),
        (
            "function f() { arguments.foo = 10; return arguments.foo; } f();",
            "number(10",
            "arguments named property write",
        ),
        (
            "function f(x, x, x) { var before = arguments[0] + ',' + arguments[1] + ',' + arguments[2]; x = 4; return before + '|' + arguments[0] + ',' + arguments[1] + ',' + arguments[2]; } f(1, 2, 3);",
            "string(1,2,3|1,2,4)",
            "only the last duplicate parameter maps to arguments",
        ),
        (
            "function f(x = 1) { arguments[0] = 3; return x; } f(2);",
            "number(2",
            "unmapped default param arguments",
        ),
        (
            "function f(...xs) { arguments[0] = 9; return xs[0]; } f(1, 2);",
            "number(1",
            "unmapped rest arguments",
        ),
        (
            "function outer() { return (() => arguments[0])(); } outer(3);",
            "number(3",
            "arrow lexical arguments",
        ),
        (
            "let o = { x: 2, f(y = this.x) { return y; } }; o.f();",
            "number(2",
            "method default with this",
        ),
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
            .unwrap_or_else(|_| panic!("{label} should run"));
        assert!(
            outcome.note.contains(expected),
            "{label} produced unexpected note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_parameter_tdz_getter_grammar_and_setter_defaults_follow_spec() {
    for source in [
        "function f(x = y, y = 1) { return x; } f();",
        "function f(x = x) { return x; } f();",
    ] {
        let error = engine()
            .run_script(
                source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect_err(&format!("parameter TDZ must throw for `{source}`"));
        assert_eq!(
            error.wasm_javascript_exception_constructor_name(),
            Some("ReferenceError"),
            "source: {source}, error: {error}"
        );
    }

    let getter_error = engine()
        .run_script(
            "({ get x(a) { return a; } }).x;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect_err("a getter parameter must be rejected before execution");
    let diagnostic = getter_error
        .parse_diagnostic()
        .expect("the getter grammar error must retain its front-end diagnostic");
    assert_eq!(diagnostic.error_type(), Some("SyntaxError"));

    let setter_source = r#"
            const holder = { set x(value = 1) { this.stored = value; } };
            holder.x = 7;
            const supplied = holder.stored;
            holder.x = undefined;
            supplied === 7 && holder.stored === 1;
        "#;
    let setter = engine()
        .run_script(
            setter_source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|error| panic!("default setter must execute: {error}"));
    assert!(setter.note.contains("boolean(true)"), "{}", setter.note);
}

#[test]
fn wasm_backend_supports_arguments_callee_identity() {
    let outcome = engine()
        .run_script(
            "function f() { return arguments.callee === f; } f();",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("arguments.callee should run");

    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_supports_host_print_global() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    for source in [
        "print(\"grug\")",
        "globalThis.print(\"grug\")",
        "let p = print; p(\"grug\")",
        "let o = { f: print }; o.f(\"grug\")",
        "function f() { print(\"x\"); } f()",
    ] {
        let outcome = engine_with_captured_prints(Arc::clone(&lines))
            .run_script(
                source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("host print should run");
        assert!(
            outcome.note.contains("undefined"),
            "source: {source}, note: {}",
            outcome.note
        );
    }
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &[
            "grug".to_string(),
            "grug".to_string(),
            "grug".to_string(),
            "grug".to_string(),
            "x".to_string(),
        ]
    );
}

#[test]
fn wasm_backend_supports_sloppy_global_name_resolution() {
    for (source, expected, label) in [
        (
            "globalThis.x = 1; x;",
            "number(1",
            "read after globalThis write",
        ),
        (
            "missing = 1; globalThis.missing;",
            "number(1",
            "implicit global create",
        ),
        (
            "function f() { return x; } globalThis.x = 2; f();",
            "number(2",
            "function global read",
        ),
        (
            "function f() { y = 3; } f(); globalThis.y;",
            "number(3",
            "function implicit global write",
        ),
        (
            "let x = 1; globalThis.x = 2; x;",
            "number(1",
            "lexical shadows global",
        ),
        (
            "function f() { return () => z; } z = 4; f()();",
            "number(4",
            "closure global read",
        ),
        ("x = 1; x++; x;", "number(2", "global numeric update"),
        (
            "globalThis.x = 1; x += 2; x;",
            "number(3",
            "global compound assign",
        ),
        (
            "if (true) { globalThis.x = 1; } else {} x",
            "number(1",
            "global read after conditional write",
        ),
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
            .unwrap_or_else(|_| panic!("{label} should run"));
        assert!(
            outcome.note.contains(expected),
            "{label} produced unexpected note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_destructuring_identifier_writes_preserve_reference_semantics() {
    let source = r#"
let defaultEvaluations = 0;
let tdzThrew = false;
try {
  {
    [later = (defaultEvaluations = defaultEvaluations + 1)] = [undefined];
    let later;
  }
} catch (error) {
  tdzThrew = error instanceof ReferenceError;
}

let strictThrew = false;
(function () {
  "use strict";
  try {
    [strictMissing] = [1];
  } catch (error) {
    strictThrew = error instanceof ReferenceError;
  }
})();

const fixed = 0;
let immutableThrew = false;
try {
  [fixed] = [1];
} catch (error) {
  immutableThrew = error instanceof TypeError;
}

[sloppyCreated] = [7];
defaultEvaluations === 1
  && tdzThrew
  && strictThrew
  && immutableThrew
  && globalThis.sloppyCreated === 7;
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
        .expect("destructuring writes should consume their typed references at PutValue");
    assert!(
        outcome.note.contains("boolean(true)"),
        "unexpected result: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_rejects_remaining_sloppy_global_tails() {
    for source in [
        "x",
        "function f() { return q; } f()",
        "topLevel = arguments",
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
            .expect_err("unsupported sloppy global seam should stay unsupported");
        let message = err.message();
        assert!(
            message.contains("unsupported in scriptw wasm-aot first slice")
                || message.contains("ReferenceError"),
            "source: {source}, err: {message}"
        );
    }
}

#[test]
fn wasm_backend_supports_update_and_compound_assignment() {
    let outcome = engine()
        .run_script(
            "let sum = 0; for (let i = 0; i < 4; i++) { sum += i; } sum;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should run updates and compound assignment");
    assert!(outcome.note.contains("number(6"));
}

#[test]
fn wasm_backend_preserves_postfix_result() {
    let outcome = engine()
        .run_script(
            "let i = 2; let x = i++; x + i;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should preserve postfix value");
    assert!(outcome.note.contains("number(5"));
}

#[test]
fn wasm_backend_supports_hoisted_var() {
    let outcome = engine()
        .run_script(
            "x; var x = 1;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("wasm backend should support hoisted var");
    assert!(outcome.note.contains("undefined("));
}

#[test]
fn wasm_backend_supports_mixed_logical_and_nullish() {
    for (source, expected) in [
        ("let x = 0; x || \"fallback\";", "string(fallback)"),
        ("let x = 1; x || \"fallback\";", "number(1"),
        ("let x = null; x ?? 3;", "number(3"),
        ("let x = 0; x ?? 3;", "number(0"),
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
            .expect("logical or nullish op should run");
        assert!(outcome.note.contains(expected));
    }
}

#[test]
fn wasm_backend_supports_typeof_core() {
    for (source, expected) in [
        ("typeof 1;", "string(number)"),
        ("typeof \"x\";", "string(string)"),
        ("typeof undefined;", "string(undefined)"),
        ("function f() {} typeof f;", "string(function)"),
        ("typeof missingName;", "string(undefined)"),
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
            .expect("typeof should run");
        assert!(outcome.note.contains(expected));
    }
}

#[test]
fn wasm_backend_supports_primitive_coercion_core() {
    for (source, expected) in [
        ("1 == \"1\";", "boolean(true)"),
        ("0 == false;", "boolean(true)"),
        ("null == undefined;", "boolean(true)"),
        ("1 != \"2\";", "boolean(true)"),
        ("\"2\" - 1;", "number(1"),
        ("true + 2;", "number(3"),
        ("null + 1;", "number(1"),
        ("\"6\" / \"2\";", "number(3"),
        ("\"10\" > \"2\";", "boolean(false)"),
        ("\"2\" < 3;", "boolean(true)"),
        ("void 1;", "undefined"),
        ("let x = 1; void (x = 3); x;", "number(3"),
        ("(1, 2);", "number(2"),
        ("let x = 0; (x = 1, x + 2);", "number(3"),
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
            .expect("primitive coercion core should run");
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_supports_heap_coercion_core() {
    for (source, expected) in [
        ("\"a\" + {};", "string(a[object Object])"),
        ("let o = {}; o + \"x\";", "string([object Object]x)"),
        ("let o = { valueOf() { return 2; } }; o + 1;", "number(3"),
        (
            "let o = { toString() { return \"x\"; } }; o + 1;",
            "string(x1)",
        ),
        ("[] + 1;", "string(1)"),
        ("[1, 2] + 3;", "string(1,23)"),
        ("let o = {}; o == \"[object Object]\";", "boolean(true)"),
        (
            "let o = { valueOf() { return 2; } }; o == \"2\";",
            "boolean(true)",
        ),
        ("let o = {}; o == o;", "boolean(true)"),
        ("[2] < 3;", "boolean(true)"),
        (
            "function f() { return arguments + \"\"; } f(1, 2);",
            "string([object Arguments])",
        ),
        ("let f = function() {}; \"x\" + f;", "string(xfunction("),
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
            .expect("heap coercion should run");
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_rejects_remaining_out_of_slice_heap_coercions() {
    for source in ["let o = { toString() { return function() {}; } }; \"\" + o;"] {
        let err = engine()
            .run_script(
                source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect_err("out-of-slice dynamic primitive op should stay unsupported");
        assert!(!err.message().trim().is_empty(), "source: {source}");
    }
}

#[test]
fn wasm_backend_supports_new_and_instanceof_core() {
    for (source, expected) in [
        (
            "function F() {} let x = new F(); x instanceof F;",
            "boolean(true)",
        ),
        (
            "function F() { this.x = 3; } let x = new F(); x.x;",
            "number(3)",
        ),
        (
            "function F() {} F.prototype.getX = function () { return this.x; }; let x = new F(); x.x = 4; x.getX();",
            "number(4)",
        ),
        (
            "function F() {} F.prototype = { x: 7 }; let x = new F(); x.x;",
            "number(7)",
        ),
        (
            "function F() { this.x = 1; return 2; } let x = new F(); x.x;",
            "number(1)",
        ),
        (
            "function F() { this.x = 1; return { y: 2 }; } let x = new F(); x.y;",
            "number(2)",
        ),
        (
            "function make(v) { return function F() { this.x = v; }; } let F = make(5); let x = new F(); x.x;",
            "number(5)",
        ),
        (
            "function F() {} function G() {} let x = new F(); x instanceof G;",
            "boolean(false)",
        ),
        (
            "function F() {} let rhs; if (true) { rhs = F; } else { rhs = print; } ({} instanceof rhs);",
            "boolean(false)",
        ),
        (
            "class C { constructor() { this.x = 1; } } let c = new C(); c.x;",
            "number(1)",
        ),
        (
            "let C = class { constructor(v) { this.x = v; } }; new C(2).x;",
            "number(2)",
        ),
        (
            "class C { x = 1; static y = 2; } let c = new C(); c.x + C.y;",
            "number(3)",
        ),
        (
            "class C { static x = 1; static { this.y = this.x + 1; } } C.y;",
            "number(2)",
        ),
        ("class C { m() { return 1; } } new C().m();", "number(1)"),
        ("class C { get x() { return 3; } } new C().x;", "number(3)"),
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
            .unwrap_or_else(|err| panic!("constructor core should run for `{source}`: {err:?}"));
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_rejects_top_level_new_target() {
    let err = engine()
        .run_script(
            "new.target;",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect_err("top-level new.target should be rejected");
    let message = err.message();
    assert!(
        message.contains("unsupported in scriptw wasm-aot first slice")
            || message.contains("parse error")
            || message.contains("TypeError"),
        "err: {message}"
    );
}

#[test]
fn wasm_backend_supports_phase_twenty_three_exception_core() {
    for (source, expected) in [
        (
            "var x = 0; try { x = 1; } catch (e) { x = 2; } x;",
            "number(1)",
        ),
        ("try { throw 1; } catch (e) { e; }", "number(1)"),
        (
            "var x; try { throw 1; } catch (e) { x = e; } x;",
            "number(1)",
        ),
        (
            "try { throw \"x\"; } catch (e) { e === \"x\"; }",
            "boolean(true)",
        ),
        (
            "try { throw undefined; } catch (e) { e; }",
            "undefined(undefined)",
        ),
        (
            "var arr = []; Object.defineProperty(arr, \"1\", { get: function() { return 42; }, configurable: true }); [arr[1]].join(\"|\");",
            "string(42)",
        ),
        (
            "var arr = []; Object.defineProperty(arr, \"1\", { get: function() { throw new RangeError(\"x\"); }, configurable: true }); try { arr[1]; } catch (e) { e.name; }",
            "string(RangeError)",
        ),
        (
            "var arr = []; Object.defineProperty(arr, \"1\", { get: function() { throw new RangeError(\"x\"); }, configurable: true }); try { arr.map(function(v) { return v; }); } catch (e) { e.name; }",
            "string(RangeError)",
        ),
        (
            "var obj = { 0: 11, length: 2 }; Object.defineProperty(obj, \"1\", { get: function() { throw new RangeError(\"x\"); }, configurable: true }); try { Array.prototype.map.call(obj, function(v) { return v; }); } catch (e) { e.name; }",
            "string(RangeError)",
        ),
        (
            "try { let x = 1; class C {} throw x; } catch (e) { e; }",
            "number(1)",
        ),
        (
            "try { let x = 1; class C { constructor() { this.x = 2; } } throw new C().x; } catch (e) { e; }",
            "number(2)",
        ),
        (
            "try { if (true) { let x = 3; class C {} throw x; } } catch (e) { e; }",
            "number(3)",
        ),
        (
            "try { label: { if (true) { class C {} throw 4; } } } catch (e) { e; }",
            "number(4)",
        ),
        (
            "try { while (true) { class C {} throw 5; } } catch (e) { e; }",
            "number(5)",
        ),
        (
            "try { do { let x = 7; class C {} throw x; } while (false); } catch (e) { e; }",
            "number(7)",
        ),
        (
            "try { for (let i = 0; i < 1; i = i + 1) { let x = 8; class C {} throw x; } } catch (e) { e; }",
            "number(8)",
        ),
        (
            "try { for (let x of [9]) { class C {} throw x; } } catch (e) { e; }",
            "number(9)",
        ),
        (
            "try { switch (1) { case 1: class C {} throw 6; } } catch (e) { e; }",
            "number(6)",
        ),
        (
            "try { class C {} C(); } catch (e) { e.name; }",
            "string(TypeError)",
        ),
        (
            "try { class C { #x = 1; read(obj) { return obj.#x; } } new C().read({}); } catch (e) { e.name; }",
            "string(TypeError)",
        ),
        (
            "class A {} class B extends A { constructor() { return { ok: 1 }; } } new B().ok;",
            "number(1)",
        ),
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
            .unwrap_or_else(|err| panic!("phase 23 case should run for `{source}`: {err:?}"));
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_supports_phase_twenty_four_abrupt_core() {
    for (source, expected) in [
        ("try { 1; } finally {}", "number(1)"),
        (
            "function f() { try { return 1; } finally {} } f();",
            "number(1)",
        ),
        (
            "var x = 0; try { throw 1; } catch (e) { x = 1; } finally { x = x + 1; } x;",
            "number(2)",
        ),
        (
            "var x = 0; try { let a = 1; class C {} x = a; } finally { let b = 2; class D {} x = x + b; } x;",
            "number(3)",
        ),
        (
            "var x = 0; try { throw 1; } catch (e) { let a = e; class C {} x = a; } finally { let b = 2; class D {} x = x + b; } x;",
            "number(3)",
        ),
        (
            "function f() { let x = 0; while (true) { try { break; } finally { x = 1; } } return x; } f();",
            "number(1)",
        ),
        (
            "let o = { x: 1 }; delete o.x; \"x\" in o;",
            "boolean(false)",
        ),
        ("let a = [1, 2]; delete a[0]; (0 in a);", "boolean(false)"),
        ("let a = [1, 2]; delete a[0]; a.length;", "number(2)"),
        ("\"x\" in { x: 1 }", "boolean(true)"),
        ("function F() {} \"prototype\" in F;", "boolean(true)"),
        (
            "function f() { return new.target; } f();",
            "undefined(undefined)",
        ),
        (
            "function F() { this.kind = typeof new.target; this.arrowKind = (() => typeof new.target)(); } let x = new F(); x.kind === \"function\" && x.arrowKind === \"function\";",
            "boolean(true)",
        ),
        (
            "class A { constructor() { this.kind = typeof new.target; } } class B extends A { constructor() { super(); } } new B().kind;",
            "string(function)",
        ),
        (
            "var x; try { \"x\" in 1; } catch (e) { x = e.name; } x;",
            "string(TypeError)",
        ),
        (
            "try { class A {} class B extends A { constructor() { this.x = 1; super(); } } new B(); } catch (e) { e.name; }",
            "string(ReferenceError)",
        ),
        (
            "try { class A {} class B extends A { constructor() { this.x = 1; super(); } } new B(); } catch (e) { e; }",
            "object",
        ),
        (
            "try { class A {} class B extends A { constructor() {} } new B(); } catch (e) { e.name; }",
            "string(ReferenceError)",
        ),
        (
            "try { class A {} class B extends A { constructor() {} } new B(); } catch (e) { e; }",
            "object",
        ),
        (
            "try { let marker = 1; class A {} class B extends A { constructor() {} } new B(); marker; } catch (e) { e.name; }",
            "string(ReferenceError)",
        ),
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
            .unwrap_or_else(|err| panic!("phase 24 case should run for `{source}`: {err:?}"));
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_supports_phase_thirty_null_heritage_classes() {
    for (source, expected) in [
        (
            "class C extends null { constructor() { return Object.create(new.target.prototype); } } let x = new C(); Object.getPrototypeOf(x) === C.prototype;",
            "boolean(true)",
        ),
        (
            "class C extends null { constructor() { return Object.create(new.target.prototype); } } new C() instanceof C;",
            "boolean(true)",
        ),
        (
            "class C extends null { constructor() { return Object.create(new.target.prototype); } } Object.getPrototypeOf(C.prototype) === null;",
            "boolean(true)",
        ),
        (
            "class C extends null { m() { return 1; } constructor() { return Object.create(new.target.prototype); } } new C().m();",
            "number(1)",
        ),
        (
            "let C = class extends null { constructor() { return Object.create(new.target.prototype); } }; new C() instanceof C;",
            "boolean(true)",
        ),
        (
            "class C extends null { x = 1; constructor() { return Object.create(new.target.prototype); } } new C().x;",
            "undefined(undefined)",
        ),
        (
            "try { class C extends null {} new C(); } catch (e) { e.name; }",
            "string(TypeError)",
        ),
        (
            "try { class C extends null { constructor() { super(); } } new C(); } catch (e) { e.name; }",
            "string(TypeError)",
        ),
        (
            "try { class C extends null { constructor() { return undefined; } } new C(); } catch (e) { e.name; }",
            "string(ReferenceError)",
        ),
        (
            "try { class C extends null { m() { return super.x; } constructor() { return Object.create(new.target.prototype); } } new C().m(); } catch (e) { e.name; }",
            "string(TypeError)",
        ),
        (
            "try { class C extends null { #x = 1; read() { return this.#x; } constructor() { return Object.create(new.target.prototype); } } new C().read(); } catch (e) { e.name; }",
            "string(TypeError)",
        ),
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
            .unwrap_or_else(|err| panic!("phase 30 case should run for `{source}`: {err:?}"));
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_supports_phase_twenty_six_builtin_globals() {
    for (source, expected) in [
        ("typeof Function;", "string(function)"),
        ("Function === globalThis.Function;", "boolean(true)"),
        ("function f() {} f instanceof Function;", "boolean(true)"),
        ("class C {} C instanceof Function;", "boolean(true)"),
        (
            "Object.getPrototypeOf(function f(){}) === Function.prototype;",
            "boolean(true)",
        ),
        ("Error(\"x\").message;", "string(x)"),
        ("new Error(\"x\") instanceof Error;", "boolean(true)"),
        ("RangeError(\"x\").name;", "string(RangeError)"),
        ("new RangeError(\"x\") instanceof Error;", "boolean(true)"),
        ("new SyntaxError(\"x\") instanceof Error;", "boolean(true)"),
        ("new EvalError(\"x\").name;", "string(EvalError)"),
        ("new URIError(\"x\").message;", "string(x)"),
        ("new TypeError(\"x\") instanceof Error;", "boolean(true)"),
        ("new ReferenceError(\"x\").name;", "string(ReferenceError)"),
        (
            "try { \"x\" in 1; } catch (e) { e instanceof TypeError; }",
            "boolean(true)",
        ),
        (
            "try { class C {} C(); } catch (e) { e instanceof TypeError; }",
            "boolean(true)",
        ),
        ("Object.create({ x: 1 }).x;", "number(1)"),
        (
            "let p = { x: 1 }; let o = Object.create(p); Object.getPrototypeOf(o) === p;",
            "boolean(true)",
        ),
        ("({}) instanceof Object;", "boolean(true)"),
        ("[] instanceof Array;", "boolean(true)"),
        ("Array.isArray([]);", "boolean(true)"),
        ("Array.isArray({});", "boolean(false)"),
        ("Array(1, 2)[1];", "number(2)"),
        ("new Array(\"x\")[0];", "string(x)"),
        ("let o = {}; Object(o) === o;", "boolean(true)"),
        (
            "function add(x, y) { return x + y; } add.call(null, 1, 2);",
            "number(3)",
        ),
        (
            "function f(x) { return this.v + x; } let o = { v: 2 }; f.call(o, 3);",
            "number(5)",
        ),
        (
            "function add(x, y) { return x + y; } add.apply(null, [1, 2]);",
            "number(3)",
        ),
        (
            "function pick() { return arguments[1]; } pick.apply(null, [1, 2, 3]);",
            "number(2)",
        ),
        (
            "function f() { return this.x; } let o = { x: 4 }; f.apply(o, []);",
            "number(4)",
        ),
        (
            "AggregateError([1, 2], \"x\").name;",
            "string(AggregateError)",
        ),
        (
            "new AggregateError([1, 2], \"x\") instanceof Error;",
            "boolean(true)",
        ),
        (
            "new AggregateError([1, 2], \"x\") instanceof AggregateError;",
            "boolean(true)",
        ),
        (
            "let e = AggregateError([1, undefined, 3], \"x\"); e.errors[1];",
            "undefined(undefined)",
        ),
        (
            "let cause = { marker: 1 }; new Error(\"m\", { cause: cause }).cause === cause;",
            "boolean(true)",
        ),
        (
            "let cause = { marker: 1 }; new AggregateError([], \"m\", { cause: cause }).cause === cause;",
            "boolean(true)",
        ),
        (
            "Object.prototype.hasOwnProperty.call(new AggregateError([], \"m\"), \"cause\");",
            "boolean(false)",
        ),
        ("AggregateError.length;", "number(2)"),
        (
            "Object.getPrototypeOf(AggregateError) === Error;",
            "boolean(true)",
        ),
        (
            "AggregateError.prototype.constructor === AggregateError;",
            "boolean(true)",
        ),
        (
            "AggregateError === globalThis.AggregateError;",
            "boolean(true)",
        ),
        ("AggregateError instanceof Function;", "boolean(true)"),
        (
            "class C {} try { C.call({}); } catch (e) { e instanceof TypeError; }",
            "boolean(true)",
        ),
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
                panic!("phase 26 builtin case should run for `{source}`: {err:?}")
            });
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_compares_multi_limb_bigint_literals_by_value() {
    let source = r#"
const positive = 340282366920938463463374607431768211456n;
const negative = -340282366920938463463374607431768211456n;
(positive === 340282366920938463463374607431768211456n) + "|" +
  (positive !== 340282366920938463463374607431768211457n) + "|" +
  (negative === -340282366920938463463374607431768211456n) + "|" +
  (positive === negative) + "|" +
  (positive == 340282366920938463463374607431768211456n) + "|" +
  (positive != 340282366920938463463374607431768211457n) + "|" +
  (typeof positive) + "|" + (!!positive);
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
        .expect("multi-limb BigInt literals should preserve mathematical identity");
    assert!(
        outcome
            .note
            .contains("string(true|true|true|false|true|true|bigint|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_atomics_add_returns_old_values_and_updates_shared_integer_views() {
    let source = r#"
const i8 = new Int8Array(new SharedArrayBuffer(1));
i8[0] = -5;
const oldI8 = Atomics.add(i8, 0, 10);
const u8 = new Uint8Array(new SharedArrayBuffer(1));
u8[0] = 250;
const oldU8 = Atomics.add(u8, 0, 10);
const i16 = new Int16Array(new SharedArrayBuffer(2));
i16[0] = -32000;
const oldI16 = Atomics.add(i16, 0, 1000);
const u16 = new Uint16Array(new SharedArrayBuffer(2));
u16[0] = 65000;
const oldU16 = Atomics.add(u16, 0, 1000);
const i32 = new Int32Array(new SharedArrayBuffer(4));
i32[0] = 2147483647;
const oldI32 = Atomics.add(i32, 0, 1);
const u32 = new Uint32Array(new SharedArrayBuffer(4));
u32[0] = 123456789;
const oldU32 = Atomics.add(u32, 0, 2);
const i64 = new BigInt64Array(new SharedArrayBuffer(8));
i64[0] = -5n;
const oldI64 = Atomics.add(i64, 0, 12n);
const u64 = new BigUint64Array(new SharedArrayBuffer(8));
u64[0] = 123456789n;
const oldU64 = Atomics.add(u64, 0, 2n);

oldI8 + ":" + i8[0] + "|" +
oldU8 + ":" + u8[0] + "|" +
oldI16 + ":" + i16[0] + "|" +
oldU16 + ":" + u16[0] + "|" +
oldI32 + ":" + i32[0] + "|" +
oldU32 + ":" + u32[0] + "|" +
oldI64 + ":" + i64[0] + "|" +
oldU64 + ":" + u64[0];
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
        .expect("Atomics.add should update shared integer typed arrays");
    assert!(
            outcome.note.contains(
                "string(-5:5|250:4|-32000:-31000|65000:464|2147483647:-2147483648|123456789:123456791|-5:7|123456789:123456791)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_atomics_add_rejects_out_of_bounds_indices() {
    let source = r#"
const ints = new Int32Array(new SharedArrayBuffer(8));
const bigs = new BigInt64Array(new SharedArrayBuffer(16));
const errors = [];
try {
  Atomics.add(ints, -1, 1);
} catch (error) {
  errors.push(error.name);
}
try {
  Atomics.add(bigs, bigs.length, 1n);
} catch (error) {
  errors.push(error.name);
}
errors.join("|");
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
        .expect("Atomics.add should reject out-of-bounds indices");
    assert!(
        outcome.note.contains("string(RangeError|RangeError)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_atomics_load_and_store_preserve_integer_values_and_validation_order() {
    let source = r#"
const i8 = new Int8Array(new ArrayBuffer(1));
i8[0] = -1;
const u32 = new Uint32Array(new SharedArrayBuffer(4));
u32[0] = 4294967295;
const i64 = new BigInt64Array(new SharedArrayBuffer(8));
i64[0] = -7n;

const stored = Atomics.store(u32, 0, Math.PI);
const objectStored = Atomics.store(u32, 0, { valueOf() { return 33; } });
const negativeStored = Atomics.store(i8, 0, -5);
const negativeZero = Atomics.store(i8, 0, -0);

let indexCoerced = false;
const poisonedIndex = { valueOf() { indexCoerced = true; return 0; } };
let loadTypeError = false;
try {
  Atomics.load(new Float32Array(new SharedArrayBuffer(8)), poisonedIndex);
} catch (error) {
  loadTypeError = error instanceof TypeError;
}

let valueCoerced = false;
const poisonedValue = { valueOf() { valueCoerced = true; return 1; } };
let storeTypeError = false;
try {
  Atomics.store(new Uint8ClampedArray(new SharedArrayBuffer(8)), 0, poisonedValue);
} catch (error) {
  storeTypeError = error instanceof TypeError;
}

Atomics.load(i8, 0) + "|" + Atomics.load(u32, 0) + "|" + Atomics.load(i64, 0) + "|" +
  stored + "|" + objectStored + "|" + negativeStored + "|" + Object.is(negativeZero, 0) + "|" +
  loadTypeError + "|" + indexCoerced + "|" + storeTypeError + "|" + valueCoerced;
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
        .expect("Atomics.load and Atomics.store should implement integer typed array semantics");
    assert!(
        outcome
            .note
            .contains("string(0|33|-7|3|33|-5|true|true|false|true|false)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_atomics_notify_validates_views_and_returns_zero_without_waiters() {
    let source = r#"
const sharedI32 = new Int32Array(new SharedArrayBuffer(8));
const localI32 = new Int32Array(new ArrayBuffer(8));
const sharedI64 = new BigInt64Array(new SharedArrayBuffer(16));
const localI64 = new BigInt64Array(new ArrayBuffer(16));
const results = [
  Atomics.notify(sharedI32, 0),
  Atomics.notify(localI32, 0, 1),
  Atomics.notify(sharedI64, 0, Infinity),
  Atomics.notify(localI64, 0, -1)
];

let indexCoerced = false;
let countCoerced = false;
const poisonedIndex = { valueOf() { indexCoerced = true; return 0; } };
const poisonedCount = { valueOf() { countCoerced = true; return 0; } };
let wrongViewThrew = false;
try {
  Atomics.notify(new BigUint64Array(new SharedArrayBuffer(8)), poisonedIndex, poisonedCount);
} catch (error) {
  wrongViewThrew = error instanceof TypeError;
}

let countErrorPreserved = false;
const countError = new RangeError("count");
try {
  Atomics.notify(sharedI64, 0, { valueOf() { throw countError; } });
} catch (error) {
  countErrorPreserved = error === countError;
}

results.join("|") + "|" + wrongViewThrew + "|" + indexCoerced + "|" + countCoerced + "|" + countErrorPreserved;
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
        .expect("Atomics.notify should validate supported views without fabricating waiters");
    assert!(
        outcome
            .note
            .contains("string(0|0|0|0|true|false|false|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_preserves_captured_custom_concat_result_kind() {
    let outcome = engine()
        .run_script(
            r#"
const baseValues = [1, 2];
baseValues.concat = function () {
  return 41;
};
const value = baseValues.concat(3);

function describeValue() {
  return typeof value + "|" + (value + 1);
}

describeValue();
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("captured custom concat results should retain their runtime kind");
    assert!(
        outcome.note.contains("string(number|42)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_preserves_captured_concat_species_result_kind() {
    let outcome = engine()
        .run_script(
            r#"
const originalConstructor = Array.prototype.constructor;
const source = [1, 2];
source.concat = Array.prototype.concat;

function Species() {
  return {};
}
function CustomArrayConstructor() {}
CustomArrayConstructor[Symbol.species] = Species;
Array.prototype.constructor = CustomArrayConstructor;

const result = source.concat(3);
Array.prototype.constructor = originalConstructor;

function resultIsArray() {
  return Array.isArray(result);
}

resultIsArray();
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("captured custom Array species results should retain their runtime kind");
    assert!(
        outcome.note.contains("boolean(false)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_supports_phase_twenty_seven_boxed_builtins() {
    for (source, expected) in [
        ("typeof Number;", "string(function)"),
        ("Number === globalThis.Number;", "boolean(true)"),
        ("String === globalThis.String;", "boolean(true)"),
        ("Boolean === globalThis.Boolean;", "boolean(true)"),
        ("new Number(1) instanceof Number;", "boolean(true)"),
        ("new String(\"x\") instanceof String;", "boolean(true)"),
        ("new Boolean(false) instanceof Boolean;", "boolean(true)"),
        ("Object(1) instanceof Number;", "boolean(true)"),
        ("new Object(\"x\") instanceof String;", "boolean(true)"),
        (
            "Object.getPrototypeOf(Object(true)) === Boolean.prototype;",
            "boolean(true)",
        ),
        (
            "function f() { return this instanceof Number; } f.call(1);",
            "boolean(true)",
        ),
        (
            "function f() { return this instanceof String; } f.apply(\"x\", []);",
            "boolean(true)",
        ),
        (
            "function f() { return this instanceof Boolean; } f.call(false);",
            "boolean(true)",
        ),
        ("new Number(1) + 1;", "number(2)"),
        ("new String(\"x\") + \"y\";", "string(xy)"),
        ("new Boolean(false) == false;", "boolean(true)"),
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
                panic!("phase 27 boxed builtin case should run for `{source}`: {err:?}")
            });
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_runs_phase_twenty_eight_builtin_tails() {
    // Literal Function-constructor sources used to be rejected here with
    // `DynamicSourceGap::aot_known_source(Function(Ordinary))`. They now
    // compile ahead of time through the prepared dynamic-function registry
    // (docs/rust-rewrite/contracts/prepared-dynamic-functions.md): the
    // source goes through the ordinary parser, early errors, lowering and
    // Wasm codegen, and CreateDynamicFunction's observable results are
    // checked here. Sources that are only known at run time remain a typed
    // runtime capability rejection, covered by
    // tests/aot_realm_modules/aot_dynamic_source_capability.rs.
    for (source, expected) in [
        ("Function(\"return 1\")();", "number(1)"),
        ("new Function(\"return 1\")();", "number(1)"),
        (
            "new Function(\"a\", \"b\", \"return a + b\")(2, 3);",
            "number(5)",
        ),
        (
            "var f = Function(\"return 1\"); \
                 [typeof f, f.name, f.length, typeof f.prototype, \
                  Object.getPrototypeOf(f) === Function.prototype, \
                  f !== Function(\"return 1\")].join();",
            "string(function,anonymous,0,object,true,true)",
        ),
        (
            "Function(\"return 1\").toString() === \"function anonymous(\\n) {\\nreturn 1\\n}\";",
            "boolean(true)",
        ),
        (
            "try { Function(\"return +\"); false; } catch (e) { e.constructor === SyntaxError; }",
            "boolean(true)",
        ),
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
                panic!("an AOT-known Function source should run for `{source}`: {err:?}")
            });
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }

    // `Function.prototype.apply` over an array-like used to be on that
    // rejection list. It is implemented now (CreateListFromArrayLike and
    // all), so the guard asserts the working behaviour instead of a stale
    // "unsupported" expectation.
    for (source, expected) in [
        (
            "function f() { return arguments.length; } f.apply(null, { length: 3, 0: 1 });",
            "number(3)",
        ),
        (
            "function f(a) { return a; } f.apply(null, { length: 1, 0: 7 });",
            "number(7)",
        ),
        (
            "function f() { return 1; } try { f.apply(null, 5); } catch (e) { e instanceof TypeError; }",
            "boolean(true)",
        ),
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
            .unwrap_or_else(|err| panic!("apply over an array-like should run: {err:?}"));
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_supports_phase_twenty_nine_identifier_delete_and_globals() {
    for (source, expected) in [
        ("delete 1;", "boolean(true)"),
        ("let x = 1; delete x;", "boolean(false)"),
        ("const x = 1; delete x;", "boolean(false)"),
        ("var x = 1; delete x; x;", "number(1)"),
        ("function f() {} delete f; typeof f;", "string(function)"),
        (
            "missing = 1; delete missing; typeof missing;",
            "string(undefined)",
        ),
        ("globalThis.x = 1; delete x; typeof x;", "string(undefined)"),
        (
            "globalThis.x = 1; delete globalThis.x; typeof x;",
            "string(undefined)",
        ),
        ("delete missingName;", "boolean(true)"),
        (
            "function f() { y = 3; return delete y; } f();",
            "boolean(true)",
        ),
        ("let x = 1; globalThis.x = 2; delete x; x;", "number(1)"),
        ("delete ({ x: 1 }).x;", "boolean(true)"),
        ("let a = [1, 2]; delete a[0]; a.length;", "number(2)"),
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
                panic!("phase 29 delete/global case should run for `{source}`: {err:?}")
            });
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_map_core_preserves_same_value_zero_and_insertion_updates() {
    for (source, expected) in [
        (
            "var map = new Map(); var object = {}; var symbol = Symbol('k'); map.set('x', 1).set(NaN, 2).set(-0, 3).set(object, 4).set(symbol, 5); map.set('x', 6); [map.size, map.get('x'), map.get(NaN), map.get(+0), map.get(object), map.get(symbol), map.has(-0)].join('|');",
            "string(5|6|2|3|4|5|true)",
        ),
        (
            "var map = new Map(); for (var i = 0; i < 9; i += 1) map.set(i, i * 2); map.delete(2); map.delete(7); map.set(2, 99); [map.size, map.get(2), map.has(7), map.delete(7), map.delete(8), map.size].join('|');",
            "string(8|99|false|false|true|7)",
        ),
        (
            "var map = new Map(); map.set(1, 1).set(2, 2); map.clear(); map.set(3, 3); [map.size, map.has(1), map.get(3)].join('|');",
            "string(1|false|3)",
        ),
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
            .unwrap_or_else(|err| panic!("Map core case should run for `{source}`: {err:?}"));
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_map_get_or_insert_methods_preserve_keys_and_callback_contract() {
    let outcome = engine()
        .run_script(
            r#"
var map = new Map();
var objectKey = {};
var symbolKey = Symbol("key");
var directMiss = map.getOrInsert(objectKey, "object");
var directHit = map.getOrInsert(objectKey, "replacement");
map.set(NaN, "nan");
var nanHit = map.getOrInsert(NaN, "replacement");
var zeroMap = new Map();
var zeroMiss = zeroMap.getOrInsert(-0, "zero");
var zeroKey = zeroMap.keys().next().value;
var callbackCalls = 0;
var callbackThis = 1;
var callbackArgs = 0;
var callbackKey;
var callback = new Proxy(function(key) {
  "use strict";
  callbackCalls += 1;
  callbackThis = this;
  callbackArgs = arguments.length;
  callbackKey = key;
  return "symbol";
}, {});
var computedMiss = map.getOrInsertComputed(symbolKey, callback);
var computedHit = map.getOrInsertComputed(symbolKey, callback);
var presentInvalidThrows = false;
try {
  map.getOrInsertComputed(symbolKey, null);
} catch (error) {
  presentInvalidThrows = error instanceof TypeError;
}
var canonicalKey;
new Map().getOrInsertComputed(-0, function(key) { canonicalKey = key; });
[
  Map.prototype.getOrInsert.name,
  Map.prototype.getOrInsert.length,
  Map.prototype.getOrInsertComputed.name,
  Map.prototype.getOrInsertComputed.length,
  directMiss,
  directHit,
  map.get(objectKey),
  nanHit,
  zeroMiss,
  zeroMap.get(+0),
  1 / zeroKey === Infinity,
  computedMiss,
  computedHit,
  callbackCalls,
  callbackThis === undefined,
  callbackArgs,
  callbackKey === symbolKey,
  presentInvalidThrows,
  1 / canonicalKey === Infinity
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Map upsert methods should preserve keys and callback contracts");
    assert!(
            outcome.note.contains(
                "string(getOrInsert|2|getOrInsertComputed|2|object|object|object|nan|zero|zero|true|symbol|symbol|1|true|1|true|true|true)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_map_get_or_insert_computed_preserves_abrupt_and_reentrant_mutation() {
    let outcome = engine()
        .run_script(
            r#"
function CallbackError() {}
var callbackError = new CallbackError();
var map = new Map([[0, "zero"]]);
var abruptIdentity = false;
try {
  map.getOrInsertComputed(1, function() {
    map.set(0, "mutated");
    throw callbackError;
  });
} catch (error) {
  abruptIdentity = error === callbackError;
}
var absentAfterThrow = !map.has(1);
var sideEffectPreserved = map.get(0) === "mutated";
var overwriteResult = map.getOrInsertComputed(2, function() {
  map.set(2, "intermediate");
  return "final";
});
var overwriteStored = map.get(2) === "final";
var undefinedResult = map.getOrInsertComputed(3, function() {
  map.set(3, "intermediate");
});
var undefinedStored = map.has(3) && map.get(3) === undefined;
var other = __scriptwCreateRealm().global;
var brandRealm = false;
try {
  other.Map.prototype.getOrInsert.call({}, 1, 2);
} catch (error) {
  brandRealm = error instanceof other.TypeError && !(error instanceof TypeError);
}
var callbackRealm = false;
try {
  other.Map.prototype.getOrInsertComputed.call(new other.Map(), 1, null);
} catch (error) {
  callbackRealm = error instanceof other.TypeError && !(error instanceof TypeError);
}
[
  abruptIdentity,
  absentAfterThrow,
  sideEffectPreserved,
  overwriteResult,
  overwriteStored,
  undefinedResult === undefined,
  undefinedStored,
  brandRealm,
  callbackRealm
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Map computed upsert should preserve abrupt and reentrant mutation semantics");
    assert!(
        outcome
            .note
            .contains("string(true|true|true|final|true|true|true|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_map_group_by_preserves_keys_order_and_callback_contract() {
    let outcome = engine()
        .run_script(
            r#"
var IntrinsicMap = Map;
var groupBy = Map.groupBy;
var invalidCallbackIteratorGets = 0;
var invalidCallbackThrows = false;
var invalidCallbackIterable = {};
Object.defineProperty(invalidCallbackIterable, Symbol.iterator, {
  get: function() {
    invalidCallbackIteratorGets += 1;
    throw new Error("iterator must not be read");
  }
});
try {
  groupBy(invalidCallbackIterable, null);
} catch (error) {
  invalidCallbackThrows = error instanceof TypeError;
}
var firstKey = {};
var secondKey = {};
var values = [firstKey, secondKey, firstKey];
var nextGets = 0;
var nextCalls = 0;
var iteratorMethod = new Proxy(function() {
  var index = 0;
  return {
    get next() {
      nextGets += 1;
      return new Proxy(function() {
        nextCalls += 1;
        if (index === values.length) return { done: true };
        var value = values[index];
        index += 1;
        return { done: false, value: value };
      }, {});
    }
  };
}, {});
var iterable = { [Symbol.iterator]: iteratorMethod };
var calls = [];
var callbackThis = 1;
var callback = new Proxy(function(value, index) {
  "use strict";
  callbackThis = this;
  calls.push((value === firstKey ? "first" : "second") + ":" + index);
  return value;
}, {});
var grouped = groupBy.call(function NotMap() {}, iterable, callback);
var zeroGrouped = groupBy([-0, +0], function(value) { return value; });
[
  typeof groupBy,
  groupBy.name,
  groupBy.length,
  invalidCallbackThrows,
  invalidCallbackIteratorGets,
  grouped instanceof IntrinsicMap,
  Object.getPrototypeOf(grouped) === IntrinsicMap.prototype,
  grouped.size,
  grouped.get(firstKey).length,
  grouped.get(firstKey)[0] === firstKey,
  grouped.get(firstKey)[1] === firstKey,
  grouped.get(secondKey)[0] === secondKey,
  calls.join(","),
  callbackThis === undefined,
  nextGets,
  nextCalls,
  zeroGrouped.size,
  zeroGrouped.has(+0),
  1 / zeroGrouped.keys().next().value === Infinity
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Map.groupBy should preserve Map keys and iterator callback contracts");
    assert!(
            outcome.note.contains(
                "string(function|groupBy|2|true|0|true|true|2|2|true|true|true|first:0,second:1,first:2|true|1|4|1|true|true)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_map_group_by_uses_the_method_defining_realm() {
    let outcome = engine()
        .run_script(
            r#"
var other = __scriptwCreateRealm().global;
var groupBy = other.Map.groupBy;
var grouped = groupBy.call(Map, [1, 2, 3], function(value) { return value % 2; });
var group = grouped.get(1);
[
  other.Map.groupBy !== Map.groupBy,
  Object.getPrototypeOf(grouped) === other.Map.prototype,
  grouped instanceof other.Map,
  grouped instanceof Map,
  group instanceof other.Array,
  group instanceof Array,
  group[0] === 1 && group[1] === 3
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Map.groupBy should allocate results from its defining realm intrinsics");
    assert!(
        outcome
            .note
            .contains("string(true|true|true|false|true|false|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_set_core_preserves_same_value_zero_and_insertion_updates() {
    for (source, expected) in [
        (
            "var set = new Set(); var object = {}; var symbol = Symbol('k'); set.add('x').add(NaN).add(-0).add(object).add(symbol).add(900719925474099100000n); set.add('x').add(+0).add(900719925474099100000n).add(900719925474099100000); [set.size, set.has('x'), set.has(NaN), set.has(+0), set.has(object), set.has(symbol), set.has(900719925474099100000n), set.has(900719925474099100000), set.add(1) === set].join('|');",
            "string(7|true|true|true|true|true|true|true|true)",
        ),
        (
            "var set = new Set(); for (var i = 0; i < 9; i += 1) set.add(i); set.delete(2); set.delete(7); set.add(2); [set.size, set.has(2), set.has(7), set.delete(7), set.delete(8), set.size].join('|');",
            "string(8|true|false|false|true|7)",
        ),
        (
            "var set = new Set(); set.add(1).add(2); set.clear(); set.add(3); [set.size, set.has(1), set.has(3)].join('|');",
            "string(1|false|true)",
        ),
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
            .unwrap_or_else(|err| panic!("Set core case should run for `{source}`: {err:?}"));
        assert!(
            outcome.note.contains(expected),
            "source: {source}, note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_set_algebra_preserves_values_order_and_intrinsic_construction() {
    let outcome = engine()
        .run_script(
            r#"
var left = new Set([1, 2, 3, -0]);
var right = new Set([3, 4, +0]);
var originalAdd = Set.prototype.add;
Set.prototype.add = function() { throw new Error("Set algebra must not call add"); };
var difference = left.difference(right);
var intersection = left.intersection(right);
var symmetricDifference = left.symmetricDifference(right);
var union = left.union(right);
Set.prototype.add = originalAdd;
var mapIntersection = left.intersection(new Map([[2, "two"], [4, "four"]]));
var zeroIterator = union.values();
zeroIterator.next();
zeroIterator.next();
zeroIterator.next();
var zero = zeroIterator.next().value;
var notConstructors = true;
for (var method of [
  Set.prototype.difference,
  Set.prototype.intersection,
  Set.prototype.symmetricDifference,
  Set.prototype.union
]) {
  try { new method(new Set()); notConstructors = false; } catch (error) {
    notConstructors = notConstructors && error instanceof TypeError;
  }
}
[
  Set.prototype.difference.name,
  Set.prototype.difference.length,
  Set.prototype.intersection.name,
  Set.prototype.intersection.length,
  Set.prototype.symmetricDifference.name,
  Set.prototype.symmetricDifference.length,
  Set.prototype.union.name,
  Set.prototype.union.length,
  [...difference].join(","),
  [...intersection].join(","),
  [...symmetricDifference].join(","),
  [...union].join(","),
  [...mapIntersection].join(","),
  difference instanceof Set,
  Object.getPrototypeOf(union) === Set.prototype,
  union.size,
  union.has(-0) && union.has(+0),
  1 / zero === Infinity,
  left.size,
  right.size,
  notConstructors
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Set algebra should preserve values, ordering, and intrinsic construction");
    assert!(
            outcome.note.contains(
                "string(difference|1|intersection|1|symmetricDifference|1|union|1|1,2|3,0|1,2,4|1,2,3,0,4|2|true|true|5|true|true|4|3|true)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_set_algebra_observes_set_like_order_mutation_and_realms() {
    let outcome = engine()
            .run_script(
                r#"
var order = [];
var differenceLike = {
  get size() {
    order.push("get size");
    return { valueOf: function() { order.push("ToNumber size"); return 3; } };
  },
  get has() {
    order.push("get has");
    return new Proxy(function(value) { order.push("call has " + value); return value === 1; }, {});
  },
  get keys() {
    order.push("get keys");
    return function() { throw new Error("keys must not be called"); };
  }
};
var difference = new Set([1, 2]).difference(differenceLike);
var differenceOrder = order.join(",");

order = [];
var values = [-0, 2, 3];
var index = 0;
var keys = new Proxy(function() {
  order.push("call keys");
  return {
    get next() {
      order.push("get next");
      return new Proxy(function() {
        order.push("call next");
        return index === values.length
          ? { get done() { order.push("get done"); return true; } }
          : {
              get done() { order.push("get done"); return false; },
              get value() { order.push("get value"); return values[index++]; }
            };
      }, {});
    }
  };
}, {});
var unionLike = {
  get size() { order.push("get size"); return 3; },
  get has() { order.push("get has"); return function() { throw new Error("has must not be called"); }; },
  get keys() { order.push("get keys"); return keys; }
};
var union = new Set([1, 2]).union(unionLike);
var unionOrder = order.join(",");

var base = new Set(["a", "b", "c", "d", "e"]);
var mutationIndex = 0;
var mutationValues = ["x", "b", "c", "c"];
var mutationLike = {
  size: 4,
  get has() { base.add("q"); return function() { throw new Error("has must not be called"); }; },
  keys: function() {
    return { next: function() {
      if (mutationIndex === 0) {
        base.delete("b"); base.delete("c"); base.add("b"); base.add("d");
      }
      return mutationIndex === mutationValues.length
        ? { done: true }
        : { done: false, value: mutationValues[mutationIndex++] };
    } };
  }
};
var symmetric = base.symmetricDifference(mutationLike);

function NextError() {}
var nextError = new NextError();
var closeCount = 0;
var abruptIdentity = false;
try {
  new Set().union({
    size: 0,
    has: function() {},
    keys: function() {
      return {
        next: function() { throw nextError; },
        return: function() { closeCount += 1; return {}; }
      };
    }
  });
} catch (error) { abruptIdentity = error === nextError; }

var other = __scriptwCreateRealm().global;
var realmResult = other.Set.prototype.union.call(new other.Set([1]), new Set([2]));
var brandRealm = false;
try { other.Set.prototype.difference.call({}, new Set()); } catch (error) {
  brandRealm = error instanceof other.TypeError && !(error instanceof TypeError);
}
var sizeTypeRealm = false;
try {
  other.Set.prototype.union.call(new other.Set(), { size: NaN, has: function() {}, keys: function() {} });
} catch (error) { sizeTypeRealm = error instanceof other.TypeError && !(error instanceof TypeError); }
var sizeRangeRealm = false;
try {
  other.Set.prototype.union.call(new other.Set(), { size: -1, has: function() {}, keys: function() {} });
} catch (error) { sizeRangeRealm = error instanceof other.RangeError && !(error instanceof RangeError); }
[
  [...difference].join(","),
  differenceOrder,
  [...union].join(","),
  unionOrder,
  [...symmetric].join(","),
  [...base].join(","),
  abruptIdentity,
  closeCount,
  Object.getPrototypeOf(realmResult) === other.Set.prototype,
  realmResult instanceof other.Set,
  realmResult instanceof Set,
  brandRealm,
  sizeTypeRealm,
  sizeRangeRealm
].join("|");
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("Set algebra should preserve observable set-like and realm semantics");
    assert!(
            outcome.note.contains(
                "string(2|get size,ToNumber size,get has,get keys,call has 1,call has 2|1,2,0,3|get size,get has,get keys,call keys,get next,call next,get done,get value,call next,get done,get value,call next,get done,get value,call next,get done|a,c,d,e,q,x|a,d,e,q,b|true|0|true|true|false|true|true|true)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_set_algebra_converts_set_like_size_to_integer_or_infinity() {
    let outcome = engine()
        .run_script(
            r#"
var result = new Set([1]).difference({
  size: -0.5,
  has: function() { throw new Error("has must not be called"); },
  keys: function() { return new Set().keys(); }
});
[...result].join(",");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Set algebra should convert a set-like size before checking its range");
    assert!(outcome.note.contains("string(1)"), "note: {}", outcome.note);
}

#[test]
fn wasm_backend_set_predicates_observe_set_like_order_proxies_and_live_mutation() {
    let outcome = engine()
            .run_script(
                r#"
var order = [];
var setLike = {
  get size() { order.push("get size"); return { valueOf: function() { order.push("ToNumber size"); return 3; } }; },
  get has() { order.push("get has"); return new Proxy(function(value) { order.push("call has " + value); return value === "b"; }, {}); },
  get keys() { order.push("get keys"); return function() { throw new Error("keys must not be called"); }; }
};
var disjoint = new Set(["x", "b"]).isDisjointFrom(setLike);
var disjointOrder = order.join(",");

order = [];
var index = 0;
var keys = new Proxy(function() {
  order.push("call keys");
  return {
    get next() {
      order.push("get next");
      return new Proxy(function() {
        order.push("call next");
        return index === 2
          ? { get done() { order.push("get done"); return true; } }
          : {
              get done() { order.push("get done"); return false; },
              get value() { order.push("get value"); return ["a", "b"][index++]; }
            };
      }, {});
    }
  };
}, {});
var supersetLike = {
  get size() { order.push("get size"); return 2; },
  get has() { order.push("get has"); return function() { throw new Error("has must not be called"); }; },
  get keys() { order.push("get keys"); return keys; }
};
var superset = new Set(["a", "b", "c"]).isSupersetOf(supersetLike);
var supersetOrder = order.join(",");

var base = new Set(["a", "b", "c"]);
var mutationLike = {
  size: 3,
  has: function(value) {
    if (value === "a") {
      base.delete("b");
      base.delete("c");
      base.add("b");
    }
    return false;
  },
  keys: function() { throw new Error("keys must not be called"); }
};
var liveDisjoint = base.isDisjointFrom(mutationLike);

var supersetBase = new Set(["a", "b", "c"]);
var mutationIndex = 0;
var supersetMutationLike = {
  size: 2,
  has: function() { throw new Error("has must not be called"); },
  keys: function() { return { next: function() {
    if (mutationIndex === 0) {
      supersetBase.delete("b");
      supersetBase.delete("c");
      supersetBase.add("b");
    }
    return mutationIndex === 2
      ? { done: true }
      : { done: false, value: ["a", "b"][mutationIndex++] };
  } }; }
};
var liveSuperset = supersetBase.isSupersetOf(supersetMutationLike);
[
  disjoint,
  disjointOrder,
  superset,
  supersetOrder,
  liveDisjoint,
  [...base].join(","),
  liveSuperset,
  [...supersetBase].join(",")
].join("|");
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("Set predicates should preserve observable set-like operations");
    assert!(
            outcome.note.contains(
                "string(false|get size,ToNumber size,get has,get keys,call has x,call has b|true|get size,get has,get keys,call keys,get next,call next,get done,get value,call next,get done,get value,call next,get done|true|a,b|true|a,b)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_set_closes_only_after_abrupt_adder_calls() {
    let outcome = engine()
        .run_script(
            r#"
function NextError() {}
function ValueError() {}
function AddError() {}
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
  new Set(iterableWithNext(function() { throw new NextError(); }));
} catch (error) {
  nextErrorPreserved = error instanceof NextError;
}
var valueErrorPreserved = false;
try {
  new Set(iterableWithNext(function() {
    return { done: false, get value() { throw new ValueError(); } };
  }));
} catch (error) {
  valueErrorPreserved = error instanceof ValueError;
}
var intrinsicAdd = Set.prototype.add;
Set.prototype.add = function() { throw new AddError(); };
var addErrorPreserved = false;
try {
  new Set(iterableWithNext(function() { return { done: false, value: 1 }; }));
} catch (error) {
  addErrorPreserved = error instanceof AddError;
}
Set.prototype.add = intrinsicAdd;
[nextErrorPreserved, valueErrorPreserved, addErrorPreserved, closeCount].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Set should close an iterator only after its captured adder throws");
    assert!(
        outcome.note.contains("string(true|true|true|1)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_resumes_nested_yield_operands() {
    let source = r#"
            function* nested() {
                yield yield 1;
            }
            const iterator = nested();
            const inner = iterator.next();
            const outer = iterator.next(3);
            const completed = iterator.next(4);
            inner.value === 1 && inner.done === false
                && outer.value === 3 && outer.done === false
                && completed.value === undefined && completed.done === true;
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
        .unwrap_or_else(|err| panic!("nested generator yield should resume: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_resumes_template_interpolations_in_evaluation_order() {
    let source = r#"
            let order = "";
            let output;
            function coercible(text, marker) {
                return {
                    toString: function () {
                        order += marker;
                        return text;
                    }
                };
            }
            function* interpolate() {
                output = `a${coercible("b", "B")}${yield 1}d${coercible("e", "E")}${yield 2}g`;
            }

            const iterator = interpolate();
            const first = iterator.next();
            const afterFirst = output === undefined && order === "B";
            const second = iterator.next(coercible("c", "C"));
            const afterSecond = output === undefined && order === "BCE";
            const third = iterator.next(coercible("f", "F"));

            first.value === 1 && first.done === false
                && second.value === 2 && second.done === false
                && third.value === undefined && third.done === true
                && afterFirst && afterSecond
                && output === "abcdefg" && order === "BCEF";
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
        .unwrap_or_else(|err| panic!("generator template interpolation should resume: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}
