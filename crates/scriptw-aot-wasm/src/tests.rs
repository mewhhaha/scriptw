use super::*;
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::{BigIntLiteralIr, HostSurfacePolicy, lower, lower_with_host_surface_policy};
use wasmparser::{Operator, Parser, Payload, Validator, WasmFeatures};

mod array_data_producers;
mod bootstrap_undefined;
mod call_arity;
mod direct_eval_dispatch;
mod numeric_imul;
mod numeric_leaf;
mod plain_data_literals;
mod program_cache_pages;
mod regexp_programs;
mod scalar_predicates;
mod scalar_regions;
mod unobserved_arguments;

fn emit_script(source: &str) -> Result<WasmArtifact, EmitError> {
    let source = parse(source, ParseOptions::script()).expect("script should parse");
    emit(&lower_with_host_surface_policy(
        &source,
        HostSurfacePolicy::Test262,
    ))
}

#[test]
fn heap_programs_link_against_one_runtime_module() {
    let first = emit_script("print([1, 2].map(x => x + 1));").expect("first program emits");
    let second = emit_script("class A { x = 1 } print(new A().x, /a+/.test('aa'));")
        .expect("second program emits");
    let runtime = first.runtime().expect("heap program links a runtime");
    assert!(
        std::sync::Arc::ptr_eq(
            runtime,
            second.runtime().expect("heap program links a runtime")
        ),
        "R is emitted once and shared"
    );
    assert_ne!(first.bytes, second.bytes);
    assert!(first.bytes.len() < runtime.bytes().len() / 20);
    let features = WasmFeatures::default()
        | WasmFeatures::THREADS
        | WasmFeatures::FUNCTION_REFERENCES
        | WasmFeatures::GC
        | WasmFeatures::EXCEPTIONS;
    for bytes in [&first.bytes[..], &second.bytes[..], &runtime.bytes()[..]] {
        Validator::new_with_features(features)
            .validate_all(bytes)
            .expect("runtime and program modules validate");
    }
    let imports = imported_function_count(&first);
    let runtime_functions = Parser::new(0)
        .parse_all(runtime.bytes())
        .filter_map(|payload| match payload.expect("runtime parses") {
            Payload::FunctionSection(reader) => Some(reader.count()),
            _ => None,
        })
        .sum::<u32>();
    let host_imports = Parser::new(0)
        .parse_all(runtime.bytes())
        .filter_map(|payload| match payload.expect("runtime parses") {
            Payload::ImportSection(reader) => Some(
                reader
                    .into_imports()
                    .filter(|import| {
                        matches!(
                            import.as_ref().expect("import parses").ty,
                            wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                        )
                    })
                    .count() as u32,
            ),
            _ => None,
        })
        .sum::<u32>();
    assert_eq!(imports, host_imports + runtime_functions);
}

