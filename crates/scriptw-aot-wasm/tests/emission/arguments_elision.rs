//! Observe actual Arguments allocation and the unchanged ordinary entry bound.
use crate::{linked_bodies::module_bodies, property_read_plans::Module};
use scriptw_aot_wasm::{emit, WasmRuntimeValueTag};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{
    HeapType, Operator, Parser, Payload, TypeRef, UnpackedIndex, ValType, Validator, WasmFeatures,
};

fn local_types(bytes: &[u8], index: u32, parameters: &[ValType]) -> Vec<ValType> {
    let mut next = 0;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("P decodes") {
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    if matches!(import.unwrap().ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) {
                        next += 1;
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                if next == index {
                    let mut types = parameters.to_vec();
                    for local in body.get_locals_reader().unwrap() {
                        let (count, ty) = local.unwrap();
                        types.extend(std::iter::repeat_n(ty, count as usize));
                    }
                    return types;
                }
                next += 1;
            }
            _ => {}
        }
    }
    panic!("the source body has encoded local declarations");
}

fn arguments_type(body: &[Operator<'_>], locals: &[ValType]) -> u32 {
    // The real observed control publishes an Arguments Value from its typed
    // allocation root. Derive P's type there; no snapshot exports or R/P type
    // index equality is assumed.
    let types = body.windows(7).filter_map(|ops| {
        let [Operator::I32Const { value }, Operator::LocalSet { .. },
            Operator::I64Const { value: 0 }, Operator::LocalSet { .. },
            Operator::LocalGet { local_index }, Operator::RefAsNonNull,
            Operator::LocalSet { .. }] = ops else { return None; };
        if *value != WasmRuntimeValueTag::Arguments as i32 {
            return None;
        }
        let ValType::Ref(reference) = locals[*local_index as usize] else {
            panic!("Arguments publication reads a GC root");
        };
        let HeapType::Concrete(UnpackedIndex::Module(index)) = reference.heap_type() else {
            panic!("Arguments publication retains a concrete allocation type");
        };
        Some(index)
    }).collect::<Vec<_>>();
    assert_eq!(
        types.len(),
        1,
        "one typed Arguments publication in the control"
    );
    types[0]
}

fn allocations(body: &[Operator<'_>], ty: u32) -> usize {
    body.iter()
        .filter(|op| {
            matches!(op,
                Operator::StructNew { struct_type_index }
                | Operator::StructNewDefault { struct_type_index } if *struct_type_index == ty
            )
        })
        .count()
}

#[test]
fn complete_ordinary_walks_omit_arguments_without_widening_five_formal_entries() {
    let (artifact, functions) = std::thread::Builder::new()
        .name("arguments-elision-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(
                r#"
                export function scan(a, b, c, d, e) {
                    var value = e;
                    try {
                        b[d] = e;
                        ++b.count;
                        value = a(b[d]);
                    } catch (error) {
                        value = error === e;
                    }
                    for (var key in c) b[key] = c[key] == value;
                    return value;
                }
                export function classify(a, b, c, d, e) {
                    b.model = e;
                    if (e === null) return 0;
                    if (e == c) return b[d]++;
                    return a(e);
                }
                export function observed(a, b, c, d, e) { return arguments[0]; }
                export function captured(a, b, c, d, e) { return () => arguments[4]; }
                export function small(row, key) { row.value = key; return row.value == key; }
                export function smallObserved(row, key) { row.value = key; return arguments[0]; }
            "#,
                ParseOptions::module(),
            )
            .unwrap();
            let program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let functions = program
                .script
                .as_ref()
                .unwrap()
                .functions
                .iter()
                .filter(|function| {
                    [
                        "scan",
                        "classify",
                        "observed",
                        "captured",
                        "small",
                        "smallObserved",
                    ]
                    .contains(&function.name.as_str())
                })
                .map(|function| (function.name.clone(), function.id.clone()))
                .collect::<Vec<_>>();
            (
                emit(&program).expect("ordinary observer walks emit"),
                functions,
            )
        })
        .unwrap()
        .join()
        .unwrap();
    for bytes in [
        artifact.runtime().unwrap().bytes(),
        artifact.bytes.as_slice(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .expect("actual R/P validate");
    }
    let program = Module::read(&artifact.bytes, true);
    let observed = functions
        .iter()
        .find(|(name, _)| name == "observed")
        .unwrap();
    let (observed_index, observed_body) = program.named(&format!("js::observed#{}", observed.1));
    let vector_signature = program.function_type(observed_index);
    assert_eq!(vector_signature.params().len(), 11, "ordinary vector ABI");
    let locals = local_types(&artifact.bytes, observed_index, vector_signature.params());
    let arguments = arguments_type(observed_body, &locals);
    assert_eq!(
        allocations(observed_body, arguments),
        1,
        "the observed control allocates its actual Arguments type"
    );
    let bodies = module_bodies(&artifact.bytes);
    for name in [
        "scan",
        "classify",
        "observed",
        "captured",
        "small",
        "smallObserved",
    ] {
        let selected = functions
            .iter()
            .filter(|(actual, _)| actual == name)
            .collect::<Vec<_>>();
        assert!(!selected.is_empty(), "the source {name} was lowered");
        for (_, id) in selected {
            let (index, body) = program.named(&format!("js::{name}#{id}"));
            let expected = usize::from(matches!(name, "observed" | "captured" | "smallObserved"));
            assert_eq!(
                allocations(body, arguments),
                expected,
                "{name} arguments materialization"
            );
            let signature = program.function_type(index);
            if name == "small" {
                assert_eq!(
                    signature.params().len(),
                    15,
                    "the same observer proof also admits the existing two-formal local entry"
                );
                assert_eq!(signature.results(), vector_signature.results());
                let adapter_name = format!("js-arguments-adapter::{name}#{id}");
                let (adapter, code) = program.named(&adapter_name);
                assert_eq!(program.function_type(adapter), vector_signature);
                assert!(code.iter().any(|op| matches!(op, Operator::ReturnCall { function_index } if *function_index == index)), "the vector adapter tail-calls the one source body");
            } else {
                assert_eq!(
                    signature, vector_signature,
                    "{name} keeps its ordinary entry"
                );
                assert!(
                    !bodies
                        .iter()
                        .any(|body| body.name == format!("js-arguments-adapter::{name}#{id}")),
                    "five-formal and observing functions do not gain a local-entry adapter"
                );
            }
        }
    }
}
