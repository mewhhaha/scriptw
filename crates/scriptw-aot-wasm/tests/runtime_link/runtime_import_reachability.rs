use scriptw_aot_wasm::{emit, GcHostImport, WasmArtifact};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{Parser, Payload, Validator, WasmFeatures};

use crate::linked_bodies;

fn emit_source(source: impl Into<String>) -> WasmArtifact {
    let source = source.into();
    std::thread::Builder::new()
        .name("runtime-import-reachability".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let parsed = parse(&source, ParseOptions::script()).expect("fixture parses");
            let program = lower(&parsed);
            let artifact = emit(&program).expect("fixture emits through ordinary AOT codegen");
            Validator::new_with_features(WasmFeatures::all())
                .validate_all(&artifact.bytes)
                .expect("runtime planning preserves valid Wasm indices");
            artifact
        })
        .expect("compiler worker spawns")
        .join()
        .expect("compiler worker does not panic")
}

fn artifact(source: &'static str) -> (Vec<u8>, String) {
    let artifact = emit_source(source);
    (artifact.bytes, artifact.debug_dump)
}

fn imported_functions(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ImportSection(section) = payload.expect("section decodes") {
            for import in section.into_imports() {
                let import = import.expect("import decodes");
                names.push(format!("{}.{}", import.module, import.name));
            }
        }
    }
    names
}

#[test]
fn runtime_free_ir_does_not_introduce_intl_or_clock_imports() {
    for source in [
        "",
        ";;",
        "262;",
        "1 + 1;",
        "(7 - 2) * 3 / 2;",
        "5 % 2;",
        "17 % (8 % 3);",
        "1.7976931348623157e308 % 3;",
        "1.5e-323 % 1e-323;",
        "-5 % 2;",
        "-4 % 2;",
        "-0 + -0;",
        "-0 - 0;",
        "-0 * 3;",
        "1 / 0;",
        "0 / 0;",
        "void (1 + 2);",
        "+(1 + 2);",
        "-(1 + 2);",
        "~(1 + 2);",
        "(1 + 2) ? (3 * 4) : (5 % 2);",
        "true; null; 'text';",
        "-0;",
        "+7;",
        "~7;",
        "delete 1;",
        "!0;",
        "false || 'text';",
        "null ?? 7;",
        "!(null ?? 7);",
        "+(0 ?? 7);",
        "+(3 ?? 7);",
        "true ? 'yes' : 3;",
        "(1, 2, 3);",
        "{ 7; { 8; } }",
    ] {
        let (bytes, debug_dump) = artifact(source);
        let imports = imported_functions(&bytes);
        for forbidden in [
            "scriptw_host.intl_call",
            "scriptw_host.system_time_zone",
            "scriptw_host.wall_clock_millis",
        ] {
            assert!(
                !imports.iter().any(|name| name == forbidden),
                "{source}: {imports:?}"
            );
        }
        assert!(
            debug_dump.contains("runtime helper functions: 0"),
            "{source}\n{}",
            debug_dump
        );
        assert!(Parser::new(0).parse_all(&bytes).all(|payload| {
            !matches!(payload.expect("section decodes"), Payload::CustomSection(section)
                if section.name() == scriptw_intl::INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION)
        }));
    }
}

#[test]
fn observable_intrinsics_keep_callable_bodies_and_provider_imports() {
    let provider = format!(
        "{}.{}",
        GcHostImport::IntlProviderCall.module(),
        GcHostImport::IntlProviderCall.name()
    );
    for source in [
        "(1)['toLocaleString']('en-US');",
        "(1n)['toLocaleString']('en-US');",
        "Object.getPrototypeOf(1).toLocaleString.call(123, 'en-US');",
        "new Intl.NumberFormat('en-US').format(123);",
    ] {
        let artifact = emit_source(source);
        // Every builtin body is compiled into R, which the program links, and
        // R imports the fixed provider host call. P keeps only host functions
        // its own bodies call, and the provider's callers are all in R.
        let runtime = artifact.runtime().expect("heap program links R");
        assert!(
            imported_functions(runtime.bytes()).contains(&provider),
            "R: {source}"
        );
        assert!(
            !imported_functions(&artifact.bytes).contains(&provider),
            "P: {source}"
        );
        let bodies = linked_bodies::linked_bodies(&artifact);
        let format = bodies.unique("builtin::Intl.NumberFormat Format Function");
        assert!(
            bodies.runtime.iter().any(|body| body.index == format.index),
            "{source}"
        );
    }
}

#[test]
fn unproven_ir_retains_ordinary_realm_bootstrap() {
    for source in [
        // The scalar proof excludes observable or non-Number conversion.
        "1 + '2';",
        "1n + 2n;",
        "1 < 2;",
        "+(null ?? 7);",
        "var answer = 262; answer;",
        "let answer = 262; answer;",
        "function answer() { return 262; } answer();",
        "({valueOf() { return 262; }}) + 1;",
        "this;",
        "try { throw 1; } catch (caught) { caught; }",
    ] {
        let (_, debug_dump) = artifact(source);
        assert!(debug_dump.contains("heap: enabled"), "{source}");
        assert!(
            !debug_dump.contains("runtime helper functions: 0"),
            "{source}"
        );
    }
}

#[test]
fn exponentiation_retains_its_planned_host_dependency() {
    let source = "var base = 2; base ** (1 + 2);";
    let (bytes, debug_dump) = artifact(source);
    assert!(
        debug_dump.contains("heap: enabled"),
        "{source}\n{debug_dump}"
    );
    assert!(
        imported_functions(&bytes)
            .iter()
            .any(|name| name == "scriptw_host.number_pow"),
        "{source} retains the Number exponentiation host import"
    );
}

#[test]
fn runtime_elision_keeps_the_experimental_wasm_gc_contract() {
    let (bytes, _) = artifact("262;");
    let mut features = WasmFeatures::all();
    features.set(WasmFeatures::GC, false);
    assert!(Validator::new_with_features(features)
        .validate_all(&bytes)
        .is_err());
}

#[test]
fn empty_eval_visible_blocks_do_not_grow_emitted_function_bodies() {
    for prefix in ["", "'use strict';"] {
        // A direct-eval site makes analysis retain observable lexical scopes.
        // Structurally empty blocks still have no scope or runtime effects.
        let small = emit_source(format!("{prefix} eval(0); 17; {{}}"));
        let large = emit_source(format!("{prefix} eval(0); 17; {}", "{}".repeat(4096)));
        let sizes = |artifact: &WasmArtifact| {
            artifact
                .function_sizes
                .iter()
                .map(|body| (body.name.clone(), body.body_bytes))
                .collect::<Vec<_>>()
        };
        assert!(small
            .function_sizes
            .iter()
            .any(|body| body.name == "scriptw::main"));
        assert_eq!(sizes(&small), sizes(&large));
    }
}