#[test]
fn operations_emits_to_boolean_spec_operation() {
    let source =
        parse("Boolean(globalThis.flag);", ParseOptions::script()).expect("script should parse");
    let program = lower(&source);
    assert!(program.ir_summary().contains("spec_operations=2"));
    let artifact = emit(&program).expect("spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn plain_async_loop_awaits_emit_a_valid_module() {
    // Each of these lowers to a `StatementIr::GeneratorLoop` that the plain
    // async body consumes its typed activation resume point, running
    // one iteration per invocation of the body.
    for source in [
        "(async function(){ let t = 0; for (let i = 0; i < 3; i++) { t += await Promise.resolve(i); } print(t); })();",
        "(async function(){ let t = 0; for (let i = 0; i < 3; i++) { const v = await Promise.resolve(i); t += v; } print(t); })();",
        "(async function(){ let n = 0; while (n < 3) { n++; await Promise.resolve(n); } print(n); })();",
        "(async function(){ const out = []; for (const x of [1,2,3]) { out.push(await Promise.resolve(x)); } print(out); })();",
    ] {
        let artifact =
            emit_script(source).unwrap_or_else(|err| panic!("{source} should emit: {err:?}"));
        expect_valid_module(&artifact, 0);
    }
}

#[test]
fn full_bootstrap_emits_without_proto_source_reference() {
    let artifact = emit_script("this;").expect("full bootstrap script should emit");

    expect_valid_module(&artifact, 0);
}

/// Reads the Wasm `name` section back out of an emitted module.
///
/// Wasmtime builds its per-function symbol as
/// `wasm[0]::function[N]::<clean_symbol(name)>` from exactly this section,
/// so a module that emits it turns an anonymous native-compilation failure
/// into a named one.
fn function_names(artifact: &WasmArtifact) -> BTreeMap<u32, String> {
    function_names_in_bytes(&artifact.bytes)
}

fn function_names_in_bytes(bytes: &[u8]) -> BTreeMap<u32, String> {
    let mut names = BTreeMap::new();
    for payload in Parser::new(0).parse_all(bytes) {
        let Payload::CustomSection(section) = payload.expect("module should parse") else {
            continue;
        };
        let wasmparser::KnownCustom::Name(subsections) = section.as_known() else {
            continue;
        };
        for subsection in subsections {
            if let wasmparser::Name::Function(map) =
                subsection.expect("name subsection should parse")
            {
                for naming in map {
                    let naming = naming.expect("function naming should parse");
                    names.insert(naming.index, naming.name.to_string());
                }
            }
        }
    }
    names
}

/// Number of *function* imports, which is the index the first code-section
/// body occupies. Read back from the encoded module rather than from the
/// emitter's own variable, so it is an independent witness of the base the
/// name section indices must start from.
fn imported_function_count(artifact: &WasmArtifact) -> u32 {
    imported_function_count_in_bytes(&artifact.bytes)
}

fn imported_function_count_in_bytes(bytes: &[u8]) -> u32 {
    let mut count = 0u32;
    for payload in Parser::new(0).parse_all(bytes) {
        let Payload::ImportSection(reader) = payload.expect("module should parse") else {
            continue;
        };
        for import in reader.into_imports() {
            let import = import.expect("import should parse");
            if matches!(
                import.ty,
                wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
            ) {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn emitted_modules_name_every_function() {
    let artifact = emit_script("function outer() { return 1; } outer();")
        .expect("named function script should emit");
    expect_valid_module(&artifact, 1);

    let names = function_names(&artifact);
    // The index *base* is the load-bearing half. `names.len() == code_entries`
    // and every prefix check below stay green if `ModuleCode::new` were given
    // `0` instead of the imported function count — every name would then be
    // off by that count and wasmtime's `wasm[0]::function[N]` label, the
    // entire point of the section, would point at the wrong function. `main`
    // is the first body pushed, so it must sit at exactly the first
    // non-imported index.
    let first_body_index = imported_function_count(&artifact);
    assert!(first_body_index > 0, "the module must import functions");
    assert_eq!(
        names.keys().next(),
        Some(&first_body_index),
        "name section must start at the first non-imported function index: {names:?}"
    );
    assert_eq!(
        names.get(&first_body_index).map(String::as_str),
        Some("scriptw::main"),
        "main is pushed first, so it must own the first non-imported index: {names:?}"
    );
    assert!(
        names.values().any(|name| name.starts_with("js::outer")),
        "user functions must be named: {names:?}"
    );
    let runtime = artifact.runtime().expect("heap program links R");
    let runtime_names = function_names_in_bytes(runtime.bytes());
    assert!(
        runtime_names
            .values()
            .any(|name| name == "helper::transient_byte_alloc"),
        "runtime helpers must be named in R: {runtime_names:?}"
    );
    assert!(
        runtime_names
            .values()
            .any(|name| name.starts_with("builtin::")),
        "compiled builtins must be named in R: {runtime_names:?}"
    );

    // Every code-section entry is named, because every entry goes through
    // `ModuleCode::push(EmittedFunction)` and an `EmittedFunction` cannot be
    // built without an identity.
    for bytes in [runtime.bytes(), artifact.bytes.as_slice()] {
        let names = function_names_in_bytes(bytes);
        let first_body = imported_function_count_in_bytes(bytes);
        let code_entries = Parser::new(0)
            .parse_all(bytes)
            .filter(|payload| {
                matches!(
                    payload.as_ref().expect("module should parse"),
                    Payload::CodeSectionEntry(_)
                )
            })
            .count();
        assert_eq!(names.len(), code_entries);
        assert_eq!(
            names.keys().copied().collect::<Vec<_>>(),
            (first_body..first_body + code_entries as u32).collect::<Vec<_>>()
        );
    }
}

#[test]
fn debug_dump_attributes_the_largest_emitted_function() {
    let artifact = emit_script("this;").expect("full bootstrap script should emit");
    let largest = artifact
        .debug_dump
        .lines()
        .find(|line| line.starts_with("largest emitted function: "))
        .expect("debug_dump should attribute the largest emitted body");
    // The `none` fallback line carries no keys, so the presence of the
    // measured `key=value` shape is what proves a body was attributed. These
    // are exactly the keys `tests/emission/emit_golden.rs::largest_function` parses.
    for key in ["index=", "bytes=", "locals=", "kind=", "name="] {
        assert!(largest.contains(key), "{largest}");
    }
    let most_locals = artifact
        .debug_dump
        .lines()
        .find(|line| line.starts_with("most locals in an emitted function: "))
        .expect("debug_dump should report the most-locals body separately");
    for key in ["index=", "bytes=", "locals=", "kind=", "name="] {
        assert!(most_locals.contains(key), "{most_locals}");
    }
    assert!(
        artifact
            .debug_dump
            .lines()
            .any(|line| line.starts_with("emitted code bytes: ")),
        "{}",
        artifact.debug_dump
    );
}

/// The typed per-function report has exactly one row per code-section
/// entry in the encoded module.
///
/// **What in here can actually fail, and what cannot.** The `largest` vs
/// `debug_dump` comparison below is near-tautological by construction:
/// `emit()` builds `function_sizes` and renders the `largest emitted
/// function:` line from the *same* slice in adjacent statements, so the two
/// cannot disagree without an edit that deliberately splits them. It is kept
/// as a guard against exactly that re-split — two independently-maintained
/// size reports are how `runtime helper functions: 27` came to disagree with
/// a counted 32 + 1 — but it is not evidence.
///
/// The falsifiable assertion is `function_sizes.len() == code_entries`,
/// counted by an independent `wasmparser` walk of the encoded bytes. That is
/// what the name promises and it is the only part that can catch a real
/// defect.
///
/// This test also does **not** cover every emit path: it calls the
/// in-process `emit_script` helper only. `Engine::emit_wasm_on_current_thread`
/// and `run_with_wasm_aot_inner` — the two paths where the dump was actually
/// being dropped — are covered in `scriptw-engine`, not here, because this
/// crate cannot reach them.
#[test]
fn typed_report_row_count_matches_the_code_section() {
    let artifact = emit_script("this;").expect("full bootstrap script should emit");
    assert!(
        !artifact.function_sizes.is_empty(),
        "every emitted module has at least `scriptw::main`"
    );

    // Independent witness: the number of typed rows must equal the number
    // of code-section entries actually in the encoded module.
    let mut code_entries = 0usize;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(_) = payload.expect("module should parse") {
            code_entries += 1;
        }
    }
    assert_eq!(artifact.function_sizes.len(), code_entries);

    let largest = artifact
        .function_sizes
        .iter()
        .max_by_key(|summary| summary.body_bytes)
        .expect("a non-empty report has a largest entry");
    let line = artifact
        .debug_dump
        .lines()
        .find(|line| line.starts_with("largest emitted function: "))
        .expect("debug_dump should attribute the largest emitted body");
    assert!(
        line.ends_with(&format!(" name={}", largest.name)),
        "typed report says {} but debug_dump says {line}",
        largest.name
    );
    assert!(
        line.contains(&format!(" bytes={} ", largest.body_bytes.bytes())),
        "typed report says {} bytes but debug_dump says {line}",
        largest.body_bytes.bytes()
    );
    assert!(
        line.contains(&format!(" index={} ", largest.wasm_index)),
        "typed report says index {} but debug_dump says {line}",
        largest.wasm_index
    );

    // The category comes from `FunctionIdentity::category`, an exhaustive
    // match, so this also pins that a bootstrap module's biggest body is
    // still classified rather than falling into some unnamed bucket.
    assert!(
        [
            "main",
            "script",
            "builtin",
            "host-builtin",
            "runtime-helper"
        ]
        .contains(&largest.category),
        "unclassified category {}",
        largest.category
    );
}

/// A user function that does one dynamic-key property read must not carry a
/// six-figure body.
///
/// The probe is the exact text measured on this tree on 2026-08-09 with
/// `SCRIPTW_WASM_DUMP=... scriptw build wasm` plus a code-section/`name`-section
/// read-back. Ablation of that same probe, one line at a time:
///
/// | probe body | `js::probe#f0` |
/// |---|---|
/// | `return 0;` | 2,215 |
/// | `var A = k.split('-'); return A.length;` | 14,754 |
/// | ... plus `x = A[0];` (static index) | 14,911 |
/// | ... plus `x = A[i];` (**this probe**) | **87,101** |
/// | ... plus a second `y = A[j];` | 159,811 |
///
/// So one dynamic key costs `(159,811 - 14,754) / 2 = 72,528` bytes and the
/// floor for this probe once ToPropertyKey/ToPrimitive are outlined is the
/// 14,754-byte row plus a call. The budget is set at 30,000: comfortably
/// above that floor, less than half of the pre-split 87,101, so it is RED
/// before the split and GREEN after without being a tripwire on ordinary
/// drift.
///
/// **The absolute budget alone is not enough**, and that is why the static
/// control below exists. The post-split floor is ~14,900 (the static-index
/// row) plus a call, so *anything* from ~15,000 to 30,000 satisfies the
/// budget — including a half-landed split in which the ToPropertyKey seam
/// fires and the ToPrimitive one does not, or the reverse. The budget
/// asserts a constant; it cannot express the relationship it means.
///
/// So the test emits a second probe that is the same text with a **static**
/// index (`x = A[0];`) and asserts that, when the dynamic body is larger,
/// it costs the static body plus at most [`DYNAMIC_KEY_MARGIN_BYTES`]. That
/// is the real claim: *a dynamic key may not add an inlined coercion
/// composite.* A cheaper dynamic path is valid when lowering can preserve
/// more useful key information than the static-control path. The
/// margin is 10,000 rather than the few hundred bytes a seam plus a call
/// should really cost, because the post-split delta has not been measured —
/// but 10,000 is one seventh of the 72,528 bytes a single inline copy of
/// either composite occupies, so no partially-fired seam can hide inside it.
/// Tighten it towards ~1,000 once the delta is counted.
///
/// It asserts on a **named** function. Asserting on the largest body in the
/// module would be vacuous: in any bootstrap-heavy module that is a builtin
/// (`builtin::Object.defineProperty`, 375,534 bytes in this very probe),
/// which stays green while the user function regresses by an order of
/// magnitude.
///
/// Note the module contains *two* bodies for this function —
/// `js::probe#f0` and `js::probe#f0$exact_helper_context$0` — so the check
/// is over every body whose name starts with the function's name, and it
/// requires at least one to exist rather than passing on an empty set.
#[test]
fn emitted_function_bodies_stay_under_budget() {
    const PROBE: &str = "function probe(k, i, j) {\n  \
             var A = k.split('-');\n  \
             var x = '';\n  \
             x = A[i];\n  \
             return x;\n\
             }\n\
             print(probe('a-b-c', 0, 1));\n";
    /// The same text with a static index. Every other line is identical, so
    /// the difference between the two largest `js::probe#` bodies is the
    /// cost of the dynamic key and nothing else.
    const STATIC_CONTROL: &str = "function probe(k, i, j) {\n  \
             var A = k.split('-');\n  \
             var x = '';\n  \
             x = A[0];\n  \
             return x;\n\
             }\n\
             print(probe('a-b-c', 0, 1));\n";
    const BUDGET_BYTES: u32 = 30_000;
    /// See the doc comment: one inline copy of either composite is 72,528
    /// bytes, so a margin this size cannot conceal a half-fired seam.
    const DYNAMIC_KEY_MARGIN_BYTES: u32 = 10_000;

    /// Largest emitted body whose name starts with `js::probe#`. There are
    /// two (`js::probe#f0` and `js::probe#f0$exact_helper_context$0`), and
    /// an empty set must fail rather than pass vacuously.
    fn largest_probe_body(artifact: &WasmArtifact) -> (String, u32) {
        let probe_bodies = artifact
            .function_sizes
            .iter()
            .filter(|summary| summary.name.starts_with("js::probe#"))
            .collect::<Vec<_>>();
        assert!(
            !probe_bodies.is_empty(),
            "the probe function must be emitted, or this budget is vacuous: {:?}",
            artifact
                .function_sizes
                .iter()
                .map(|summary| summary.name.as_str())
                .collect::<Vec<_>>()
        );
        let largest = probe_bodies
            .iter()
            .max_by_key(|summary| summary.body_bytes.bytes())
            .expect("non-empty");
        (largest.name.clone(), largest.body_bytes.bytes())
    }

    let artifact = emit_script(PROBE).expect("probe script should emit");
    let probe_bodies = artifact
        .function_sizes
        .iter()
        .filter(|summary| summary.name.starts_with("js::probe#"))
        .collect::<Vec<_>>();
    assert!(
        !probe_bodies.is_empty(),
        "the probe function must be emitted, or this budget is vacuous: {:?}",
        artifact
            .function_sizes
            .iter()
            .map(|summary| summary.name.as_str())
            .collect::<Vec<_>>()
    );
    for body in probe_bodies {
        assert!(
            body.body_bytes.bytes() <= BUDGET_BYTES,
            "{} is {} bytes against a budget of {BUDGET_BYTES}; \
                 one dynamic-key property read should not cost a six-figure body",
            body.name,
            body.body_bytes.bytes()
        );
    }

    // The relational half. `BUDGET_BYTES` above is a constant and cannot
    // distinguish "both composites outlined" from "one of the two".
    let (dynamic_name, dynamic_bytes) = largest_probe_body(&artifact);
    let control = emit_script(STATIC_CONTROL).expect("static control script should emit");
    let (static_name, static_bytes) = largest_probe_body(&control);
    if let Some(delta) = dynamic_bytes.checked_sub(static_bytes) {
        assert!(
            delta <= DYNAMIC_KEY_MARGIN_BYTES,
            "a dynamic key costs {delta} bytes over the static control \
                 ({dynamic_name} {dynamic_bytes} vs {static_name} {static_bytes}), \
                 against a margin of {DYNAMIC_KEY_MARGIN_BYTES}. One inline copy of the \
                 ToPrimitive/ToPropertyKey composite is 72,528 bytes, so a delta this \
                 large means at least one of the two seams did not fire — read the two \
                 numbers rather than only raising the margin"
        );
    }
}

/// The `SCRIPTW_EMIT_SIZE_REPORT_PATH` sink writes one line per emitted
/// function, from the same traversal as the typed report, with the largest
/// body first.
///
/// This exists because the sink was the one mechanism the size-report work
/// added specifically so that "I set the variable and saw nothing" could not
/// be read as "there are no large functions" — and it was itself protected
/// only by review. The env read is deliberately *not* exercised here (it
/// would be visible to every other test in the process); this drives
/// `write_size_report_file`, which is everything the env wrapper does after
/// reading the variable.
#[test]
fn the_size_report_file_is_the_same_traversal_as_the_typed_report() {
    let artifact = emit_script("this;").expect("full bootstrap script should emit");
    let path = std::env::temp_dir().join(format!(
        "scriptw-emit-size-report-{}.txt",
        std::process::id()
    ));
    crate::emitted_function::write_size_report_file(&path, &artifact.function_sizes);

    let written = std::fs::read_to_string(&path).expect("the sink must write the report file");
    let _ = std::fs::remove_file(&path);

    let lines = written.lines().collect::<Vec<_>>();
    assert_eq!(
        lines.len(),
        artifact.function_sizes.len(),
        "the report file must hold one row per typed summary"
    );

    let largest_bytes = artifact
        .function_sizes
        .iter()
        .map(|summary| summary.body_bytes.bytes())
        .max()
        .expect("a non-empty report has a largest entry");
    assert!(
        lines[0].contains(&format!(" bytes={largest_bytes} ")),
        "the first report row must be a largest body ({largest_bytes} bytes), got {:?}",
        lines[0]
    );
    // `report_lines` breaks size ties by ascending wasm index while
    // `EmittedFunctionSummary::largest` keeps the last maximum, so assert on
    // the row's own identity rather than assuming the two pick the same tie.
    let reported_name = lines[0]
        .rsplit_once(" name=")
        .map(|(_, name)| name)
        .expect("every report row ends with name=");
    assert!(
        artifact.function_sizes.iter().any(|summary| {
            summary.name == reported_name && summary.body_bytes.bytes() == largest_bytes
        }),
        "the first report row names {reported_name}, which is not a largest typed summary"
    );
}

#[test]
fn runtime_helper_count_is_derived_not_asserted() {
    // The same registry partitions helpers between R and P. Full bootstrap
    // emits the JSON helper even when source never mentions JSON.
    assert_eq!(
        RuntimeHelperId::ALL
            .iter()
            .filter(|helper| helper.is_conditional())
            .count(),
        1
    );
    let expected_program = RuntimeHelperId::ALL
        .iter()
        .filter(|helper| helper.is_program_owned())
        .count();
    let expected = format!("runtime helper functions: {expected_program}");
    for source in ["this;", "JSON.stringify({});"] {
        let artifact = emit_script(source).expect("script should emit");
        assert!(
            artifact.debug_dump.lines().any(|line| line == expected),
            "expected `{expected}` for `{source}`\n{}",
            artifact.debug_dump
        );
        let program_names = function_names(&artifact);
        let runtime_names =
            function_names_in_bytes(artifact.runtime().expect("heap program links R").bytes());
        for helper in RuntimeHelperId::ALL {
            let name = format!("helper::{}", helper.debug_name());
            for (program_owned, names) in [(true, &program_names), (false, &runtime_names)] {
                assert_eq!(
                    names.values().filter(|actual| *actual == &name).count(),
                    usize::from(helper.is_program_owned() == program_owned),
                    "helper {name} must occur exactly in its declared R/P owner"
                );
            }
        }
    }
}

#[test]
fn call_spread_iterator_paths_emit_valid_module() {
    let artifact = emit_script(
        r#"
function iterable(values) {
  return {
    [Symbol.iterator]() {
      let index = 0;
      return {
        next() {
          if (index === values.length) return { done: true };
          return { done: false, value: values[index++] };
        }
      };
    }
  };
}
function collect() { return arguments.length; }
class Base { constructor(first, second) { this.total = first + second; } }
class Derived extends Base {
  constructor(values) { super(1, ...values); }
}
let source = iterable([2, 3]);
let indirect = collect;
collect(0, ...source, 4);
indirect(...source);
new Base(...source);
new Derived(source);
"#,
    )
    .expect("call spread iterator paths should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn object_literal_copy_data_properties_module_validates() {
    let artifact = emit_script(
        r#"
let calls = [];
let symbol = Symbol("copied");
let source = { get visible() { calls.push("get"); return 2; } };
source[symbol] = 3;
Object.defineProperty(source, "hidden", { value: 4, enumerable: false });
let proxy = new Proxy(source, {
  ownKeys(target) {
    calls.push("keys");
    return ["visible", "hidden", symbol];
  },
  getOwnPropertyDescriptor(target, key) {
    calls.push(key === symbol ? "descriptor:symbol" : "descriptor:" + key);
    return Reflect.getOwnPropertyDescriptor(target, key);
  },
  get(target, key) {
    calls.push(key === symbol ? "get:symbol" : "get:" + key);
    return Reflect.get(target, key);
  }
});
let result = { before: 1, ...null, ...undefined, ...proxy, visible: 5, ..."xy" };
result.visible + result[symbol] + result[0] + result[1] + calls.length;
"#,
    )
    .expect("object literal CopyDataProperties paths should emit");

    expect_valid_module(&artifact, 0);
}

#[test]
fn generator_parameter_modules_emit_valid_intrinsic_global_references() {
    let artifact = emit_script("function* f({ value }) {} f({ value: 1 });")
        .expect("generator parameter script should emit");

    expect_valid_module(&artifact, 1);
}

#[test]
fn async_generator_parameter_initialization_module_validates() {
    let artifact = emit_script(
        "let g = async function* (value = (g.prototype = null)) {}; let oldPrototype = g.prototype; let iterator = g(); Object.getPrototypeOf(iterator) !== oldPrototype;",
    )
    .expect("async-generator parameter initialization should emit");

    expect_valid_module(&artifact, 1);
}

#[test]
fn async_generator_invalid_receiver_rejection_module_validates() {
    let artifact = emit_script(
        "async function* stream() {} function* syncStream() {} let methods = [stream.prototype.next, stream.prototype.return, stream.prototype.throw]; let receivers = [1, {}, function () {}, syncStream()]; for (let methodIndex = 0; methodIndex < methods.length; methodIndex += 1) { for (let receiverIndex = 0; receiverIndex < receivers.length; receiverIndex += 1) { methods[methodIndex].call(receivers[receiverIndex]).then(undefined, function () {}); } }",
    )
    .expect("invalid async-generator receivers should emit rejected promises");

    expect_valid_module(&artifact, 1);
}

#[test]
fn async_iterator_async_dispose_module_validates() {
    let artifact = emit_script(
        "async function* stream() {} let prototype = Object.getPrototypeOf(Object.getPrototypeOf(stream.prototype)); let fulfilled = Object.create(prototype); fulfilled.return = function (value) { return Promise.resolve(value); }; let rejected = Object.create(prototype); rejected.return = function () { return Promise.reject(1); }; let absent = Object.create(prototype); prototype[Symbol.asyncDispose].call(fulfilled); prototype[Symbol.asyncDispose].call(rejected).catch(function () {}); prototype[Symbol.asyncDispose].call(absent);",
    )
    .expect("AsyncIterator asyncDispose paths should emit");

    expect_valid_module(&artifact, 1);
}

#[test]
fn async_generator_no_suspension_conditional_module_validates() {
    let artifact = emit_script(
        "async function* choose(flag) { let value = 0; if (flag) { value = 1; if (false) value = 2; } else { value = 3; } return value; } choose(true).next(); choose(false).next();",
    )
    .expect("async-generator ordinary conditionals should emit");

    expect_valid_module(&artifact, 1);
}

#[test]
fn async_generator_direct_yield_conditionals_validate() {
    for source in [
        "async function* choose(flag) { if (flag) yield 1; return 2; } choose(true).next(); choose(false).next();",
        "async function* choose(flag) { if (flag) yield 1; else yield 2; return 3; } choose(true).next(); choose(false).next();",
        "async function* choose(flag) { var value = flag ? 1 : 3; if (flag) { value += 1; yield value; value += 1; } else { value += 2; yield value; value += 2; } return value; } choose(true).next(); choose(false).next();",
    ] {
        let artifact =
            emit_script(source).expect("async-generator direct-yield branch should emit");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn async_generator_await_conditional_suspension_module_validates() {
    let artifact = emit_script(
        "async function* choose(flag) { if (flag) await 1; return 2; } choose(true).next();",
    )
    .expect("conditional Await has a checked resumable branch");
    expect_valid_module(&artifact, 1);
}

#[test]
fn completed_async_generator_return_reaction_module_validates() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_call(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Function),
            ExprIr::FunctionValue(StandardBuiltinId::AsyncGeneratorPrototypeReturn.function_id()),
        ),
        TypedExpr::from_info(ValueInfo::new(ValueKind::Undefined), ExprIr::Undefined),
        vec![TypedExpr::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::Number(1.0f64.to_bits()),
        )],
    ));
    script.body.result_kind = ValueKind::Dynamic;

    let artifact = emit(&program).expect("completed async-generator return reaction should emit");

    expect_valid_module(&artifact, 1);
}

#[test]
fn async_generator_body_await_dispatcher_module_validates() {
    let artifact = emit_script(
        "let started = false; async function* stream(value) { started = true; await value; return value; } const iterator = stream(1); iterator.next();",
    )
    .expect("async-generator body Await dispatcher should emit");

    expect_valid_module(&artifact, 1);
}

#[test]
fn async_generator_await_terminal_modules_validate() {
    for source in [
        "async function* stream(promise) { await promise; return 1; } stream(Promise.resolve()).next();",
        "async function* stream(value) { return value; } stream(1).next();",
        "async function* stream(value) { return await value; } stream(1).next();",
        "async function* stream(promise) { await promise; } stream(Promise.reject(1)).next();",
    ] {
        let artifact = emit_script(source).expect("async-generator body Await should emit");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn async_generator_yield_lifecycle_modules_validate() {
    for source in [
        "async function* stream() { yield; } const iterator = stream(); iterator.next(); iterator.next();",
        "async function* stream() { yield yield 1; } const iterator = stream(); iterator.next(); iterator.next(); iterator.next();",
        "async function* stream() { yield 1; throw 2; } const iterator = stream(); iterator.next(); iterator.return(2); iterator.next();",
        "async function* stream() { yield 1; throw 2; } const iterator = stream(); iterator.next(); iterator.return(Promise.resolve(2)); iterator.next();",
        "async function* stream() { yield 1; throw 2; } const reason = {}; const iterator = stream(); iterator.next(); iterator.throw(reason); iterator.next();",
        "async function* stream() { yield 1; throw 2; } const reason = Promise.resolve(2); const iterator = stream(); iterator.next(); iterator.throw(reason); iterator.next();",
    ] {
        let artifact = emit_script(source).expect("async-generator Yield lifecycle should emit");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn async_generator_yield_thenable_modules_validate() {
    for source in [
        "let thenable = { then: function (resolve, reject) { resolve(1); reject(2); } }; async function* stream() { yield thenable; } stream().next();",
        "let thenable = { then: function (resolve, reject) { reject(1); resolve(2); } }; async function* stream() { yield thenable; } stream().next();",
    ] {
        let artifact = emit_script(source).expect("async-generator thenable yield should emit");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn async_generator_yield_await_staging_module_validates() {
    let artifact = emit_script(
        "async function* stream(value) { yield await value; } const iterator = stream(1); iterator.next(); iterator.next();",
    )
    .expect("async-generator yield-await staging should emit");

    expect_valid_module(&artifact, 1);
}

#[test]
fn async_generator_delegation_module_validates() {
    let artifact = emit_script(
        "async function* inner() { yield 1; } async function* outer() { yield* inner(); } const iterator = outer(); iterator.next(); iterator.next();",
    )
    .expect("async-generator delegation should emit");

    expect_valid_module(&artifact, 1);
}

#[test]
fn async_generator_delegation_completion_module_validates() {
    for source in [
        "let source = { [Symbol.asyncIterator]: function () { return this; }, next: function () { return Promise.resolve({ value: 1, done: true }); } }; async function* outer() { var completion = yield* source; return completion; } outer().next();",
        "let source = { [Symbol.iterator]: function () { return this; }, next: function () { return { value: 1, done: false }; }, return: function (value) { return { value: value, done: true }; } }; let iterator = (async function*() { yield* source; }()); iterator.next(); iterator.return(2);",
        "let source = { [Symbol.iterator]: function () { return this; }, next: function () { return { value: 1, done: false }; }, throw: function (value) { return { value: value, done: true }; } }; let iterator = (async function*() { var completion = yield* source; return completion; }()); iterator.next(); iterator.throw(2);",
    ] {
        let artifact =
            emit_script(source).expect("async-generator delegated completion should emit");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn async_generator_rejected_yield_routing_modules_validate() {
    for source in [
        "async function* stream(reason) { yield Promise.reject(reason); yield 'unreachable'; } stream({}).next();",
        "async function* source(reason) { yield Promise.reject(reason); } async function* stream(reason) { for await (let value of source(reason)) { yield value; } } stream({}).next();",
        "async function* stream(reason) { for await (let value of [Promise.reject(reason)]) { yield value; } } stream({}).next();",
    ] {
        let artifact =
            emit_script(source).expect("async-generator rejected yield routing should emit");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn async_generator_yield_spread_modules_validate() {
    for source in [
        "let source = { [Symbol.iterator]: function () { return this; }, next: function () { return { done: true }; } }; [0, ...source, 1];",
        "async function* stream() { yield [...yield]; } stream().next();",
        "async function* stream() { yield [...yield yield]; } stream().next();",
        "async function* stream() { yield { ...yield, y: 1, ...yield yield }; } stream().next();",
    ] {
        let artifact = emit_script(source).expect("async-generator yield spread should emit");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn async_generator_yield_spread_conditional_module_validates() {
    let artifact = emit_script(
        "async function* stream(flag) { yield [...(flag ? yield [] : [])]; } stream(true).next();",
    )
    .expect("conditional spread yield has a checked resumable branch");
    expect_valid_module(&artifact, 1);
}

#[test]
fn promise_all_iterable_module_validates() {
    let artifact = emit_script(
        r#"let iterable = {};
iterable[Symbol.iterator] = function () {
  let index = 0;
  return { next: function () {
    if (index === 2) return { done: true };
    return { done: false, value: index++ };
  } };
};
Promise.all(iterable).then(function (values) { return values.length; });"#,
    )
    .expect("Promise.all iterable should emit");
    expect_valid_module(&artifact, 1);
}

#[test]
fn promise_race_iterable_module_validates() {
    let artifact = emit_script(
        r#"let iterable = {};
iterable[Symbol.iterator] = function () {
  let index = 0;
  return { next: function () {
    if (index === 2) return { done: true };
    return { done: false, value: index++ };
  } };
};
Promise.race(iterable).then(function (value) { return value; });"#,
    )
    .expect("Promise.race iterable should emit");
    expect_valid_module(&artifact, 1);
}

#[test]
fn promise_finally_module_validates() {
    let artifact = emit_script(
        r#"Promise.resolve(1)
  .finally(function () { return Promise.resolve(2); })
  .then(function (value) { return value; });
Promise.reject(3)
  .finally(function () {})
  .catch(function (reason) { return reason; });"#,
    )
    .expect("Promise.prototype.finally should emit");
    expect_valid_module(&artifact, 1);
}

#[test]
fn iterator_zip_keyed_module_validates() {
    let artifact = emit_script(
        r#"
const key = Symbol("key");
const inputs = { first: [1], second: [2, 3], [key]: [4] };
const iterator = Iterator.zipKeyed(inputs, {
  mode: "longest",
  padding: { first: 5, [key]: 6 },
});
const first = iterator.next().value;
const second = iterator.next().value;
Object.getPrototypeOf(first) === null
  && first.first === 1
  && first.second === 2
  && first[key] === 4
  && second.first === 5
  && second.second === 3
  && second[key] === 6;
"#,
    )
    .expect("Iterator.zipKeyed should emit");

    expect_valid_module(&artifact, 0);
}

#[test]
fn object_from_entries_module_validates() {
    let artifact = emit_script(
        r#"
const result = Object.fromEntries([["first", 1], ["second", 2]]);
result.first === 1 && result.second === 2;
"#,
    )
    .expect("Object.fromEntries should emit");

    expect_valid_module(&artifact, 0);
}

#[test]
fn async_generator_mixed_await_and_yield_module_validates() {
    let artifact = emit_script(
        "async function* stream(value) { await value; yield value; } stream(1).next();",
    )
    .expect("mixed async-generator suspension should emit");

    expect_valid_module(&artifact, 1);
}

#[test]
fn async_generator_resumable_loop_modules_validate() {
    for source in [
        "async function* stream() { for (let i = 0; i < 0; i++) { yield i; } yield 9; } stream().next();",
        "async function* stream() { for (let i = 0; i < 1; i++) { yield i; } yield 9; } stream().next();",
        "async function* stream() { for (let i = 0; i < 3; i++) { yield Promise.resolve(i * 2); } yield 9; } stream().next();",
        "async function* stream() { for (let i = 0; i < 2; i++) { let observed; try { observed = value; } catch (error) { observed = 'tdz'; } yield observed; let value = i; } } stream().next();",
        "let observed; async function* stream() { for (let i = 0; i < 1; i++) { let value = 7; yield value; observed = value; } } stream().next();",
    ] {
        let artifact = emit_script(source).expect("async-generator resumable loop should emit");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn async_generator_resumable_await_loop_module_validates() {
    let artifact = emit_script(
        "async function tick(value) { return value; }
             async function* stream() {
                 for (let i = 0; i < 2; i++) {
                     await tick(i);
                 }
                 return 0;
             }
             stream().next();",
    )
    .expect("async-generator resumable Await loop should emit");

    expect_valid_module(&artifact, 1);
}

#[test]
fn async_generator_terminal_completion_and_completed_queue_modules_validate() {
    for source in [
        "async function* stream() {} const iterator = stream(); iterator.next(); iterator.next(); iterator.throw(1); iterator.return(2);",
        "const stream = async function* named() {}; stream().next(); stream().throw(1); stream().return(2);",
        "async function* stream() { return 1; } stream().next();",
        "async function* stream() { throw 1; } stream().next();",
    ] {
        let artifact =
            emit_script(source).expect("async-generator terminal body completion should emit");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn async_generator_body_dispatcher_nested_for_await_module_validates() {
    let artifact = emit_script(
        "async function* stream(source) { for await (const outer of source) { for await (const inner of outer) { print(inner); } } }",
    ).expect("nested for-await owns independent continuation states");
    expect_valid_module(&artifact, 1);
}

#[test]
fn async_generator_for_await_with_a_suspending_body_modules_validate() {
    for source in [
        // The headline shape: a non-transparent yield, so the delegation
        // shortcut does not apply and the generic emitter runs.
        "async function* stream(source) { for await (const value of source) { print(value); yield value * 2; } }
             stream([1, 2]).next();",
        // A statement after the suspension, which only runs on the
        // invocation that resumes at the body's own state.
        "async function* stream(source) { for await (const value of source) { yield value; print(value); } }
             stream([1, 2]).next();",
        // Two suspensions in one body: three invocations per iteration.
        "async function* stream(source) { for await (const value of source) { yield value; yield value + 1; } }
             stream([1, 2]).next();",
        // The loop sits between other suspensions, so its span starts above
        // zero and the enclosing dispatcher has to route into it.
        "async function* stream(source) { yield 0; for await (const value of source) { print(value); yield value; } yield 1; }
             stream([1, 2]).next();",
        // `var` takes the storage-without-environment path for the binding.
        "async function* stream(source) { for await (var value of source) { yield value; print(value); } }
             stream([1, 2]).next();",
    ] {
        let artifact = emit_script(source)
            .expect("for-await with a suspending body should emit in an async generator");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn async_generator_for_await_with_a_suspension_free_body_modules_validate() {
    for source in [
        "async function* stream(source) { for await (const value of source) { print(value); } }
             stream([1, 2]).next();",
        "async function* stream(source) { for await (const value of source) { if (value) continue; break; } yield 1; }
             stream([1, 2]).next();",
        "async function* stream(source) { yield 0; for await (const value of source) { print(value); } yield 1; }
             stream([1, 2]).next();",
    ] {
        let artifact = emit_script(source)
            .expect("for-await with a suspension-free body should emit in an async generator");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn map_cross_realm_new_target_modules_validate() {
    for source in [
        r#"var other = __scriptwCreateRealm().global;
var C = other.Object;
C.prototype = null;
Reflect.construct(Map, [], C);"#,
        r#"var other = __scriptwCreateRealm().global;
var C = other.Object;
C.prototype = null;
var bound = C.bind(null);
Reflect.construct(Map, [], bound);"#,
        r#"var other = __scriptwCreateRealm().global;
var C = other.Object;
C.prototype = null;
var revocable;
revocable = Proxy.revocable(C, {
  get: function(target, key) {
    if (key === "prototype") revocable.revoke();
    return null;
  }
});
try { Reflect.construct(Map, [], revocable.proxy); } catch (error) {}"#,
    ] {
        let artifact = emit_script(source).expect("Map newTarget script should emit");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn set_cross_realm_new_target_modules_validate() {
    for source in [
        r#"var other = __scriptwCreateRealm().global;
var C = other.Object;
C.prototype = null;
Reflect.construct(Set, [], C);"#,
        r#"var other = __scriptwCreateRealm().global;
var C = other.Object;
C.prototype = null;
var bound = C.bind(null);
Reflect.construct(Set, [], bound);"#,
        r#"var other = __scriptwCreateRealm().global;
var C = other.Object;
C.prototype = null;
var revocable;
revocable = Proxy.revocable(C, {
  get: function(target, key) {
    if (key === "prototype") revocable.revoke();
    return null;
  }
});
try { Reflect.construct(Set, [], revocable.proxy); } catch (error) {}"#,
    ] {
        let artifact = emit_script(source).expect("Set newTarget script should emit");
        expect_valid_module(&artifact, 1);
    }
}

#[test]
fn operations_emits_to_numeric_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] =
        StatementIr::Expression(TypedExpr::spec_to_numeric(TypedExpr::from_info(
            ValueInfo::new(ValueKind::BigInt),
            ExprIr::BigInt(BigIntLiteralIr::from_i64(1)),
        )));
    script.body.result_kind = ValueKind::Dynamic;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("ToNumeric spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn arbitrary_precision_bigint_literal_emits_a_gc_module() {
    let source = parse("184467440737095516161234567890n;", ParseOptions::script())
        .expect("script should parse");
    let program = lower(&source);
    let artifact = emit(&program).expect("arbitrary precision BigInt should emit");

    expect_valid_module(&artifact, 0);
    assert!(
        artifact
            .gc_host_imports()
            .contains(&GcHostImport::CollectGc)
    );
}

#[test]
fn operations_emits_is_callable_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] =
        StatementIr::Expression(TypedExpr::spec_is_callable(TypedExpr::from_info(
            ValueInfo::new(ValueKind::Function),
            ExprIr::FunctionValue(StandardBuiltinId::MathMax.function_id()),
        )));
    script.body.result_kind = ValueKind::Boolean;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("IsCallable spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn planning_roots_number_static_method_reached_through_getv_call() {
    let source = parse(
        r#"
            let actual = Number.isNaN(NaN);
            if (actual !== true) throw actual;
            if (Number.isNaN("NaN") !== false) throw "string must not coerce";
            if (Number.isFinite(Infinity) !== false) throw "infinity must stay non-finite";
            262;
            "#,
        ParseOptions::script(),
    )
    .expect("script should parse");
    let program = lower(&source);
    let script = program.script.as_ref().expect("script ir should exist");

    assert!(script_references_standard_builtin(
        script,
        StandardBuiltinId::NumberIsNaN
    ));
    assert!(script_references_standard_builtin(
        script,
        StandardBuiltinId::NumberIsFinite
    ));
}

#[test]
fn operations_emits_is_constructor_spec_operation() {
    let source = parse(
        "let value = function C() {}; __scriptwIsConstructor(value);",
        ParseOptions::script(),
    )
    .expect("script should parse");
    let program = lower_with_host_surface_policy(&source, HostSurfacePolicy::Test262);
    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("IsConstructor spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_is_property_key_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] =
        StatementIr::Expression(TypedExpr::spec_is_property_key(TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("key".to_string()),
        )));
    script.body.result_kind = ValueKind::Boolean;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("IsPropertyKey spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_to_number_spec_operation() {
    let source = parse("let value = \"42\"; Number(value);", ParseOptions::script())
        .expect("script should parse");
    let program = lower(&source);
    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("ToNumber spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_to_primitive_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_to_primitive(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Object),
            ExprIr::ObjectLiteral(vec![]),
        ),
        ToPrimitiveHint::String,
    ));
    script.body.result_kind = ValueKind::Dynamic;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("ToPrimitive spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_ordinary_object_to_primitive_default_concat_validates() {
    // "a" + {}: OrdinaryToPrimitive on a plain object must fall back to the
    // inherited Object.prototype.toString default ("[object Object]") instead of
    // throwing a TypeError.
    let artifact = emit_script(r#""a" + {};"#).expect("string concat with object should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn operations_ordinary_object_to_primitive_default_loose_equality_validates() {
    // {} == "[object Object]": abstract equality coerces the object through the
    // same OrdinaryToPrimitive default path.
    let artifact = emit_script(
        r#"let o = {};
o == "[object Object]";"#,
    )
    .expect("loose equality with object should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn operations_ordinary_object_to_primitive_respects_own_hooks() {
    // Own valueOf / toString / @@toPrimitive still take precedence over the
    // inherited default fallback.
    let artifact = emit_script(
        r#"let a = { toString() { return "x"; } };
let b = { valueOf() { return 1; } };
let c = { [Symbol.toPrimitive]() { return "s"; } };
("" + a) + (b + 0) + ("" + c);"#,
    )
    .expect("objects with own coercion hooks should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn operations_in_operator_type_error_reaches_active_catch_handler() {
    // `"x" in 1` throws a TypeError that must be caught by the enclosing
    // try/catch rather than escaping the function via an over-shooting branch.
    let artifact = emit_script(
        r#"try {
  "x" in 1;
} catch (e) {
  e instanceof TypeError;
}"#,
    )
    .expect("`in` on a non-object should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn operations_instanceof_dynamic_rhs_guard_validates() {
    // A right-hand side that is not a single statically-known constructor reaches
    // the runtime OrdinaryHasInstance guard in emit_instanceof_i32, which throws a
    // TypeError about the `instanceof` operand when the value is not callable/an
    // object at runtime. The union constructor keeps the RHS off the static-prototype
    // fast path so the guard is actually emitted.
    let artifact = emit_script(
        r#"function pick(flag) {
  let Ctor = flag ? Array : Object;
  try {
    return ({}) instanceof Ctor;
  } catch (e) {
    return e instanceof TypeError;
  }
}
pick(true);"#,
    )
    .expect("instanceof with a dynamic rhs should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn operations_emits_to_bigint_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_to_bigint(
        TypedExpr::from_info(ValueInfo::new(ValueKind::Boolean), ExprIr::Boolean(true)),
    ));
    script.body.result_kind = ValueKind::BigInt;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("ToBigInt spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_to_string_spec_operation() {
    let source = parse("let value = 42; String(value);", ParseOptions::script())
        .expect("script should parse");
    let program = lower(&source);
    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("ToString spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_to_object_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] =
        StatementIr::Expression(TypedExpr::spec_to_object(TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("boxed".to_string()),
        )));
    script.body.result_kind = ValueKind::Dynamic;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("ToObject spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_to_property_key_spec_operation_for_string_result() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] =
        StatementIr::Expression(TypedExpr::spec_to_property_key(TypedExpr::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::Number(7.0f64.to_bits()),
        )));
    script.body.result_kind = ValueKind::Dynamic;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("ToPropertyKey spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_to_property_key_spec_operation_for_symbol_result() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] =
        StatementIr::Expression(TypedExpr::spec_to_property_key(TypedExpr::from_info(
            ValueInfo::new(ValueKind::Symbol),
            ExprIr::Symbol { description: None },
        )));
    script.body.result_kind = ValueKind::Dynamic;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("ToPropertyKey symbol spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_to_integer_or_infinity_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_to_integer_or_infinity(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("-3.7".to_string()),
        ),
    ));
    script.body.result_kind = ValueKind::Number;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("ToIntegerOrInfinity spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_to_length_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] =
        StatementIr::Expression(TypedExpr::spec_to_length(TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("3.7".to_string()),
        )));
    script.body.result_kind = ValueKind::Number;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("ToLength spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_to_index_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] =
        StatementIr::Expression(TypedExpr::spec_to_index(TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("3".to_string()),
        )));
    script.body.result_kind = ValueKind::Number;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("ToIndex spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_strict_equality_spec_operation() {
    let source =
        parse("let value = 1; value === 1;", ParseOptions::script()).expect("script should parse");
    let program = lower(&source);
    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("StrictEqualityComparison spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn temporal_now_builtins_emit() {
    let source = parse(
        "Temporal.Now.timeZoneId(); Temporal.Now.instant(); Temporal.Now.zonedDateTimeISO();",
        ParseOptions::script(),
    )
    .expect("script should parse");
    let program = lower(&source);
    let artifact = emit(&program).expect("Temporal.Now members should emit");

    assert!(!artifact.bytes.is_empty());
    assert!(
        artifact
            .debug_dump
            .contains("import func: scriptw_host.wall_clock_millis"),
        "{}",
        artifact.debug_dump
    );
}

#[test]
fn temporal_duration_family_emits() {
    // Every member installs together, so naming one accessor has to bring
    // the whole prototype with it.
    let source = parse(
        "const d = new Temporal.Duration(1, 2, 3, 4, 5, 6, 7, 8, 9, 10);\n\
             d.years; d.sign; d.blank; d.toString(); d.toJSON(); d.negated(); d.abs();\n\
             d.with({ hours: 1 }); d.add({ hours: 1 }); d.subtract({ hours: 1 });\n\
             d.round('seconds'); d.total('seconds');\n\
             Temporal.Duration.from('P1Y'); Temporal.Duration.compare(d, d);",
        ParseOptions::script(),
    )
    .expect("script should parse");
    let program = lower(&source);
    let artifact = emit(&program).expect("Temporal.Duration should emit");

    assert!(!artifact.bytes.is_empty());
    assert!(
        artifact.debug_dump.contains("Temporal.Duration.prototype"),
        "{}",
        artifact.debug_dump
    );
}

#[test]
fn temporal_namespace_shape_exists_from_a_bare_temporal_reference() {
    // A bare namespace reference roots the complete shape, including all
    // `Temporal.Now` clock readers.
    let source =
        parse("var namespace = Temporal;", ParseOptions::script()).expect("script should parse");
    let program = lower(&source);
    let artifact = emit(&program).expect("complete Temporal namespace should emit");

    assert!(!artifact.bytes.is_empty());
    assert!(
        artifact
            .debug_dump
            .contains("import func: scriptw_host.wall_clock_millis"),
        "{}",
        artifact.debug_dump
    );
}

#[test]
fn temporal_instant_equals_builtin_emits() {
    let source = parse(
        "new Temporal.Instant(1n).equals(new Temporal.Instant(1n));",
        ParseOptions::script(),
    )
    .expect("script should parse");
    let program = lower(&source);
    let artifact = emit(&program).expect("Temporal.Instant.prototype.equals should emit");

    assert!(!artifact.bytes.is_empty());
}

#[test]
fn temporal_zoned_date_time_from_builtin_emits() {
    let source = parse(
        "Temporal.ZonedDateTime.from(\"1970-01-01T00:00Z[UTC]\");",
        ParseOptions::script(),
    )
    .expect("script should parse");
    let program = lower(&source);
    let artifact = emit(&program).expect("Temporal.ZonedDateTime.from should emit");

    assert!(!artifact.bytes.is_empty());
}

#[test]
fn temporal_zoned_date_time_from_property_bags_emit() {
    let source = parse(
        r#"
            Temporal.ZonedDateTime.from({
                year: 1976,
                monthCode: "M11",
                day: 18,
                timeZone: "+01:00"
            }, { overflow: "constrain" });
            var arrayBag = [];
            arrayBag.year = 1970;
            arrayBag.month = 1;
            arrayBag.day = 1;
            arrayBag.timeZone = "UTC";
            Temporal.ZonedDateTime.from(arrayBag);
            function functionBag() {}
            functionBag.year = 1970;
            functionBag.month = 1;
            functionBag.day = 1;
            functionBag.timeZone = "UTC";
            Temporal.ZonedDateTime.from(functionBag, function () {});
            "#,
        ParseOptions::script(),
    )
    .expect("script should parse");
    let program = lower(&source);
    let artifact = emit(&program).expect("Temporal.ZonedDateTime.from property bags should emit");

    assert!(!artifact.bytes.is_empty());
}

#[test]
fn temporal_zoned_date_time_offset_accessors_emit() {
    let source = parse(
        "const value = new Temporal.ZonedDateTime(0n, \"+01:30\"); \
             value.offset; value.offsetNanoseconds;",
        ParseOptions::script(),
    )
    .expect("script should parse");
    let program = lower(&source);
    let artifact = emit(&program).expect("Temporal.ZonedDateTime offset accessors should emit");

    assert!(!artifact.bytes.is_empty());
}

#[test]
fn temporal_zoned_date_time_civil_accessors_and_equals_emit() {
    let source = parse(
        r#"
            const value = new Temporal.ZonedDateTime(-1n, "+01:30");
            value.year;
            value.month;
            value.monthCode;
            value.day;
            value.hour;
            value.minute;
            value.second;
            value.millisecond;
            value.microsecond;
            value.nanosecond;
            value.equals({ year: 1970, month: 1, day: 1, timeZone: "+01:30" });
            "#,
        ParseOptions::script(),
    )
    .expect("script should parse");
    let program = lower(&source);
    let artifact =
        emit(&program).expect("Temporal.ZonedDateTime civil accessors and equals should emit");

    assert!(!artifact.bytes.is_empty());
}

#[test]
fn temporal_zoned_date_time_with_time_zone_emits() {
    let source = parse(
        r#"
            const value = new Temporal.ZonedDateTime(0n, "UTC");
            value.withTimeZone("+0130");
            value.withTimeZone("2021-08-19T17:30Z");
            value.withTimeZone(new Temporal.ZonedDateTime(1n, "-08"));
            "#,
        ParseOptions::script(),
    )
    .expect("script should parse");
    let program = lower(&source);
    let artifact =
        emit(&program).expect("Temporal.ZonedDateTime.prototype.withTimeZone should emit");

    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_loose_equality_spec_operation() {
    let source = parse("let value = 1; value == \"1\";", ParseOptions::script())
        .expect("script should parse");
    let program = lower(&source);
    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("IsLooselyEqual spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_same_value_spec_operation() {
    // Object.is is an observable property call, not a source-level intrinsic.
    // Exercise the exact spec-operation IR, as the SameValueZero test does.
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_same_value(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::Number(f64::NAN.to_bits()),
        ),
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::Number(f64::NAN.to_bits()),
        ),
    ));
    script.body.result_kind = ValueKind::Boolean;
    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("SameValue spec operation should emit");
    expect_valid_module(&artifact, 0);
    let property_call =
        emit_script("Object.is(NaN, NaN);").expect("ordinary Object.is call should emit");
    expect_valid_module(&property_call, 0);
}

#[test]
fn operations_emits_same_value_zero_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_same_value_zero(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::Number(0.0f64.to_bits()),
        ),
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::Number((-0.0f64).to_bits()),
        ),
    ));
    script.body.result_kind = ValueKind::Boolean;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("SameValueZero spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_get_v_spec_operation() {
    let source = parse("globalThis.flag;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_get_v(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Object),
            ExprIr::ExecutionGlobalObject,
        ),
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("flag".to_string()),
        ),
    ));
    script.body.result_kind = ValueKind::Dynamic;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("GetV spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_get_spec_operation() {
    let source = parse("globalThis.flag;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_get(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Object),
            ExprIr::ExecutionGlobalObject,
        ),
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("flag".to_string()),
        ),
    ));
    script.body.result_kind = ValueKind::Dynamic;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("Get spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_has_property_spec_operation() {
    let source =
        parse("\"flag\" in globalThis;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_has_property(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Object),
            ExprIr::ExecutionGlobalObject,
        ),
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("flag".to_string()),
        ),
    ));
    script.body.result_kind = ValueKind::Boolean;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("HasProperty spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_create_data_property_or_throw_spec_operation() {
    let source = parse("globalThis.flag;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] =
        StatementIr::Expression(TypedExpr::spec_create_data_property_or_throw(
            TypedExpr::from_info(
                ValueInfo::new(ValueKind::Object),
                ExprIr::ExecutionGlobalObject,
            ),
            TypedExpr::from_info(
                ValueInfo::new(ValueKind::String),
                ExprIr::String("flag".to_string()),
            ),
            TypedExpr::from_info(
                ValueInfo::new(ValueKind::Number),
                ExprIr::Number(1.0f64.to_bits()),
            ),
        ));
    script.body.result_kind = ValueKind::Undefined;

    assert!(program.ir_summary().contains("spec_operations=1"));
    assert!(program.ir_summary().contains("property_writes=1"));
    let artifact = emit(&program).expect("CreateDataPropertyOrThrow spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_set_spec_operation() {
    let source = parse("globalThis.flag;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_set(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Object),
            ExprIr::ExecutionGlobalObject,
        ),
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("flag".to_string()),
        ),
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::Number(1.0f64.to_bits()),
        ),
    ));
    script.body.result_kind = ValueKind::Boolean;

    assert!(program.ir_summary().contains("spec_operations=1"));
    assert!(program.ir_summary().contains("property_writes=1"));
    let artifact = emit(&program).expect("Set spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_delete_property_or_throw_spec_operation() {
    let source = parse("globalThis.flag;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_delete_property_or_throw(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Object),
            ExprIr::ExecutionGlobalObject,
        ),
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("flag".to_string()),
        ),
    ));
    script.body.result_kind = ValueKind::Boolean;

    assert!(program.ir_summary().contains("spec_operations=1"));
    assert!(program.ir_summary().contains("deletes=1"));
    let artifact = emit(&program).expect("DeletePropertyOrThrow spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_has_own_property_spec_operation() {
    let source = parse("globalThis.flag;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_has_own_property(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Object),
            ExprIr::ExecutionGlobalObject,
        ),
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("flag".to_string()),
        ),
    ));
    script.body.result_kind = ValueKind::Boolean;

    assert!(program.ir_summary().contains("spec_operations=1"));
    let artifact = emit(&program).expect("HasOwnProperty spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_get_method_spec_operation() {
    let source = parse("globalThis.flag;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_get_method(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Object),
            ExprIr::ExecutionGlobalObject,
        ),
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::String("flag".to_string()),
        ),
    ));
    script.body.result_kind = ValueKind::Dynamic;

    assert!(program.ir_summary().contains("spec_operations=1"));
    assert!(program.ir_summary().contains("property_reads=1"));
    let artifact = emit(&program).expect("GetMethod spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_call_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_call(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Function),
            ExprIr::FunctionValue(StandardBuiltinId::MathMax.function_id()),
        ),
        TypedExpr::from_info(ValueInfo::new(ValueKind::Undefined), ExprIr::Undefined),
        vec![TypedExpr::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::Number(1.0f64.to_bits()),
        )],
    ));
    script.body.result_kind = ValueKind::Dynamic;

    assert!(program.ir_summary().contains("spec_operations=1"));
    assert!(program.ir_summary().contains("calls=1"));
    let artifact = emit(&program).expect("Call spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

#[test]
fn operations_emits_construct_spec_operation() {
    let source = parse("0;", ParseOptions::script()).expect("script should parse");
    let mut program = lower(&source);
    let script = program.script.as_mut().expect("script ir should exist");
    script.body.statements[0] = StatementIr::Expression(TypedExpr::spec_construct(
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Function),
            ExprIr::FunctionValue(StandardBuiltinId::ArrayConstructor.function_id()),
        ),
        vec![TypedExpr::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::Number(1.0f64.to_bits()),
        )],
    ));
    script.body.result_kind = ValueKind::Dynamic;

    assert!(program.ir_summary().contains("spec_operations=1"));
    assert!(program.ir_summary().contains("constructs=1"));
    let artifact = emit(&program).expect("Construct spec operation should emit");
    assert!(!artifact.bytes.is_empty());
}

fn data_segment_at(bytes: &[u8], index: usize) -> Vec<u8> {
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::DataSection(reader) = payload.expect("module should parse") {
            return reader
                .into_iter()
                .nth(index)
                .expect("data segment exists")
                .expect("data segment should decode")
                .data
                .to_vec();
        }
    }
    panic!("module has no data section");
}

fn data_segment_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut collected = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("wasm parse should succeed") {
            Payload::DataSection(reader) => {
                for segment in reader {
                    let segment = segment.expect("data segment should decode");
                    match segment.kind {
                        wasmparser::DataKind::Active { .. } | wasmparser::DataKind::Passive => {
                            collected.extend_from_slice(segment.data);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    collected
}

#[test]
fn string_pool_encodes_literal_lone_surrogates_as_wtf8_bytes() {
    let encoded = format!("{JS_STRING_SURROGATE_SENTINEL}D800");
    assert_eq!(
        StringPool::runtime_bytes_for_string(&encoded),
        vec![0xED, 0xA0, 0x80]
    );
}

#[test]
fn string_pool_escapes_literal_surrogate_sentinel() {
    let encoded = format!("{JS_STRING_SURROGATE_SENTINEL}{JS_STRING_SURROGATE_SENTINEL}");
    assert_eq!(
        StringPool::runtime_bytes_for_string(&encoded),
        JS_STRING_SURROGATE_SENTINEL.to_string().as_bytes().to_vec()
    );
}

fn global_init_i64s(bytes: &[u8]) -> Vec<i64> {
    let mut values = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::GlobalSection(reader) = payload.expect("wasm parse should succeed") {
            for global in reader {
                let global = global.expect("global should decode");
                if let wasmparser::ValType::I64 = global.ty.content_type {
                    let mut init = global.init_expr.get_operators_reader();
                    match init.read().expect("global init op should decode") {
                        Operator::I64Const { value } => values.push(value),
                        op => panic!("unexpected i64 global init op: {op:?}"),
                    }
                }
            }
        }
    }
    values
}

fn memory_initial_pages(bytes: &[u8]) -> Vec<u64> {
    let mut pages = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::MemorySection(reader) = payload.expect("wasm parse should succeed") {
            for memory in reader {
                pages.push(memory.expect("memory should decode").initial);
            }
        }
    }
    pages
}

fn code_body_context(bytes: &[u8], offset: usize) -> String {
    let mut defined_index = 0usize;
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.expect("wasm parse should succeed");
        if let Payload::CodeSectionEntry(body) = payload {
            let range = body.range();
            if range.start <= offset && offset < range.end {
                let mut nearby = Vec::new();
                let mut reader = body
                    .get_operators_reader()
                    .expect("operators should decode");
                let mut depth = 1usize;
                while !reader.eof() {
                    match reader.read_with_offset() {
                        Ok((op, op_offset)) => {
                            if op_offset.saturating_add(512) >= offset && op_offset <= offset + 64 {
                                nearby.push(format!("d{depth} {op_offset:#x}: {op:?}"));
                            }
                            match op {
                                Operator::Block { .. }
                                | Operator::Loop { .. }
                                | Operator::If { .. } => depth += 1,
                                Operator::End => depth = depth.saturating_sub(1),
                                _ => {}
                            }
                        }
                        Err(err) => {
                            nearby.push(format!("operator decode error: {err}"));
                            break;
                        }
                    }
                }
                return format!(
                    "function body #{defined_index} byte range {:#x}..{:#x}; nearby ops: {}",
                    range.start,
                    range.end,
                    nearby.join("; ")
                );
            }
            defined_index += 1;
        }
    }
    "no containing function body found".to_string()
}

fn validation_error_offset(message: &str) -> Option<usize> {
    let marker = "offset 0x";
    let start = message.find(marker)? + marker.len();
    let hex = message[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_hexdigit())
        .collect::<String>();
    usize::from_str_radix(&hex, 16).ok()
}

/// Validates with `wasmparser` directly (rather than an engine's own
/// validator) so the accepted proposal set can be pinned to match the
/// production wasmtime configuration (`scriptw-engine`'s
/// `run_with_wasm_aot_inner`: threads, function-references, gc, and
/// exceptions all enabled) instead of an embedding engine's own default
/// feature set, which may lag behind the production target.
fn expect_valid_module(artifact: &WasmArtifact, _script_function_count: usize) {
    let features = WasmFeatures::default()
        | WasmFeatures::THREADS
        | WasmFeatures::FUNCTION_REFERENCES
        | WasmFeatures::GC
        | WasmFeatures::EXCEPTIONS;
    Validator::new_with_features(features)
        .validate_all(&artifact.bytes[..])
        .unwrap_or_else(|err| {
            let message = err.to_string();
            let context = validation_error_offset(&message)
                .map(|offset| code_body_context(&artifact.bytes, offset))
                .unwrap_or_else(|| "no validation offset found".to_string());
            panic!("module should validate: {message}; {context}");
        });
}

fn declared_function_local_counts(bytes: &[u8]) -> Vec<u32> {
    let mut counts = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("wasm parse should succeed") else {
            continue;
        };
        let locals = body
            .get_locals_reader()
            .expect("function locals should decode");
        let mut count = 0_u32;
        for local in locals {
            let (consecutive_count, _) = local.expect("function local should decode");
            count = count
                .checked_add(consecutive_count)
                .expect("function local count should fit in u32");
        }
        counts.push(count);
    }
    counts
}

#[test]
fn emitted_module_validates() {
    let artifact = emit_script("let x = 40; const y = 2; x + y;").expect("emit should work");
    expect_valid_module(&artifact, 0);
    assert!(artifact.debug_dump.contains("export func: main"));
    assert!(
        artifact
            .debug_dump
            .contains("main completion ABI: tag/scalar/reference/kind/target")
    );
}

#[test]
fn dynamic_number_exponentiation_module_validates_with_runtime_pow_import() {
    let artifact = emit_script(
        "let base = 9; let exponent = 0.5; base ** exponent + Math.pow(base, exponent);",
    )
    .expect("dynamic Number exponentiation should emit");

    expect_valid_module(&artifact, 0);
    assert!(
        artifact
            .debug_dump
            .contains("import func: scriptw_host.number_pow"),
        "{}",
        artifact.debug_dump
    );
}

#[test]
fn date_current_time_consumers_import_wall_clock_milliseconds() {
    for (source, consumer) in [
        ("Date.now();", "Date.now"),
        ("Date();", "Date function call"),
        ("new Date();", "zero-argument Date construction"),
    ] {
        let artifact = emit_script(source)
            .unwrap_or_else(|error| panic!("{consumer} script should emit: {error}"));

        expect_valid_module(&artifact, 0);
        assert!(
            artifact
                .debug_dump
                .contains("import func: scriptw_host.wall_clock_millis"),
            "{consumer} omitted its clock import:\n{}",
            artifact.debug_dump
        );
    }

    let artifact = emit_script("262;").expect("constant script should emit");
    assert!(
        !artifact
            .debug_dump
            .contains("import func: scriptw_host.wall_clock_millis"),
        "{}",
        artifact.debug_dump
    );
}

#[test]
fn math_random_alone_imports_the_typed_host_random_capability() {
    let artifact = emit_script("Math.random();").expect("Math.random script should emit");

    expect_valid_module(&artifact, 0);
    assert!(
        artifact
            .debug_dump
            .contains("import func: scriptw_host.random_f64"),
        "{}",
        artifact.debug_dump
    );

    let artifact = emit_script("262;").expect("constant script should emit");
    assert!(
        !artifact
            .debug_dump
            .contains("import func: scriptw_host.random_f64"),
        "{}",
        artifact.debug_dump
    );
}

#[test]
fn math_transcendentals_alone_import_their_typed_host_capabilities() {
    for (source, import) in [
        ("Math.acos(0.5);", "math_acos"),
        ("Math.acosh(2);", "math_acosh"),
        ("Math.asin(0.5);", "math_asin"),
        ("Math.asinh(1);", "math_asinh"),
        ("Math.atan(1);", "math_atan"),
        ("Math.atanh(0.5);", "math_atanh"),
        ("Math.cbrt(8);", "math_cbrt"),
        ("Math.cos(1);", "math_cos"),
        ("Math.cosh(1);", "math_cosh"),
        ("Math.exp(1);", "math_exp"),
        ("Math.expm1(1);", "math_expm1"),
        ("Math.log(2);", "math_log"),
        ("Math.log10(100);", "math_log10"),
        ("Math.log1p(1);", "math_log1p"),
        ("Math.log2(8);", "math_log2"),
        ("Math.sin(1);", "math_sin"),
        ("Math.sinh(1);", "math_sinh"),
        ("Math.tan(1);", "math_tan"),
        ("Math.tanh(1);", "math_tanh"),
        ("Math.atan2(1, 2);", "math_atan2"),
    ] {
        let artifact =
            emit_script(source).unwrap_or_else(|error| panic!("{source} should emit: {error}"));

        expect_valid_module(&artifact, 0);
        assert!(
            artifact
                .debug_dump
                .contains(&format!("import func: scriptw_host.{import}")),
            "{source} omitted its {import} import:\n{}",
            artifact.debug_dump
        );
    }

    let artifact = emit_script("262;").expect("constant script should emit");
    for import in [
        "math_acos",
        "math_acosh",
        "math_asin",
        "math_asinh",
        "math_atan",
        "math_atanh",
        "math_cbrt",
        "math_cos",
        "math_cosh",
        "math_exp",
        "math_expm1",
        "math_log",
        "math_log10",
        "math_log1p",
        "math_log2",
        "math_sin",
        "math_sinh",
        "math_tan",
        "math_tanh",
        "math_atan2",
    ] {
        assert!(
            !artifact
                .debug_dump
                .contains(&format!("import func: scriptw_host.{import}")),
            "constant script dragged in {import}:\n{}",
            artifact.debug_dump
        );
    }
}

#[test]
fn canonical_locale_list_alone_imports_the_typed_intl_host_call() {
    fn components(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        Parser::new(0)
            .parse_all(bytes)
            .filter_map(|payload| {
                let Payload::CustomSection(section) = payload.expect("module should parse") else {
                    return None;
                };
                matches!(
                    section.name(),
                    scriptw_intl::INTL_LOCALE_DATA_CUSTOM_SECTION
                        | scriptw_intl::INTL_LIST_DATA_CUSTOM_SECTION
                        | scriptw_intl::INTL_COLLATOR_DATA_CUSTOM_SECTION
                        | scriptw_intl::INTL_NUMBER_DATA_CUSTOM_SECTION
                        | scriptw_intl::INTL_SEGMENTER_DATA_CUSTOM_SECTION
                        | scriptw_intl::INTL_DISPLAY_NAMES_DATA_CUSTOM_SECTION
                        | scriptw_intl::INTL_RELATIVE_TIME_DATA_CUSTOM_SECTION
                        | scriptw_intl::INTL_DURATION_DATA_CUSTOM_SECTION
                        | scriptw_intl::INTL_NAMED_TIME_ZONE_DATA_CUSTOM_SECTION
                        | scriptw_intl::INTL_DATETIME_DATA_CUSTOM_SECTION
                        | scriptw_intl::INTL_TIME_ZONE_NAMES_DATA_CUSTOM_SECTION
                        | scriptw_intl::INTL_NATIVE_LOCALE_INFORMATION_CUSTOM_SECTION
                )
                .then(|| (section.name().to_owned(), section.data().to_vec()))
            })
            .collect()
    }
    let artifact = emit_script("Intl.getCanonicalLocales(['iw-IL']);")
        .expect("canonical locale list should emit");
    expect_valid_module(&artifact, 0);
    let runtime_bytes = artifact.runtime().expect("Intl program links R").bytes();
    assert!(Parser::new(0).parse_all(runtime_bytes).any(|payload| {
        let Payload::ImportSection(reader) = payload.expect("runtime should parse") else {
            return false;
        };
        reader.into_imports().any(|import| {
            let import = import.expect("host import should parse");
            import.module == GcHostImport::IntlProviderCall.module()
                && import.name == GcHostImport::IntlProviderCall.name()
                && matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                )
        })
    }));
    assert!(
        components(&artifact.bytes).is_empty(),
        "Intl images belong to R"
    );
    let expected_identity = scriptw_intl::embedded_intl_data_identity()
        .expect("embedded Intl identity should be valid")
        .artifact_identity();
    let identity_sections = Parser::new(0)
        .parse_all(runtime_bytes)
        .filter_map(|payload| {
            let Payload::CustomSection(section) = payload.expect("module should parse") else {
                return None;
            };
            (section.name() == scriptw_intl::INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION)
                .then(|| section.data().to_vec())
        })
        .collect::<Vec<_>>();
    assert_eq!(identity_sections, [expected_identity.as_bytes().to_vec()]);
    let actual = components(runtime_bytes);
    let expected = [
        (
            scriptw_intl::INTL_LOCALE_DATA_CUSTOM_SECTION,
            scriptw_intl::embedded_locale_data_image().unwrap().bytes(),
        ),
        (
            scriptw_intl::INTL_LIST_DATA_CUSTOM_SECTION,
            scriptw_intl::embedded_list_data_image().unwrap().bytes(),
        ),
        (
            scriptw_intl::INTL_COLLATOR_DATA_CUSTOM_SECTION,
            scriptw_intl::embedded_collator_data_image()
                .unwrap()
                .bytes(),
        ),
        (
            scriptw_intl::INTL_NUMBER_DATA_CUSTOM_SECTION,
            scriptw_intl::embedded_number_profiles_data_image()
                .unwrap()
                .bytes(),
        ),
        (
            scriptw_intl::INTL_SEGMENTER_DATA_CUSTOM_SECTION,
            scriptw_intl::embedded_segmenter_data_image()
                .unwrap()
                .bytes(),
        ),
        (
            scriptw_intl::INTL_DISPLAY_NAMES_DATA_CUSTOM_SECTION,
            scriptw_intl::embedded_display_names_data_image()
                .unwrap()
                .bytes(),
        ),
        (
            scriptw_intl::INTL_RELATIVE_TIME_DATA_CUSTOM_SECTION,
            scriptw_intl::embedded_relative_time_data_image()
                .unwrap()
                .bytes(),
        ),
        (
            scriptw_intl::INTL_DURATION_DATA_CUSTOM_SECTION,
            scriptw_intl::embedded_duration_data_image()
                .unwrap()
                .bytes(),
        ),
        (
            scriptw_intl::INTL_NAMED_TIME_ZONE_DATA_CUSTOM_SECTION,
            scriptw_intl::embedded_named_time_zone_data_image()
                .unwrap()
                .bytes(),
        ),
        (
            scriptw_intl::INTL_DATETIME_DATA_CUSTOM_SECTION,
            scriptw_intl::embedded_date_time_data_image()
                .unwrap()
                .bytes(),
        ),
        (
            scriptw_intl::INTL_TIME_ZONE_NAMES_DATA_CUSTOM_SECTION,
            scriptw_intl::embedded_time_zone_names_data_image()
                .unwrap()
                .bytes(),
        ),
        (
            scriptw_intl::INTL_NATIVE_LOCALE_INFORMATION_CUSTOM_SECTION,
            scriptw_intl::embedded_native_locale_information_data_image()
                .unwrap()
                .bytes(),
        ),
    ];
    assert_eq!(actual.len(), expected.len());
    for (name, data) in expected {
        assert_eq!(actual.iter().filter(|(key, _)| key == name).count(), 1);
        assert_eq!(
            actual
                .iter()
                .find(|(key, _)| key == name)
                .unwrap()
                .1
                .as_slice(),
            data.as_ref()
        );
    }
    let custom_profile = scriptw_intl::IntlCompilationProfile::Custom(
        scriptw_intl::CustomProfileId::parse("aot-image-owner").unwrap(),
    );
    let selection = scriptw_intl::IntlDataSelection::new(custom_profile.clone());
    let selected = selection.selected().expect("selected data should admit");
    let parsed = parse(
        "Intl.getCanonicalLocales(['iw-IL']); Intl.supportedValuesOf('calendar');",
        ParseOptions::script(),
    )
    .expect("selected Intl source should parse");
    let custom = emit_with_intl_profile(
        &lower(&parsed),
        PromiseRejectionPolicy::default(),
        &custom_profile,
    )
    .expect("selected catalogues and host images should emit together");
    expect_valid_module(&custom, 0);
    let custom_runtime = custom
        .runtime()
        .expect("custom Intl program links R")
        .bytes();
    assert!(components(&custom.bytes).is_empty());
    let custom_sections = components(custom_runtime);
    assert_eq!(custom_sections.len(), 12);
    for (name, bytes) in selected.component_sections() {
        assert_eq!(
            custom_sections
                .iter()
                .filter(|(key, _)| key == name)
                .count(),
            1
        );
        assert_eq!(
            custom_sections
                .iter()
                .find(|(key, _)| key == name)
                .unwrap()
                .1
                .as_slice(),
            bytes.as_ref(),
        );
    }
    let custom_identity = selected.identity().artifact_identity();
    assert_ne!(custom_identity.as_bytes(), expected_identity.as_bytes());
    let actual_identity = Parser::new(0)
        .parse_all(custom_runtime)
        .filter_map(|payload| {
            let Payload::CustomSection(section) = payload.expect("module should parse") else {
                return None;
            };
            (section.name() == scriptw_intl::INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION)
                .then(|| section.data().to_vec())
        })
        .collect::<Vec<_>>();
    assert_eq!(actual_identity, [custom_identity.as_bytes().to_vec()]);
    let parsed = parse("262;", ParseOptions::script()).expect("constant script should parse");
    let without_intl = emit_with_intl_profile(
        &lower(&parsed),
        PromiseRejectionPolicy::default(),
        &custom_profile,
    )
    .expect("unused selected profile should remain inert");
    assert!(components(&without_intl.bytes).is_empty());
    assert!(
        Parser::new(0)
            .parse_all(&without_intl.bytes)
            .all(|payload| {
                !matches!(payload.expect("module should parse"), Payload::CustomSection(section)
            if section.name() == scriptw_intl::INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION)
            })
    );
    let artifact = emit_script("262;").expect("constant script should emit");
    assert!(
        !artifact
            .debug_dump
            .contains("import func: scriptw_host.intl_call"),
        "{}",
        artifact.debug_dump
    );
    assert!(components(&artifact.bytes).is_empty());
    assert!(Parser::new(0).parse_all(&artifact.bytes).all(|payload| {
        !matches!(payload.expect("module should parse"), Payload::CustomSection(section)
            if section.name() == scriptw_intl::INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION)
    }));

    // A compiled primitive catalogue consumes selected data even when it
    // needs no host call. R carries its full image/identity binding.
    let artifact = emit_script("Intl.supportedValuesOf('calendar');")
        .expect("native supported-values catalogue should emit");
    let runtime_bytes = artifact
        .runtime()
        .expect("catalogue program links R")
        .bytes();
    assert!(components(&artifact.bytes).is_empty());
    assert_eq!(components(runtime_bytes).len(), 12);
    let identities = |bytes: &[u8]| {
        Parser::new(0).parse_all(bytes).filter(|payload| {
            matches!(payload.as_ref().expect("module should parse"), Payload::CustomSection(section)
                if section.name() == scriptw_intl::INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION)
        }).count()
    };
    assert_eq!(identities(runtime_bytes), 1);
    assert_eq!(identities(&artifact.bytes), 0);
}

#[test]
fn emitted_functions_declare_only_referenced_temporary_locals() {
    let artifact = emit_script(
        r#"
function ordinary(value) { return value + 1; }
let buffer = new ArrayBuffer(8);
let view = new Uint8Array(buffer);
let parsed = JSON.parse("1.1e-1");
let serialized = JSON.stringify({ parsed });
ordinary(view.byteLength) === 9 && /1/.test(serialized);
"#,
    )
    .expect("ordinary, builtin, JSON, decimal, and RegExp paths should emit");
    expect_valid_module(&artifact, 1);

    let local_counts = declared_function_local_counts(&artifact.bytes);
    let declared_main_local_count = local_counts
        .first()
        .copied()
        .expect("emitted module should contain a main function");
    let reported_main_local_count = artifact
        .debug_dump
        .lines()
        .find_map(|line| line.strip_prefix("locals: "))
        .expect("debug dump should report main locals")
        .parse::<u32>()
        .expect("reported main local count should be numeric");
    assert_eq!(reported_main_local_count, declared_main_local_count);
    let max_local_count = local_counts
        .iter()
        .copied()
        .max()
        .expect("emitted module should contain functions");
    assert!(
        max_local_count < 2048,
        "emitted function retained temporary-local planning capacity: {max_local_count}"
    );
}

#[test]
fn named_class_accessor_capture_module_validates() {
    let artifact = emit_script(
        "var seen;
             class C {
                 get value() { return C; }
                 set value(next) { seen = C; }
             }
             const original = C;
             const instance = new C();
             instance.value;
             instance.value = null;
             seen === original;",
    )
    .expect("named class accessors should emit");
    expect_valid_module(&artifact, 3);
}

#[test]
fn ordered_class_elements_module_validates() {
    let artifact = emit_script(
        "let order = '';
             function key(name) { order += name; return name; }
             class C {
                 [key('a')]() {}
                 static first = (order += '1', C.later());
                 static { order += '2'; }
                 [key('b')]() {}
                 static #private = (order += '3', 3);
                 static later() { return 1; }
                 before = #instance in this;
                 #instance = 1;
             }
             new C();
             order === 'ab123';",
    )
    .expect("ordered class elements should emit");
    expect_valid_module(&artifact, 8);
}

#[test]
fn class_instance_element_boundaries_module_validates() {
    let artifact = emit_script(
        "let order = '';
             class Base { constructor() { order += 's'; } }
             class Derived extends Base {
                 field = (order += 'f', 1);
                 constructor(value = (order += 'p', 1)) {
                     order += value;
                     (() => super())();
                     order += 'a';
                 }
             }
             new Derived();
             order === 'p1sfa';",
    )
    .expect("constructor-bound instance elements should emit");
    expect_valid_module(&artifact, 4);
}

#[test]
fn strict_class_callable_arguments_module_validates() {
    let artifact = emit_script(
        "class C {
                 constructor(value) { value = 2; this.first = arguments[0]; }
                 method(value) { value = 2; return arguments[0]; }
                 get value() { return arguments.length; }
                 set value(next) { next = 2; this.second = arguments[0]; }
                 static method(value) { value = 2; return arguments[0]; }
             }
             const instance = new C(1);
             instance.method(1);
             instance.value = 1;
             C.method(1);",
    )
    .expect("strict class callable arguments should emit");
    expect_valid_module(&artifact, 6);
}

