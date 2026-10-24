use scriptw_aot_wasm::emit;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use std::collections::BTreeMap;
use wasmparser::{ExternalKind, KnownCustom, Name, Operator, Parser, Payload, TypeRef};

fn bitwise_body_bytes(expression: String, repetitions: usize) -> u32 {
    std::thread::Builder::new()
        .name(format!("numeric-bitwise-{repetitions}"))
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let statements = format!("{expression};\n").repeat(repetitions);
            let source = format!("function shifts() {{ {statements} return 1; }} shifts();");
            let parsed = parse(&source, ParseOptions::script()).expect("bitwise fixture parses");
            let artifact = emit(&lower(&parsed)).expect("bitwise fixture emits real Wasm");
            artifact
                .function_sizes
                .iter()
                .filter(|body| body.name.starts_with("js::shifts#"))
                .map(|body| body.body_bytes.bytes())
                .max()
                .expect("the shifts function must be emitted")
        })
        .expect("compiler worker starts")
        .join()
        .expect("bitwise emission does not panic")
}

#[test]
fn static_number_bitwise_sites_have_bounded_incremental_emission() {
    for operator in ["<<", ">>", ">>>", "&", "|", "^"] {
        let single = bitwise_body_bytes(format!("2147483649 {operator} 33"), 1);
        let repeated = bitwise_body_bytes(format!("2147483649 {operator} 33"), 65);
        let growth = repeated
            .checked_sub(single)
            .expect("additional Number operations must remain in the emitted body");
        assert!(
            growth > 0,
            "{operator}: the repeated operations disappeared"
        );
        assert!(
            growth < 64 * 512,
            "{operator}: 64 Number operations added {growth} bytes ({single} -> {repeated}); \
             their site cost must not include generic conversion and BigInt dispatch"
        );
    }
}

#[test]
fn unary_and_nested_number_sites_do_not_restore_generic_numeric_dispatch() {
    for expression in [
        "-2147483649",
        "~2147483649",
        "-2147483649 << -1",
        "~2147483649 >> 33",
        "(2147483649 << 1) | ~7",
        "(-1 >>> 1) ^ 3",
    ] {
        let single = bitwise_body_bytes(expression.to_string(), 1);
        let repeated = bitwise_body_bytes(expression.to_string(), 65);
        let growth = repeated
            .checked_sub(single)
            .expect("additional operations remain emitted");
        assert!(growth > 0 && growth < 64 * 768,
            "{expression}: 64 static Number expressions added {growth} bytes ({single} -> {repeated})");
    }
}

struct NumericBody {
    bytes: usize,
    calls: Vec<u32>,
}

#[derive(Default)]
struct NumericModule {
    imports: BTreeMap<u32, (String, String)>,
    exports: BTreeMap<String, u32>,
    names: BTreeMap<u32, String>,
    bodies: BTreeMap<u32, NumericBody>,
}