#[test]
fn computed_class_field_keys_module_validates() {
    let artifact = emit_script(
        "let calls = 0;
             function key(name) { calls += 1; return name; }
             class C {
                 [key('instance')] = 1;
                 static [key('shared')] = 2;
             }
             const first = new C();
             const second = new C();
             calls === 2 && first.instance === 1 && second.instance === 1 && C.shared === 2;",
    )
    .expect("computed class field keys should emit");
    expect_valid_module(&artifact, 4);
}

#[test]
fn private_expression_operands_are_collected_before_emission() {
    let artifact = emit_script(
        "class C {
                 #value = 0;
                 read() { return (void 'private-read-target-literal', this).#value; }
                 write() {
                     (void 'private-write-target-literal', this).#value =
                         'private-write-value-literal';
                 }
                 has() { return #value in (void 'private-in-rhs-literal', this); }
             }
             const instance = new C();
             instance.read();
             instance.write();
             instance.has();",
    )
    .expect("private expression operands should be collected");
    expect_valid_module(&artifact, 4);
}

#[test]
fn private_in_rhs_boundary_module_validates() {
    let artifact = emit_script(
        "class C {
                 #field;
                 nonObject() {
                     try { #field in {} << 0; } catch (error) {
                         return error.name === 'TypeError';
                     }
                 }
                 unresolvable() {
                     try { #field in missingName; } catch (error) {
                         return error.name === 'ReferenceError';
                     }
                 }
             }
             const instance = new C();
             instance.nonObject() && instance.unresolvable();",
    )
    .expect("private-in RHS boundaries should emit");
    expect_valid_module(&artifact, 3);
}

#[test]
fn private_assignment_reference_module_validates() {
    let artifact = emit_script(
        "class C {
                 #field;
                 assign(iterable, object) {
                     for (this.#field of iterable) {}
                     for (this.#field in object) {}
                     [this.#field, ...this.#field] = iterable;
                     ({ value: this.#field } = { value: 1 });
                     return this.#field;
                 }
             }
             new C().assign([1, 2], { first: 1, second: 2 });",
    )
    .expect("private assignment references should emit");
    expect_valid_module(&artifact, 4);
}

#[test]
fn private_callable_source_names_module_validates() {
    let artifact = emit_script(
        "class C {
                 #instanceMethod() {}
                 static #staticMethod() {}
                 instanceName() { return this.#instanceMethod.name; }
                 static staticName() { return this.#staticMethod.name; }
                 publicMethod() {}
             }
             const instance = new C();
             instance.instanceName() === '#instanceMethod'
                 && C.staticName() === '#staticMethod'
                 && C.publicMethod.name === 'C.publicMethod';",
    )
    .expect("private callable source names should emit");
    expect_valid_module(&artifact, 5);
}

#[test]
fn optional_private_access_module_validates() {
    let artifact = emit_script(
        "class C {
                 #field = 1;
                 get #value() { return this.#field; }
                 #method() { return this; }
                 read(o) { return o?.c.#field; }
                 readGetter(o) { return o?.#value; }
                 call(o) { return o?.#method(); }
             }
             const instance = new C();
             instance.read({ c: instance }) === 1
                 && instance.read(null) === undefined
                 && instance.readGetter(instance) === 1
                 && instance.call(instance) === instance;",
    )
    .expect("optional private access should emit");
    expect_valid_module(&artifact, 7);
}

#[test]
fn json_parse_number_validation_module_validates() {
    let artifact = emit_script(r#"JSON.parse("00");"#).expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn string_pad_end_utf16_prefix_module_validates() {
    let artifact = emit_script(r#""abc".padEnd(6, "\uD83D\uDCA9");"#).expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn string_pad_start_utf16_prefix_module_validates() {
    let artifact = emit_script(r#""abc".padStart(6, "\uD83D\uDCA9");"#).expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn proxy_get_own_property_descriptor_module_validates() {
    let artifact = emit_script(
        r#"var target = {};
Object.defineProperty(target, "attr", { value: 1, configurable: true });
var proxy = new Proxy(target, {});
Object.getOwnPropertyDescriptor(proxy, "attr");"#,
    )
    .expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn proxy_set_define_property_fallback_module_validates() {
    let artifact = emit_script(
        r#"
var desc;
var p = new Proxy({}, {
  defineProperty: function(target, key, candidate) {
    desc = candidate;
    return true;
  }
});
p.a = 0;
desc;
"#,
    )
    .expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn outlined_ordinary_set_receiver_paths_validate() {
    let artifact = emit_script(
        r#"
var setterReceiver;
var prototype = {};
Object.defineProperty(prototype, "value", {
  set(next) { setterReceiver = this; }
});
var receiver = Object.create(prototype);
receiver.value = 1;

var symbol = Symbol("value");
Reflect.set(receiver, symbol, 2);

function updateMapped(argument) {
  Object.defineProperty(arguments, "0", { writable: true });
  arguments[0] = 3;
  return argument;
}
updateMapped(2);

var array = [];
Reflect.set(array, "length", 1);

var target = {};
Object.defineProperty(target, "fixed", {
  configurable: false,
  writable: false,
  value: 4
});
var proxy = new Proxy(target, { set() { return true; } });
try { proxy.fixed = 5; } catch (error) {}
setterReceiver === receiver;
"#,
    )
    .expect("ordinary receiver set paths should emit");
    expect_valid_module(&artifact, 3);
}

#[test]
fn string_script_emits_memory_and_data() {
    let artifact = emit_script("const s = \"hi\"; s;").expect("emit should work");
    expect_valid_module(&artifact, 0);
    assert!(artifact.runtime().is_some(), "GC string literal links R");
    assert!(Parser::new(0).parse_all(&artifact.bytes).any(|payload| {
        let Payload::ExportSection(reader) = payload.expect("program should parse") else {
            return false;
        };
        reader.into_iter().any(|export| {
            let export = export.expect("export should parse");
            export.name == "memory"
                && export.kind == wasmparser::ExternalKind::Memory
                && export.index == 0
        })
    }));
    let units = data_segment_at(&artifact.bytes, 0);
    assert!(
        units.windows(4).any(|units| units == b"h\0i\0"),
        "literal UTF-16 remains in P"
    );
    let segments = Parser::new(0)
        .parse_all(&artifact.bytes)
        .filter_map(|payload| {
            let Payload::DataSection(reader) = payload.expect("program should parse") else {
                return None;
            };
            Some(
                reader
                    .into_iter()
                    .map(|segment| {
                        assert!(matches!(
                            segment.expect("segment should parse").kind,
                            wasmparser::DataKind::Passive
                        ));
                    })
                    .count(),
            )
        })
        .sum::<usize>();
    assert_eq!(
        segments, 4,
        "P owns UTF-16, static bytes, String descriptors and pure Array recipes"
    );
}

/// `(passive, active)` data segment counts. Every active segment must be
/// the preseeded wire data, written at `STATIC_DATA_OFFSET` of memory 0.
fn wire_data_segment_counts(bytes: &[u8]) -> (usize, usize) {
    let mut passive_segments = 0;
    let mut active_segments = 0;
    for payload in Parser::new(0).parse_all(bytes) {
        let Payload::DataSection(reader) = payload.expect("module should parse") else {
            continue;
        };
        for segment in reader {
            let segment = segment.expect("segment should decode");
            match segment.kind {
                wasmparser::DataKind::Passive => passive_segments += 1,
                wasmparser::DataKind::Active {
                    memory_index,
                    offset_expr,
                } => {
                    active_segments += 1;
                    assert_eq!(memory_index, 0);
                    let mut offset = offset_expr.get_operators_reader();
                    assert!(
                        matches!(offset.read().expect("wire offset"), Operator::I32Const { value } if value == STATIC_DATA_OFFSET as i32)
                    );
                    assert!(matches!(
                        offset.read().expect("wire offset end"),
                        Operator::End
                    ));
                    assert!(offset.eof());
                }
            }
        }
    }
    (passive_segments, active_segments)
}

#[test]
fn scalar_restricted_global_declarations_link_the_intrinsic_error_runtime() {
    for source in ["let undefined;", "const NaN = 0;", "let Infinity;"] {
        for directive in ["", "'use strict';\n"] {
            let artifact = emit_script(&format!("{directive}{source}")).expect("emit Script");
            expect_valid_module(&artifact, 0);
            assert!(artifact.runtime().is_some(), "{directive}{source}");
        }
    }
    let scalar = emit_script("40 + 2;").expect("emit ordinary scalar computation");
    expect_valid_module(&scalar, 0);
    assert!(scalar.runtime().is_none());
}

#[test]
fn preseeded_wire_data_stays_in_private_memory() {
    let mut expected_prefix = vec![b' '; 11];
    expected_prefix.extend_from_slice(b"\n: ,undefinednulltruefalse");

    // A runtime-free program is one self-contained module that owns its
    // preseed as the only (active) segment.
    let standalone = emit_script("\",\";").expect("emit should work");
    expect_valid_module(&standalone, 0);
    assert!(standalone.runtime().is_none());
    assert!(data_segment_at(&standalone.bytes, 0).starts_with(&expected_prefix));
    assert_eq!(wire_data_segment_counts(&standalone.bytes), (0, 1));

    // A heap program's preseed is R's one active segment; P only appends
    // passive suffixes to it.
    let artifact = emit_script("({ value: \",\" });").expect("emit should work");
    expect_valid_module(&artifact, 0);
    let runtime = artifact.runtime().expect("heap program links R");
    assert!(data_segment_at(runtime.bytes(), 1).starts_with(&expected_prefix));
    assert_eq!(wire_data_segment_counts(runtime.bytes()), (1, 1));
    assert_eq!(wire_data_segment_counts(&artifact.bytes), (4, 0));
    assert!(data_segment_at(&artifact.bytes, 3).is_empty());
    assert!(
        artifact
            .gc_host_imports()
            .contains(&GcHostImport::CollectGc)
    );
}

// P repeats its exact R's host imports, then imports R's defined
// functions by their original index and type. This checks all scalar and
// GC host roles without accepting an arbitrary second import namespace.
pub(crate) fn linked_function_imports(runtime: &RuntimeArtifact) -> Vec<(String, String, u32)> {
    let mut imports = Vec::new();
    let mut defined_types = Vec::new();
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        match payload.expect("actual R parses") {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.expect("actual R import parses");
                    if let wasmparser::TypeRef::Func(index)
                    | wasmparser::TypeRef::FuncExact(index) = import.ty
                    {
                        assert_eq!(import.module, crate::module::HOST_IMPORT_MODULE);
                        imports.push((import.module.to_owned(), import.name.to_owned(), index));
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                defined_types.extend(reader.into_iter().map(Result::unwrap));
            }
            _ => {}
        }
    }
    let host_count = imports.len();
    imports.extend(defined_types.into_iter().enumerate().map(|(position, ty)| {
        (
            crate::RUNTIME_IMPORT_NAMESPACE.to_owned(),
            format!("f{}", host_count + position),
            ty,
        )
    }));
    imports
}

fn regexp_descriptor_from_pool(
    pool: &StringPool,
    reference: RegExpProgramRef,
) -> scriptw_ir::ValidatedRegExpProgram {
    let offset = (reference.payload() >> 32) as usize - STATIC_DATA_OFFSET as usize;
    let length = reference.payload() as u32 as usize;
    scriptw_ir::ValidatedRegExpProgram::from_bytes(pool.bytes[offset..offset + length].to_vec())
        .unwrap()
}

#[test]
fn regexp_program_wire_data_is_aligned_and_deduplicated() {
    let artifact = emit_script("\",\"; /[a-c]/; /[a-c]/g;").expect("emit should work");
    let program = scriptw_ir::RegExpProgram::compile("[a-c]", "").unwrap();
    let encoded = scriptw_ir::ValidatedRegExpProgram::from_program(&program).unwrap();
    let runtime = artifact.runtime().expect("RegExp program links R");
    let runtime_data = data_segment_at(runtime.bytes(), 1);
    let data = data_segment_at(&artifact.bytes, 1);
    let offsets = data
        .windows(encoded.bytes().len())
        .enumerate()
        .filter_map(|(offset, candidate)| (candidate == encoded.bytes()).then_some(offset))
        .collect::<Vec<_>>();
    assert_eq!(offsets.len(), 1);
    let pointer = STATIC_DATA_OFFSET as usize + runtime_data.len() + offsets[0];
    assert_eq!(pointer % 8, 0);
    expect_valid_module(&artifact, 0);
    assert!(
        global_init_i64s(runtime.bytes()).contains(&(align_heap_start(runtime_data.len()) as i64))
    );
    // The aligned address above is a virtual offset into P's immutable
    // metadata image, not an installation in R's linear scratch memory.
    // No P bootstrap, main or outlined body may install this image or move
    // the shared R cursor past it.
    let mut bodies = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("program should parse") else {
            continue;
        };
        bodies += 1;
        for operator in body.get_operators_reader().expect("P operators") {
            match operator.expect("P operator decodes") {
                Operator::GlobalSet { global_index } => assert_ne!(
                    global_index, PRIVATE_BYTE_CURSOR_GLOBAL_INDEX,
                    "P metadata must not move R's scratch cursor"
                ),
                Operator::MemoryInit { data_index, .. } => assert_ne!(
                    data_index,
                    StringPool::PROGRAM_STATIC_DATA_SEGMENT,
                    "P metadata stays in its immutable GC image"
                ),
                _ => {}
            }
        }
    }
    assert!(bodies > 0, "the actual emitted P bodies must be checked");
    assert_eq!(wire_data_segment_counts(&artifact.bytes), (4, 0));
}

#[test]
fn regexp_static_program_descriptors_preserve_capture_and_choice_metadata() {
    use scriptw_ir::RegExpProgramWord as Word;
    for (pattern, captures, splits, repeated) in [
        ("a?a?", 0, 2, 0),
        ("a?b*", 0, 2, 1),
        ("(a|b)*", 1, 2, 2),
        (r"(?<=\w+)f", 0, 2, 1),
    ] {
        let program = scriptw_ir::RegExpProgram::compile(pattern, "").unwrap();
        let mut pool = StringPool::default();
        let reference = pool.collect_regexp_program_for_test(&program);
        let descriptor = regexp_descriptor_from_pool(&pool, reference);
        assert_eq!(descriptor.word(Word::CaptureCount), captures, "{pattern}");
        assert_eq!(descriptor.word(Word::SplitCount), splits, "{pattern}");
        assert_eq!(
            descriptor.word(Word::RepeatableSplitCount),
            repeated,
            "{pattern}"
        );
    }
}

#[test]
fn regexp_static_program_dedup_key_includes_capture_count() {
    let original = scriptw_ir::RegExpProgram::compile("a", "").unwrap();
    let mut with_capture = original.clone();
    with_capture.capture_count = 1;
    assert_eq!(original.encode(), with_capture.encode());
    let mut pool = StringPool::default();
    let first = pool.collect_regexp_program_for_test(&original);
    let second = pool.collect_regexp_program_for_test(&with_capture);
    assert_ne!(first.payload(), second.payload());
    assert_eq!(
        regexp_descriptor_from_pool(&pool, first).word(scriptw_ir::RegExpProgramWord::CaptureCount),
        0
    );
    assert_eq!(
        regexp_descriptor_from_pool(&pool, second)
            .word(scriptw_ir::RegExpProgramWord::CaptureCount),
        1
    );
}

#[test]
fn constructed_constant_regexp_retains_one_validated_program_image() {
    let artifact = emit_script(r#"/(a|b)*/; new RegExp("(a|b)*", "");"#).unwrap();
    let program = scriptw_ir::RegExpProgram::compile("(a|b)*", "").unwrap();
    let encoded = scriptw_ir::ValidatedRegExpProgram::from_program(&program).unwrap();
    let data = data_segment_bytes(&artifact.bytes);
    let positions = data
        .windows(encoded.bytes().len())
        .enumerate()
        .filter_map(|(offset, candidate)| (candidate == encoded.bytes()).then_some(offset))
        .collect::<Vec<_>>();
    assert_eq!(positions.len(), 1);
    expect_valid_module(&artifact, 0);
}

#[test]
fn large_static_string_data_increases_initial_memory_pages() {
    let source = format!("\"{}\";", "x".repeat(WASM_PAGE_SIZE as usize));
    let artifact = emit_script(&source).expect("emit should work");
    let pages = memory_initial_pages(&artifact.bytes);
    assert!(pages[0] >= 2);
}

#[test]
fn supports_assignment_branching_and_loops() {
    let artifact = emit_script(
        "let i = 0; let sum = 0; for (; i < 5; i = i + 1) { if (i === 2) { continue; } if (i === 4) { break; } sum = sum + i; } sum;",
    )
    .expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn supports_updates_and_compound_assignment() {
    let artifact = emit_script("let sum = 0; for (let i = 0; i < 4; i++) { sum += i; } sum;")
        .expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn supports_switch_labels_and_debugger() {
    let artifact = emit_script(
        "let x = 0; outer: while (x < 3) { x += 1; switch (x) { case 1: continue outer; case 2: debugger; break outer; default: break; } } x;",
    )
    .expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn supports_direct_function_calls_and_recursion() {
    let artifact =
        emit_script("function up(n) { if (n === 0) { return 0; } return up(n - 1) + 1; } up(3);")
            .expect("emit should work");
    expect_valid_module(&artifact, 1);
    assert!(artifact.debug_dump.contains("internal functions: "));
}

#[test]
fn outlined_function_call_preserves_receiver_and_arguments_module_validates() {
    let artifact = emit_script(
        "function combine(left, right) { return this.base + left + right; } let receiver = { base: 4, combine: combine }; receiver.combine(2, 3);",
    )
    .expect("emit should work");
    expect_valid_module(&artifact, 1);
}

#[test]
fn outlined_function_call_throw_routing_module_validates() {
    let artifact = emit_script(
        "function fail(value) { throw value; } let caught = 0; try { fail(7); } catch (error) { caught = error; } caught;",
    )
    .expect("emit should work");
    expect_valid_module(&artifact, 1);
}

#[test]
fn outlined_proxy_fallback_call_module_validates() {
    let artifact = emit_script(
        "function combine(left, right) { return this.base + left + right; } let callable = new Proxy(combine, {}); let receiver = { base: 4, callable: callable }; receiver.callable(2, 3);",
    )
    .expect("emit should work");
    expect_valid_module(&artifact, 1);
}

#[test]
fn outlined_dynamic_property_read_normalizes_computed_key_once_module_validates() {
    let artifact = emit_script(
        "let calls = 0; let key = { [Symbol.toPrimitive]() { calls += 1; return \"value\"; } }; let object = { value: 3 }; let absent = null; let skipped = absent?.[key]; let read = object?.[key]; calls === 1 && skipped === undefined && read === 3;",
    )
    .expect("emit should work");
    expect_valid_module(&artifact, 1);
}

#[test]
fn outlined_dynamic_property_read_preserves_runtime_exotics_module_validates() {
    let artifact = emit_script(
        "function readArguments() { return arguments?.length === 3 && arguments?.[1] === 2; } let key = Symbol(\"key\"); let object = { [key]: 5 }; let values = [1, 2]; \"ab\"?.[1] === \"b\" && values?.length === 2 && object?.[key] === 5 && readArguments(1, 2, 3);",
    )
    .expect("emit should work");
    expect_valid_module(&artifact, 1);
}

#[test]
fn outlined_dynamic_property_read_preserves_proxy_receiver_module_validates() {
    let artifact = emit_script(
        "let seen = false; let proxy; let target = { get value() { return this === proxy ? 7 : 0; } }; proxy = new Proxy(target, { get(target, key, receiver) { seen = key === \"value\" && receiver === proxy; return Reflect.get(target, key, receiver); } }); proxy?.value === 7 && seen;",
    )
    .expect("emit should work");
    expect_valid_module(&artifact, 1);
}

#[test]
fn ordinary_computed_dynamic_property_reads_have_bounded_incremental_body_growth() {
    let single_read = emit_script(
        r#"
function choose(flag) { return flag ? { value: 1 } : null; }
let object = choose(true);
let key = "value";
object[key];
"#,
    )
    .expect("single ordinary computed property read should emit");
    let repeated_reads = emit_script(
        r#"
function choose(flag) { return flag ? { value: 1 } : null; }
let object = choose(true);
let key = "value";
object[key];
object[key];
object[key];
object[key];
object[key];
object[key];
object[key];
object[key];
object[key];
object[key];
object[key];
object[key];
"#,
    )
    .expect("repeated ordinary computed property reads should emit");
    expect_valid_module(&single_read, 1);
    expect_valid_module(&repeated_reads, 1);

    let main_body_bytes = |artifact: &WasmArtifact| {
        Parser::new(0)
            .parse_all(&artifact.bytes)
            .find_map(
                |payload| match payload.expect("wasm parse should succeed") {
                    Payload::CodeSectionEntry(body) => Some(body.range().len()),
                    _ => None,
                },
            )
            .expect("emitted module should contain a main function")
    };
    let single_read_body_bytes = main_body_bytes(&single_read);
    let repeated_read_body_bytes = main_body_bytes(&repeated_reads);
    let incremental_body_bytes = repeated_read_body_bytes
        .checked_sub(single_read_body_bytes)
        .expect("repeated reads should not shrink the main function");
    assert!(
        incremental_body_bytes < 64 * 1024,
        "eleven additional outlined reads added {incremental_body_bytes} bytes \
             ({single_read_body_bytes} -> {repeated_read_body_bytes})"
    );
}

#[test]
fn with_has_binding_has_bounded_incremental_function_body_growth() {
    let emit_reads = |count| {
        let reads = "selected;".repeat(count);
        emit_script(&format!(
            "function probe(scope, selected) {{ with (scope) {{ {reads} }} }} probe({{selected: 1}}, 0);"
        ))
        .expect("with binding reads should emit")
    };
    let single = emit_reads(1);
    let repeated = emit_reads(10);
    expect_valid_module(&single, 1);
    expect_valid_module(&repeated, 1);
    let largest_probe = |artifact: &WasmArtifact| {
        artifact
            .function_sizes
            .iter()
            .filter(|body| body.name.starts_with("js::probe#"))
            .map(|body| body.body_bytes.bytes())
            .max()
            .expect("the probe body must be emitted")
    };
    let single_bytes = largest_probe(&single);
    let repeated_bytes = largest_probe(&repeated);
    let growth = repeated_bytes
        .checked_sub(single_bytes)
        .expect("additional observable reads must not shrink the body");
    // The complete ResolveBinding/GetBindingValue dispatch belongs in R;
    // repeated observable reads in P retain only rooted helper operands.
    assert!(
        growth < 9_000,
        "nine With reads added {growth} bytes ({single_bytes} -> {repeated_bytes})"
    );
    for (artifact, count) in [(&single, 1), (&repeated, 10)] {
        let runtime = artifact.runtime().expect("With reads link R");
        let runtime_names = function_names_in_bytes(runtime.bytes());
        let indices = [
            "helper::with_environment_has_binding",
            "helper::with_environment_get_binding_value_sloppy",
        ]
        .map(|name| {
            *runtime_names
                .iter()
                .find(|(_, actual)| actual.as_str() == name)
                .expect("the shared lookup body exists")
                .0
        });
        let program_names = function_names(artifact);
        let mut index = imported_function_count_in_bytes(&artifact.bytes);
        let mut found = false;
        for payload in Parser::new(0).parse_all(&artifact.bytes) {
            if let Payload::CodeSectionEntry(body) = payload.expect("P decodes") {
                if program_names
                    .get(&index)
                    .is_some_and(|name| name.starts_with("js::probe#"))
                {
                    let calls = body
                        .get_operators_reader()
                        .expect("probe opens")
                        .into_iter()
                        .filter_map(|operator| match operator.expect("operator decodes") {
                            Operator::Call { function_index } => Some(function_index),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    assert!(
                        indices.iter().all(|helper| {
                            calls.iter().filter(|callee| *callee == helper).count() == count
                        }),
                        "each actual With read calls its complete shared body once"
                    );
                    found = true;
                }
                index += 1;
            }
        }
        assert!(found, "the actual probe body is inspected");
    }
}

#[test]
fn nested_with_writes_share_dynamic_property_set_dispatch() {
    let emit_writes = |count| {
        let writes = "destination = selected;".repeat(count);
        emit_script(&format!(
            "function probe(outer, inner, destination, selected) {{ \
                 with (outer) {{ with (inner) {{ {writes} }} }} }} \
                 probe({{}}, {{}}, 0, 1);"
        ))
        .expect("nested With assignments should emit")
    };
    let single = emit_writes(1);
    let repeated = emit_writes(10);
    expect_valid_module(&single, 1);
    expect_valid_module(&repeated, 1);
    // R and P share declared function indices. Pin actual call edges,
    // not just the existence of a helper with the right name.
    for artifact in [&single, &repeated] {
        let runtime = artifact.runtime().expect("With writes link R");
        let runtime_names = function_names_in_bytes(runtime.bytes());
        let helper_index = |name: &str| {
            let indices = runtime_names
                .iter()
                .filter_map(|(&index, actual)| (actual == name).then_some(index))
                .collect::<Vec<_>>();
            assert_eq!(indices.len(), 1, "{name} has one body in R");
            indices[0]
        };
        let sloppy = helper_index("helper::environment_identifier_put_sloppy");
        let strict = helper_index("helper::environment_identifier_put_strict");
        let set = helper_index("helper::ordinary_set");
        let calls_by_index = |bytes: &[u8]| {
            let mut next = imported_function_count_in_bytes(bytes);
            let mut calls = BTreeMap::new();
            for payload in Parser::new(0).parse_all(bytes) {
                if let Payload::CodeSectionEntry(body) = payload.expect("module decodes") {
                    let direct = body
                        .get_operators_reader()
                        .expect("body opens")
                        .into_iter()
                        .filter_map(|operator| match operator.expect("operator decodes") {
                            Operator::Call { function_index }
                            | Operator::ReturnCall { function_index } => Some(function_index),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    calls.insert(next, direct);
                    next += 1;
                }
            }
            calls
        };
        let runtime_calls = calls_by_index(runtime.bytes());
        for index in [sloppy, strict] {
            let calls = &runtime_calls[&index];
            assert!(
                calls.contains(&set),
                "PutValue keeps the actual OrdinarySet call"
            );
            assert!(
                !calls.contains(&sloppy) && !calls.contains(&strict),
                "a PutValue helper must never reenter its own facade"
            );
        }
        let program_calls = calls_by_index(&artifact.bytes);
        let names = function_names(artifact);
        let probes = names
            .iter()
            .filter(|(_, name)| name.starts_with("js::probe#"))
            .map(|(&index, _)| index)
            .collect::<Vec<_>>();
        assert!(!probes.is_empty());
        // The write's Set is R's one dynamic dispatch: the probe calls
        // `ordinary_set` directly instead of expanding it per site.
        assert!(
            probes
                .iter()
                .any(|index| program_calls[index].contains(&set)),
            "the actual nested With probe calls the shared OrdinarySet"
        );
        let mut imported = 0u32;
        let mut matched_import = false;
        for payload in Parser::new(0).parse_all(&artifact.bytes) {
            if let Payload::ImportSection(section) = payload.expect("P decodes") {
                for import in section.into_imports() {
                    let import = import.expect("import decodes");
                    if matches!(
                        import.ty,
                        wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                    ) {
                        if imported == set {
                            assert_eq!(
                                import.module,
                                crate::runtime_artifact::RUNTIME_IMPORT_NAMESPACE
                            );
                            assert_eq!(import.name, format!("f{set}"));
                            matched_import = true;
                        }
                        imported += 1;
                    }
                }
            }
        }
        assert!(
            matched_import,
            "P's call reaches R's exact exported function"
        );
    }
    let largest_probe = |artifact: &WasmArtifact| {
        artifact
            .function_sizes
            .iter()
            .filter(|body| body.name.starts_with("js::probe#"))
            .map(|body| body.body_bytes.bytes())
            .max()
            .expect("the probe body must be emitted")
    };
    let single_bytes = largest_probe(&single);
    let repeated_bytes = largest_probe(&repeated);
    let growth = repeated_bytes
        .checked_sub(single_bytes)
        .expect("additional observable assignments must not shrink the body");
    // The pre-outline baseline added 538,272 bytes; the bound is 180,000.
    // `WithEnvironmentReferencePlan::put_value` retains the HasBinding
    // selection and evaluates the RHS (a nested With read, ~6.5 KB) once,
    // so each statement carries one copy: 130,842 bytes today. Cloning the
    // RHS into every Object Environment branch and the fallback measured
    // 247,068.
    assert!(
        growth < 180_000,
        "nine nested With assignments added {growth} bytes ({single_bytes} -> {repeated_bytes})"
    );
}

#[test]
fn statically_nullish_computed_property_read_emits_after_throw_path() {
    let artifact = emit_script(
        r#"
let calls = 0;
function key() { calls += 1; return "value"; }
try { null[key()]; } catch (error) {}
calls;
"#,
    )
    .expect("statically nullish computed property read should emit");
    expect_valid_module(&artifact, 1);
}

#[test]
fn object_and_array_scripts_emit_memory_with_agent_capability_import() {
    let artifact =
        emit_script("let o = { x: 1 }; let a = [1]; a[2] = 4; o.x;").expect("emit should work");
    expect_valid_module(&artifact, 0);
    assert!(
        artifact
            .debug_dump
            .contains("import func: scriptw_host.agent_can_suspend")
    );
    assert!(
        artifact
            .debug_dump
            .contains("memory: exported private linear memory")
    );
    assert!(artifact.debug_dump.contains("data segments: 1"));
}

#[test]
fn test262_agent_builtins_import_the_agent_call_and_split_memories() {
    let artifact =
        emit_script("__scriptwAgentSleep(1);").expect("Test262 agent host call should emit");
    expect_valid_module(&artifact, 0);

    assert!(
        artifact
            .debug_dump
            .contains("import func: scriptw_host.agent_call")
    );
    assert!(
        artifact
            .debug_dump
            .contains("import memory: scriptw_host.private_memory")
    );
    assert!(
        artifact
            .debug_dump
            .contains("import memory: scriptw_host.shared_memory")
    );
}

#[test]
fn product_lowering_cannot_reauthorize_a_test262_name_in_aot() {
    let source =
        parse("__scriptwAgentSleep;", ParseOptions::script()).expect("product script should parse");
    let program = lower(&source);
    // Every heap-backed module compiles every host builtin body; only the
    // script's own host surface decides which names resolve.
    assert!(
        program
            .script
            .as_ref()
            .expect("script should lower")
            .host_builtins
            .is_empty()
    );
    emit(&program).expect("an unresolved global identifier is handled at runtime");
}

#[test]
fn supports_sparse_array_assignment_module_validates() {
    let artifact = emit_script("let a = [1]; a[2] = 4; a[2];").expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn supports_object_property_assignment_module_validates() {
    let artifact = emit_script("let o = {}; o.x = 1; o.x;").expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn supports_object_return_from_function() {
    let artifact =
        emit_script("function box(x) { let o = { x: x }; return o; } let o = box(2); o.x;")
            .expect("emit should work");
    expect_valid_module(&artifact, 1);
}

#[test]
fn supports_chained_heap_access_and_array_length() {
    let artifact = emit_script(
        "function box() { let o = { inner: { x: 2 } }; return o; } let a = [1, 2, 3]; box().inner.x + a.length;",
    )
    .expect("emit should work");
    expect_valid_module(&artifact, 1);
}

#[test]
fn supports_heap_growth_beyond_initial_capacity() {
    let source = format!(
        "let o = {{}}; {} o.k64;",
        (0..65)
            .map(|index| format!("o[\"k{index}\"] = {index};"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let artifact = emit_script(&source).expect("emit should work");
    expect_valid_module(&artifact, 0);
    assert!(
        artifact
            .debug_dump
            .contains("memory: exported private linear memory")
    );
}

#[test]
fn supports_dynamic_primitive_string_concat() {
    let artifact = emit_script("\"a\" + \"b\";").expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn supports_primitive_coercion_core() {
    let artifact = emit_script("1 == \"1\"; \"2\" - 1; \"10\" > \"2\"; void 1; (1, 2);")
        .expect("emit should work");
    expect_valid_module(&artifact, 0);
}

#[test]
fn supports_heap_coercion_core() {
    let artifact = emit_script(
        "\"a\" + {}; let o = { valueOf() { return 2; } }; o + 1; [1, 2] + 3; ({}) == 1; [2] < 3; function f() { return arguments + \"\"; } f(1, 2);",
    )
    .expect("heap coercion should emit");
    expect_valid_module(&artifact, 2);
}

#[test]
fn supports_dynamic_value_plus_proven_string() {
    let artifact = emit_script(
        "function choose(flag) { if (flag) return 1; return {}; } function format(message) { return message + \" suffix\"; } format(choose(true));",
    )
    .expect("dynamic plus string should emit");
    expect_valid_module(&artifact, 2);
}

#[test]
fn supports_host_gc_builtin_as_explicit_unsupported_throw() {
    let artifact = emit_script("if (typeof gc === \"function\") { gc(); }")
        .expect("gc host builtin should emit");
    expect_valid_module(&artifact, 1);
}

#[test]
fn supports_typeof_heap_values() {
    let object_artifact =
        emit_script("let obj = {}; typeof obj;").expect("object typeof script should emit");
    expect_valid_module(&object_artifact, 0);

    let function_artifact = emit_script("let f = function() {}; typeof f;")
        .expect("function typeof script should emit");
    expect_valid_module(&function_artifact, 1);
}

#[test]
fn supports_global_var_object_write() {
    let artifact = emit_script("var x = 1; x;").expect("var write script should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn proxy_eval_target_module_validates() {
    let artifact =
        emit_script("var proxy = new Proxy(eval, {}); proxy();").expect("proxy script should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn proxy_non_callable_target_call_module_validates() {
    let artifact = emit_script(
        r#"var p = new Proxy({}, {});
try {
  p();
} catch (error) {
  error instanceof TypeError;
}"#,
    )
    .expect("proxy non-callable script should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn proxy_revocation_function_metadata_module_validates() {
    let artifact = emit_script(
        r#"var revocationFunction = Proxy.revocable({}, {}).revoke;
Object.getOwnPropertyDescriptor(revocationFunction, "length");
Object.getOwnPropertyDescriptor(revocationFunction, "name");
Object.getOwnPropertyNames(revocationFunction);"#,
    )
    .expect("proxy revocation metadata script should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn typedarray_own_property_keys_module_validates() {
    let artifact = emit_script(
        r#"var buffer = new ArrayBuffer(4, { maxByteLength: 8 });
var view = new Uint8Array(buffer, 1);
var symbol = Symbol("key");
view.visible = 1;
Object.defineProperty(view, "hidden", { value: 2, enumerable: false });
view[symbol] = 3;
Reflect.ownKeys(view);
Object.getOwnPropertyNames(view);
Reflect.ownKeys(view.subarray(1));
buffer.resize(8);
Reflect.ownKeys(view);
buffer.resize(0);
Reflect.ownKeys(view);"#,
    )
    .expect("typed array own property keys script should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn object_seal_module_validates() {
    let artifact = emit_script(
        r#"var target = { value: 1 };
Object.seal(target);
Object.getOwnPropertyDescriptor(target, "value");
var array = [1];
Object.seal(array);
var proxy = new Proxy({ x: 1 }, {
  preventExtensions: function(target) {
    return Reflect.preventExtensions(target);
  },
  ownKeys: function(target) {
    return Reflect.ownKeys(target);
  },
  defineProperty: function(target, key, descriptor) {
    return Reflect.defineProperty(target, key, descriptor);
  }
});
Object.seal(proxy);"#,
    )
    .expect("Object.seal script should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn typedarray_get_own_property_descriptor_module_validates() {
    let artifact = emit_script(
        r#"var numeric = new Uint8Array([42]);
Object.getOwnPropertyDescriptor(numeric, "0");
Object.getOwnPropertyDescriptor(numeric, "-0");
Object.getOwnPropertyDescriptor(numeric, "1.0");
Object.getOwnPropertyDescriptor(numeric, "1.1");
Object.getOwnPropertyDescriptor(numeric, "Infinity");
var bigint = new BigInt64Array([42n]);
Object.getOwnPropertyDescriptor(bigint, 0);
var detached = new Uint8Array([1]);
__scriptwDetachArrayBuffer(detached.buffer);
Object.getOwnPropertyDescriptor(detached, 0);
var other = __scriptwCreateRealm().global;
var otherDetached = new other.Uint8Array(1);
__scriptwDetachArrayBuffer(otherDetached.buffer);
Object.getOwnPropertyDescriptor(otherDetached, 0);"#,
    )
    .expect("typed array get own property script should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn typedarray_has_property_module_validates() {
    let artifact = emit_script(
        r#"var view = new Uint8Array([42, 43]);
Reflect.has(view, 0);
Reflect.has(view, "-0");
Reflect.has(view, "1.0");
Reflect.has(view, "Infinity");
var bigint = new BigInt64Array([42n]);
Reflect.has(bigint, 0);
var buffer = new ArrayBuffer(4, { maxByteLength: 8 });
var tracking = new Uint8Array(buffer, 1);
Reflect.has(tracking, 2);
buffer.resize(1);
Reflect.has(tracking, 0);
var detached = new Uint8Array([1]);
__scriptwDetachArrayBuffer(detached.buffer);
Reflect.has(detached, 0);"#,
    )
    .expect("typed array has property script should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn typedarray_delete_module_validates() {
    let artifact = emit_script(
        r#"var numeric = new Uint8Array([42]);
delete numeric[0];
delete numeric["-0"];
delete numeric["1.1"];
Reflect.deleteProperty(numeric, "Infinity");
var bigint = new BigInt64Array([42n]);
delete bigint[0];
var shared = new Uint8Array(new SharedArrayBuffer(1));
delete shared[0];
var detached = new Uint8Array([1]);
__scriptwDetachArrayBuffer(detached.buffer);
delete detached[0];
function strictDelete(view) {
  "use strict";
  delete view[0];
}
try { strictDelete(numeric); } catch (error) {}
var proxy = new Proxy(numeric, { deleteProperty: function() { return true; } });
delete proxy[0];"#,
    )
    .expect("typed array delete script should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn atomics_modules_import_private_and_capped_shared_memories() {
    /// The memory index of the integer atomics every width of
    /// `Atomics.load/store/add/compareExchange` compiles to.
    fn atomic_memory(operator: &Operator) -> Option<u32> {
        macro_rules! memory_of {
            ($($variant:ident),+) => {
                match operator {
                    $(Operator::$variant { memarg } => Some(memarg.memory),)+
                    _ => None,
                }
            };
        }
        memory_of!(
            I64AtomicLoad8U,
            I64AtomicLoad16U,
            I64AtomicLoad32U,
            I64AtomicLoad,
            I64AtomicStore8,
            I64AtomicStore16,
            I64AtomicStore32,
            I64AtomicStore,
            I64AtomicRmw8AddU,
            I64AtomicRmw16AddU,
            I64AtomicRmw32AddU,
            I64AtomicRmwAdd,
            I64AtomicRmw8CmpxchgU,
            I64AtomicRmw16CmpxchgU,
            I64AtomicRmw32CmpxchgU,
            I64AtomicRmwCmpxchg
        )
    }

    for source in [
        "var view = new Int32Array(new SharedArrayBuffer(8)); view[0] = 1; Atomics.add(view, 0, 2); Atomics.compareExchange(view, 0, 3, 4); view[0];",
        "var view = new Int32Array(new SharedArrayBuffer(8)); var add = Atomics['add']; add(view, 0, 2);",
    ] {
        let artifact = emit_script(source).expect("Atomics script should emit");
        expect_valid_module(&artifact, 0);

        // R and P import the same host memories: the private heap memory
        // and the capped shared memory that backs SharedArrayBuffer.
        let runtime = artifact.runtime().expect("Atomics program links R");
        let mut atomic_memory_indexes = Vec::new();
        for bytes in [runtime.bytes(), artifact.bytes.as_slice()] {
            let mut memory_imports = Vec::new();
            for payload in Parser::new(0).parse_all(bytes) {
                match payload.expect("wasm parse should succeed") {
                    Payload::ImportSection(reader) => {
                        for imports in reader {
                            match imports.expect("import should decode") {
                                wasmparser::Imports::Single(_, import) => {
                                    if let wasmparser::TypeRef::Memory(memory) = import.ty {
                                        memory_imports.push((
                                            import.name.to_string(),
                                            memory.shared,
                                            memory.maximum,
                                        ));
                                    }
                                }
                                wasmparser::Imports::Compact1 { items, .. } => {
                                    for import in items {
                                        let import = import.expect("compact import should decode");
                                        if let wasmparser::TypeRef::Memory(memory) = import.ty {
                                            memory_imports.push((
                                                import.name.to_string(),
                                                memory.shared,
                                                memory.maximum,
                                            ));
                                        }
                                    }
                                }
                                wasmparser::Imports::Compact2 { ty, names, .. } => {
                                    if let wasmparser::TypeRef::Memory(memory) = ty {
                                        for name in names {
                                            memory_imports.push((
                                                name.expect("compact import name should decode")
                                                    .to_string(),
                                                memory.shared,
                                                memory.maximum,
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Payload::CodeSectionEntry(body) => {
                        let mut reader = body
                            .get_operators_reader()
                            .expect("operators should decode");
                        while !reader.eof() {
                            atomic_memory_indexes.extend(atomic_memory(
                                &reader.read().expect("operator should decode"),
                            ));
                        }
                    }
                    _ => {}
                }
            }

            assert_eq!(
                memory_imports,
                vec![
                    ("private_memory".to_string(), false, None),
                    ("shared_memory".to_string(), true, Some(16_384)),
                ],
                "source: {source}"
            );
        }
        assert!(!atomic_memory_indexes.is_empty(), "source: {source}");
        assert!(
            atomic_memory_indexes.iter().all(|index| *index == 1),
            "source: {source}"
        );
    }
}

#[test]
fn atomics_wait_async_modules_import_monotonic_timeout_host_functions() {
    let artifact = emit_script(
        "var view = new Int32Array(new SharedArrayBuffer(4)); Atomics.waitAsync(view, 0, 0, 1);",
    )
    .expect("Atomics.waitAsync script should emit");
    expect_valid_module(&artifact, 0);

    assert!(
        artifact
            .debug_dump
            .contains("import func: scriptw_host.monotonic_clock_nanos")
    );
    assert!(
        artifact
            .debug_dump
            .contains("import func: scriptw_host.sleep_nanos")
    );
}

#[test]
fn typedarray_define_own_property_module_validates() {
    let artifact = emit_script(
        r#"var numeric = new Uint8Array([1]);
Object.defineProperty(numeric, 0, { value: 2 });
Reflect.defineProperty(numeric, "-0", { value: 3 });
Reflect.defineProperty(numeric, "1.1", { value: 4 });
Reflect.defineProperty(numeric, "Infinity", { value: 5 });
var bigint = new BigInt64Array([1n]);
Object.defineProperty(bigint, 0, { value: 2n, configurable: true });
var ordinary = Symbol("ordinary");
Reflect.defineProperty(numeric, "1.0", { value: 6 });
Reflect.defineProperty(numeric, ordinary, { value: 7 });
var detached = new Uint8Array([1]);
__scriptwDetachArrayBuffer(detached.buffer);
Reflect.defineProperty(detached, 0, { value: 2 });
var proxy = new Proxy(numeric, { defineProperty: function() { return true; } });
Object.defineProperty(proxy, 0, { value: 8 });"#,
    )
    .expect("typed array define own property script should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn typedarray_set_module_validates() {
    let artifact = emit_script(
        r#"var target = new Uint8Array([1]);
var directValue = { valueOf: function() { return 2; } };
target["-0"] = directValue;
target["1.1"] = directValue;
target["-1"] = directValue;
Reflect.set(target, 0, directValue);
Reflect.set(target, "1.1", directValue);
var receiver = {};
Reflect.set(target, 0, directValue, receiver);
Reflect.set(target, "1.1", directValue, receiver);
var typedReceiver = new Uint8Array([0]);
Reflect.set(target, 0, 257, typedReceiver);
var inheritedReceiver = Object.create(target);
inheritedReceiver[0] = directValue;
inheritedReceiver["1.1"] = directValue;"#,
    )
    .expect("typed array set script should emit");
    expect_valid_module(&artifact, 0);
}

#[test]
fn proxy_revoked_cross_realm_call_module_validates() {
    let artifact = emit_script(
        r#"var other = __scriptwCreateRealm();
var OProxy = other.global.Proxy;
var proxyObj = OProxy.revocable(function() {}, {});
var proxy = proxyObj.proxy;
proxyObj.revoke();

var caught = false;
try {
  proxy();
} catch (error) {
  caught = true;
  // ValidateNonRevokedProxy throws in the running (caller's) Realm.
  if (Object.getPrototypeOf(error) !== TypeError.prototype) {
    throw "revoked proxy wrong realm";
  }
}

if (!caught) throw "revoked proxy missing TypeError";"#,
    )
    .expect("cross-realm revoked proxy script should emit");
    expect_valid_module(&artifact, 1);
}

#[test]
fn supports_object_returning_to_primitive_hook_fallback() {
    let artifact = emit_script("let o = { valueOf() { return {}; } }; o + 1;")
        .expect("object-returning valueOf should fall back to toString");
    expect_valid_module(&artifact, 1);
}

#[test]
fn supports_coercive_compound_assignment() {
    let artifact = emit_script("let s = \"a\"; s += \"b\";").expect("emit should work");
    expect_valid_module(&artifact, 0);
}
#[test]
fn dense_literals_avoid_sparse_bookkeeping_and_sparse_writes_share_one_body() {
    fn literal_artifact(count: usize) -> WasmArtifact {
        let elements = (0..count)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",");
        emit_script(&format!(
            "function make() {{ return [{elements}]; }} make();"
        ))
        .expect("ordinary array literal should emit")
    }

    fn largest_make_body(artifact: &WasmArtifact) -> u32 {
        artifact
            .function_sizes
            .iter()
            .filter(|body| body.name.starts_with("js::make#"))
            .map(|body| body.body_bytes.bytes())
            .max()
            .expect("array producer must be emitted")
    }

    let small = literal_artifact(128);
    let large = literal_artifact(1024);
    expect_valid_module(&large, 1);
    let added_bytes = largest_make_body(&large) - largest_make_body(&small);
    // Each element owns evaluation and fixed stores, not another copy of
    // the presence-list search, allocation and six-field copy loops.
    assert!(
        added_bytes <= (1024 - 128) * 256,
        "896 literal elements added {added_bytes} bytes"
    );
}