fn numeric_module(bytes: &[u8], collect_calls: bool) -> NumericModule {
    let mut module = NumericModule::default();
    let mut next_function = 0u32;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("numeric module decodes") {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.expect("numeric import decodes");
                    if matches!(import.ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) {
                        module.imports.insert(
                            next_function,
                            (import.module.to_owned(), import.name.to_owned()),
                        );
                        next_function += 1;
                    }
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.expect("numeric export decodes");
                    if export.kind == ExternalKind::Func {
                        module.exports.insert(export.name.to_owned(), export.index);
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let mut calls = Vec::new();
                if collect_calls {
                    for operator in body.get_operators_reader().expect("numeric body opens") {
                        if let Operator::Call { function_index }
                        | Operator::ReturnCall { function_index } =
                            operator.expect("numeric operator decodes")
                        {
                            calls.push(function_index);
                        }
                    }
                }
                module.bodies.insert(
                    next_function,
                    NumericBody {
                        bytes: body.range().len(),
                        calls,
                    },
                );
                next_function += 1;
            }
            Payload::CustomSection(section) => {
                if let KnownCustom::Name(subsections) = section.as_known() {
                    for subsection in subsections {
                        if let Name::Function(map) = subsection.expect("name subsection decodes") {
                            for naming in map {
                                let naming = naming.expect("numeric function name decodes");
                                assert!(module
                                    .names
                                    .insert(naming.index, naming.name.to_owned())
                                    .is_none());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    module
}

fn source_body<'a>(module: &'a NumericModule, prefix: &str) -> &'a NumericBody {
    let indices = module
        .names
        .iter()
        .filter_map(|(&index, name)| name.starts_with(prefix).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(indices.len(), 1, "one source body must match {prefix}");
    module
        .bodies
        .get(&indices[0])
        .expect("source name identifies actual encoded body")
}

fn runtime_helper<'a>(module: &'a NumericModule, name: &str) -> (u32, &'a NumericBody) {
    let indices = module
        .names
        .iter()
        .filter_map(|(&index, actual)| (actual == name).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(indices.len(), 1, "{name} has one runtime definition");
    let index = indices[0];
    let body = module
        .bodies
        .get(&index)
        .expect("runtime helper name identifies actual encoded code");
    assert!(
        body.bytes > 64,
        "the shared {name} body contains its algorithm"
    );
    (index, body)
}

#[test]
fn literal_string_additions_emit_pooled_values_without_runtime_concatenation_calls() {
    let source = r#"
function literal() { return ('left' + 'right') + ('\ud83d' + '\ude00'); }
function dynamic(left, right) { return left + right; }
[literal, dynamic];
"#;
    let parsed = parse(source, ParseOptions::script()).expect("addition fixture parses");
    let artifact = emit(&lower(&parsed)).expect("addition fixture emits real Wasm");
    let program = numeric_module(&artifact.bytes, true);
    let runtime = numeric_module(
        artifact.runtime().expect("string additions link R").bytes(),
        true,
    );
    let (add_index, _) = runtime_helper(&runtime, "helper::coercive_add");
    let add_export = runtime
        .exports
        .iter()
        .find_map(|(name, &index)| (index == add_index).then_some(name))
        .expect("R exports the actual addition helper");
    let add_imports = program
        .imports
        .iter()
        .filter_map(|(&index, (namespace, name))| {
            (namespace == scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE && name == add_export)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(add_imports.len(), 1);
    assert!(
        !source_body(&program, "js::literal#")
            .calls
            .contains(&add_imports[0]),
        "the literal result must come from the pool without a concatenation allocation"
    );
    assert_eq!(
        source_body(&program, "js::dynamic#")
            .calls
            .iter()
            .filter(|&&index| index == add_imports[0])
            .count(),
        1,
        "dynamic operands must retain the complete runtime operation"
    );
}

#[test]
fn coercive_add_sites_reuse_linked_conversion_bodies_with_bounded_growth() {
    const REPEATED_SITES: usize = 33;
    std::thread::Builder::new()
        .name("coercive-add-emission".to_owned())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // Publish the functions without supplying a direct-call context.
            // Literal call arguments can specialize away the conversions, and
            // distinct observed contexts can clone the source bodies.
            let source = format!(
                "function single(left, right) {{ left + right; }}\n\
                 function repeated(left, right) {{ {} }}\n\
                 [single, repeated];",
                "left + right;\n".repeat(REPEATED_SITES)
            );
            let parsed = parse(&source, ParseOptions::script()).expect("addition fixture parses");
            let lowered = lower(&parsed);
            let script = lowered.script.as_ref().expect("addition fixture owns script IR");
            for name in ["single", "repeated"] {
                let functions = script.functions.iter()
                    .filter(|function| function.name == name).collect::<Vec<_>>();
                assert_eq!(functions.len(), 1, "one unspecialized {name} body");
                assert!(functions[0].params.iter().all(|param| param.kind == scriptw_ir::ValueKind::Dynamic),
                    "unobserved parameters must keep {name}'s additions coercive");
            }
            let artifact = emit(&lowered).expect("addition fixture emits real Wasm");
            let program = numeric_module(&artifact.bytes, true);
            let runtime = numeric_module(
                artifact.runtime().expect("coercive addition links its runtime").bytes(),
                true,
            );
            let single = source_body(&program, "js::single#");
            let repeated = source_body(&program, "js::repeated#");
            let growth = repeated.bytes.checked_sub(single.bytes)
                .expect("additional dynamic additions remain in the encoded body");
            eprintln!("coercive-add body: {} -> {} bytes; {} added sites", single.bytes, repeated.bytes, REPEATED_SITES - 1);
            let (add_index, add_body) = runtime_helper(&runtime, "helper::coercive_add");
            let add_exports = runtime.exports.iter()
                .filter_map(|(name, &index)| (index == add_index).then_some(name))
                .collect::<Vec<_>>();
            assert_eq!(add_exports.len(), 1, "R exports its actual addition body once");
            let add_imports = program.imports.iter()
                .filter_map(|(&index, (namespace, name))| {
                    (namespace == scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE && name == add_exports[0])
                        .then_some(index)
                })
                .collect::<Vec<_>>();
            assert_eq!(add_imports.len(), 1, "P imports the actual addition export once");
            for (body, sites) in [(single, 1), (repeated, REPEATED_SITES)] {
                assert_eq!(body.calls.iter().filter(|&&index| index == add_imports[0]).count(), sites,
                    "each coercive addition must call R's complete operation once");
            }
            assert!(!add_body.calls.contains(&add_index),
                "the addition helper must call its physical body, not its own facade");
            for helper in ["helper::value_to_string", "helper::value_to_number"] {
                let (helper_index, _) = runtime_helper(&runtime, helper);
                assert_eq!(add_body.calls.iter().filter(|&&index| index == helper_index).count(), 2,
                    "R's addition body converts both primitives through the actual {helper} body");
                let export = runtime.exports.iter()
                    .find_map(|(name, &index)| (index == helper_index).then_some(name))
                    .expect("the actual runtime helper is exported");
                let imports = program.imports.iter()
                    .filter_map(|(&index, (namespace, name))| {
                        (namespace == scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE && name == export)
                            .then_some(index)
                    })
                    .collect::<Vec<_>>();
                for body in [single, repeated] {
                    assert!(!body.calls.iter().any(|index| imports.contains(index)),
                        "P's addition sites must leave primitive {helper} conversion inside R");
                }
            }
            for (helper, calls) in [
                ("helper::value_to_primitive_default", 2),
                ("helper::bigint_arithmetic", 1),
            ] {
                let (index, _) = runtime_helper(&runtime, helper);
                assert_eq!(add_body.calls.iter().filter(|&&target| target == index).count(), calls,
                    "R's addition body must retain the actual {helper} operation");
            }
            // The complete helper measures 1,257 bytes per added site, including
            // two ordinary parameter reads and whole-Completion handling. This
            // bound leaves room for that plumbing while rejecting the previous
            // 2,324-byte site that inlined coercion and numeric/string dispatch.
            assert!(growth > 0 && growth < (REPEATED_SITES - 1) * 1536,
                "{} dynamic additions added {growth} bytes; each site must reuse the complete addition body",
                REPEATED_SITES - 1);
        })
        .expect("compiler worker starts")
        .join()
        .expect("addition emission does not panic");
}

#[test]
fn relational_sites_share_one_complete_runtime_comparison_with_bounded_growth() {
    const SITES: usize = 33;
    std::thread::Builder::new()
        .name("relational-comparison-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let source = format!(
                "function single(left, right) {{ left < right; }}\n\
                 function repeated(left, right) {{ {} }}\n[single, repeated];",
                [
                    "left < right;",
                    "left <= right;",
                    "left > right;",
                    "left >= right;"
                ]
                .into_iter()
                .cycle()
                .take(SITES)
                .collect::<Vec<_>>()
                .join("\n")
            );
            let parsed = parse(&source, ParseOptions::script()).expect("relational source parses");
            let artifact = emit(&lower(&parsed)).expect("relational source emits linked Wasm");
            let program = numeric_module(&artifact.bytes, true);
            let runtime =
                numeric_module(artifact.runtime().expect("comparison owns R").bytes(), true);
            let single = source_body(&program, "js::single#");
            let repeated = source_body(&program, "js::repeated#");
            let (index, body) = runtime_helper(&runtime, "helper::relational_comparison");
            let exports = runtime
                .exports
                .iter()
                .filter_map(|(name, &actual)| (actual == index).then_some(name))
                .collect::<Vec<_>>();
            assert_eq!(
                exports.len(),
                1,
                "R exports the complete physical comparison once"
            );
            let imports = program
                .imports
                .iter()
                .filter_map(|(&index, (namespace, name))| {
                    (namespace == scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE && name == exports[0])
                        .then_some(index)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                imports.len(),
                1,
                "P imports the comparison's actual R export once"
            );
            for (caller, sites) in [(single, 1), (repeated, SITES)] {
                assert_eq!(
                    caller
                        .calls
                        .iter()
                        .filter(|&&target| target == imports[0])
                        .count(),
                    sites,
                    "each source comparison calls the complete R algorithm once"
                );
            }
            assert!(
                !body.calls.contains(&index),
                "R must not call its own source facade"
            );
            let (primitive, _) = runtime_helper(&runtime, "helper::value_to_primitive_number");
            assert_eq!(
                body.calls
                    .iter()
                    .filter(|&&target| target == primitive)
                    .count(),
                2,
                "both Number-hint primitive conversions remain in the complete R operation"
            );
            let growth = repeated
                .bytes
                .checked_sub(single.bytes)
                .expect("sites remain encoded");
            eprintln!(
                "relational body: {} -> {} bytes",
                single.bytes, repeated.bytes
            );
            assert!(
                growth > 0 && growth < (SITES - 1) * 1024,
                "{growth} bytes for {} added sites must exclude inline generic comparison",
                SITES - 1
            );
        })
        .expect("emission worker starts")
        .join()
        .expect("relational emission does not panic");
}

#[test]
fn lexical_read_tdz_branches_share_one_runtime_error_without_erasing_reads() {
    const READS: usize = 33;
    std::thread::Builder::new()
        .name("lexical-read-error-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let source = format!(
                "function single(value) {{ value; }}\n\
                 function repeated(value) {{ {} }}\n[single, repeated];",
                "value;\n".repeat(READS)
            );
            let parsed =
                parse(&source, ParseOptions::script()).expect("lexical read source parses");
            let artifact = emit(&lower(&parsed)).expect("lexical reads emit linked Wasm");
            let program = numeric_module(&artifact.bytes, true);
            let runtime =
                numeric_module(artifact.runtime().expect("TDZ branch owns R").bytes(), true);
            let single = source_body(&program, "js::single#");
            let repeated = source_body(&program, "js::repeated#");
            let (index, body) = runtime_helper(&runtime, "helper::lexical_binding_uninitialized");
            let exports = runtime
                .exports
                .iter()
                .filter_map(|(name, &actual)| (actual == index).then_some(name))
                .collect::<Vec<_>>();
            assert_eq!(exports.len(), 1);
            let imports = program
                .imports
                .iter()
                .filter_map(|(&index, (namespace, name))| {
                    (namespace == scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE && name == exports[0])
                        .then_some(index)
                })
                .collect::<Vec<_>>();
            assert_eq!(imports.len(), 1);
            for (caller, reads) in [(single, 1), (repeated, READS)] {
                assert_eq!(
                    caller
                        .calls
                        .iter()
                        .filter(|&&target| target == imports[0])
                        .count(),
                    reads,
                    "each encoded read retains its TDZ error branch"
                );
            }
            assert!(!body.calls.contains(&index));
            let (error, _) = runtime_helper(&runtime, "helper::runtime_error_object");
            assert_eq!(
                body.calls.iter().filter(|&&target| target == error).count(),
                1,
                "the shared error body constructs the actual fresh native error"
            );
            let growth = repeated
                .bytes
                .checked_sub(single.bytes)
                .expect("reads remain encoded");
            eprintln!(
                "lexical read body: {} -> {} bytes",
                single.bytes, repeated.bytes
            );
            assert!(
                growth > 0 && growth < (READS - 1) * 320,
                "{growth} bytes for {} reads must exclude duplicated error construction",
                READS - 1
            );
        })
        .expect("emission worker starts")
        .join()
        .expect("lexical read emission does not panic");
}
