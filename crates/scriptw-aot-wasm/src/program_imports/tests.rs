use super::*;
use crate::code_sink::{Function, LocalDeclarations};
use crate::emitted_function::{EmittedFunction, ModuleCode};
use std::borrow::Cow;
use wasm_encoder::{ConstExpr, EntityType, ExportKind, GlobalType, Instruction, ValType};

fn body(ops: &[Instruction<'_>]) -> Function {
    let mut f = Function::new(LocalDeclarations::from_types([]));
    for op in ops {
        f.instruction(op);
    }
    f.instruction(&Instruction::End);
    f
}
fn owned_body(
    imports: u32,
    ops: &[Instruction<'_>],
) -> (wasm_encoder::CodeSection, ModuleFunctionTable) {
    let mut code = ModuleCode::new(imports);
    code.push(EmittedFunction::main(body(ops)));
    let (_, code, table) = code.finish();
    (code, table)
}
fn types() -> wasm_encoder::TypeSection {
    let mut s = wasm_encoder::TypeSection::new();
    s.ty().function([], []);
    s
}
fn functions() -> wasm_encoder::FunctionSection {
    let mut s = wasm_encoder::FunctionSection::new();
    s.function(0);
    s
}
fn global_type() -> GlobalType {
    GlobalType {
        val_type: ValType::I32,
        mutable: false,
        shared: false,
    }
}
fn validate(bytes: &[u8]) {
    wasmparser::Validator::new_with_features(crate::runtime_artifact::program_validation_features())
        .validate_all(bytes).expect("fixture/result must be a valid owned core module");
}
fn import_names(bytes: &[u8]) -> Vec<(String, String)> {
    parser()
        .parse_all(bytes)
        .filter_map(|p| match p.unwrap() {
            Payload::ImportSection(r) => Some(
                r.into_imports()
                    .map(|i| {
                        let i = i.unwrap();
                        (i.module.to_owned(), i.name.to_owned())
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn removes_only_unused_runtime_imports_and_remaps_calls_refs_exports_and_names() {
    let mut imports = wasm_encoder::ImportSection::new();
    for (module, name) in [
        (crate::RUNTIME_IMPORT_NAMESPACE, "dead"),
        ("host", "unused-host"),
        (crate::RUNTIME_IMPORT_NAMESPACE, "live"),
        ("foreign", "unused-foreign"),
    ] {
        imports.import(module, name, EntityType::Function(0));
    }
    imports.import(
        crate::RUNTIME_IMPORT_NAMESPACE,
        "dead-global",
        EntityType::Global(global_type()),
    );
    imports.import(
        crate::RUNTIME_IMPORT_NAMESPACE,
        "live-global",
        EntityType::Global(global_type()),
    );
    let (code, mut table) = owned_body(
        4,
        &[
            Instruction::Call(2),
            Instruction::RefFunc(4),
            Instruction::Drop,
            Instruction::GlobalGet(1),
            Instruction::Drop,
        ],
    );
    let mut exports = wasm_encoder::ExportSection::new();
    exports.export("entry", ExportKind::Func, 4);
    exports.export("state", ExportKind::Global, 1);
    let mut elements = wasm_encoder::ElementSection::new();
    elements.declared(wasm_encoder::Elements::Functions(Cow::Borrowed(&[0, 2, 4])));
    let mut names = wasm_encoder::NameSection::new();
    let mut fs = wasm_encoder::NameMap::new();
    for (i, n) in [(0, "dead"), (2, "live"), (4, "entry")] {
        fs.append(i, n);
    }
    names.functions(&fs);
    let mut gs = wasm_encoder::NameMap::new();
    gs.append(0, "dead-global");
    gs.append(1, "live-global");
    names.globals(&gs);
    for labels in [false, true] {
        let mut outer = wasm_encoder::IndirectNameMap::new();
        let mut inner = wasm_encoder::NameMap::new();
        inner.append(0, "slot");
        outer.append(0, &inner);
        outer.append(4, &inner);
        if labels {
            names.labels(&outer);
        } else {
            names.locals(&outer);
        }
    }
    let mut m = Module::new();
    m.section(&types())
        .section(&imports)
        .section(&functions())
        .section(&exports)
        .section(&elements)
        .section(&code)
        .section(&names);
    let input = m.finish();
    validate(&input);
    let output = compact(input, &mut table).unwrap();
    validate(&output);
    assert_eq!(
        import_names(&output),
        vec![
            ("host".into(), "unused-host".into()),
            (crate::RUNTIME_IMPORT_NAMESPACE.into(), "live".into()),
            ("foreign".into(), "unused-foreign".into()),
            (crate::RUNTIME_IMPORT_NAMESPACE.into(), "live-global".into())
        ]
    );
    let mut names_seen = 0;
    for p in parser().parse_all(&output) {
        match p.unwrap() {
            Payload::ExportSection(r) => {
                assert_eq!(
                    r.into_iter().map(|e| e.unwrap().index).collect::<Vec<_>>(),
                    vec![3, 0]
                )
            }
            Payload::CodeSectionEntry(b) => {
                let ops = b
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect::<Vec<_>>();
                assert!(matches!(
                    ops[0],
                    wasmparser::Operator::Call { function_index: 1 }
                ));
                assert!(matches!(
                    ops[1],
                    wasmparser::Operator::RefFunc { function_index: 3 }
                ));
                assert!(matches!(
                    ops[3],
                    wasmparser::Operator::GlobalGet { global_index: 0 }
                ));
            }
            Payload::ElementSection(r) => {
                let e = r.into_iter().next().unwrap().unwrap();
                let ElementItems::Functions(r) = e.items else {
                    panic!()
                };
                assert_eq!(
                    r.into_iter().map(Result::unwrap).collect::<Vec<_>>(),
                    vec![1, 3]
                );
            }
            Payload::CustomSection(s) if s.name() == "name" => {
                for n in wasmparser::NameSectionReader::new(wasmparser::BinaryReader::new(
                    s.data(),
                    s.data_offset(),
                )) {
                    match n.unwrap() {
                        wasmparser::Name::Function(r) => {
                            assert_eq!(
                                r.into_iter().map(|n| n.unwrap().index).collect::<Vec<_>>(),
                                vec![1, 3]
                            );
                            names_seen += 1;
                        }
                        wasmparser::Name::Global(r) => {
                            assert_eq!(
                                r.into_iter().map(|n| n.unwrap().index).collect::<Vec<_>>(),
                                vec![0]
                            );
                            names_seen += 1;
                        }
                        wasmparser::Name::Local(r) | wasmparser::Name::Label(r) => {
                            assert_eq!(
                                r.into_iter().map(|n| n.unwrap().index).collect::<Vec<_>>(),
                                vec![3]
                            );
                            names_seen += 1;
                        }
                        _ => panic!("unexpected name map"),
                    }
                }
            }
            _ => {}
        }
    }
    assert_eq!(names_seen, 4);
    assert_eq!(table.summaries()[0].wasm_index, 3);
}

#[test]
fn wide_leb_reencoding_remeasures_exact_body_and_preserves_opaque_ordinals() {
    let mut imports = wasm_encoder::ImportSection::new();
    for _ in 0..130 {
        imports.import(
            crate::RUNTIME_IMPORT_NAMESPACE,
            "unused",
            EntityType::Function(0),
        );
    }
    let (code, mut table) = owned_body(
        130,
        &[
            Instruction::I32Const(0),
            Instruction::Drop,
            Instruction::Call(130),
        ],
    );
    let original_size = table.summaries()[0].body_bytes.bytes();
    let context =
        crate::native_program_contexts::section(&[crate::native_program_contexts::ModuleContext {
            body: 0,
            context: crate::native_program_contexts::FunctionContext::Program {
                schema: crate::native_program_contexts::Schema {
                    program_type: 0,
                    payload_type: 0,
                    payload_field: 0,
                },
                local: 0,
                ready: 1,
            },
        }])
        .unwrap();
    // Body/operator ordinals must survive immediate-width changes unchanged.
    let literal = wasm_encoder::CustomSection {
        name: Cow::Borrowed(crate::program_literal_operands::SECTION_NAME),
        data: Cow::Owned(
            [
                1u32.to_le_bytes().as_slice(),
                1u32.to_le_bytes().as_slice(),
                0u32.to_le_bytes().as_slice(),
                0u32.to_le_bytes().as_slice(),
                &[1],
                0i32.to_le_bytes().as_slice(),
            ]
            .concat(),
        ),
    };
    let mut exports = wasm_encoder::ExportSection::new();
    exports.export("entry", ExportKind::Func, 130);
    let mut m = Module::new();
    m.section(&types())
        .section(&imports)
        .section(&functions())
        .section(&exports)
        .section(&code)
        .section(&context)
        .section(&literal);
    let input = m.finish();
    validate(&input);
    let custom = |bytes: &[u8]| {
        parser()
            .parse_all(bytes)
            .filter_map(|p| match p.unwrap() {
                Payload::CustomSection(s) => Some((s.name().to_owned(), s.data().to_vec())),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let before = custom(&input);
    let output = compact(input, &mut table).unwrap();
    validate(&output);
    assert_eq!(custom(&output), before);
    let summary = &table.summaries()[0];
    assert_eq!(summary.wasm_index, 0);
    assert_eq!(summary.body_bytes.bytes() + 1, original_size);
    let encoded = parser()
        .parse_all(&output)
        .find_map(|p| match p.unwrap() {
            Payload::CodeSectionEntry(b) => Some(b.range().len() as u32),
            _ => None,
        })
        .unwrap();
    assert_eq!(summary.body_bytes.bytes(), encoded);
}

#[test]
fn unknown_custom_section_is_rejected_even_without_removable_imports() {
    let (code, mut table) = owned_body(0, &[]);
    let mut m = Module::new();
    m.section(&types())
        .section(&functions())
        .section(&code)
        .section(&wasm_encoder::CustomSection {
            name: Cow::Borrowed("unreviewed.indices"),
            data: Cow::Borrowed(&[0]),
        });
    let error = compact(m.finish(), &mut table).unwrap_err();
    assert!(error
        .to_string()
        .contains("unreviewed compiler P custom section"));
}

#[test]
fn initializer_and_active_passive_segment_references_are_live_and_segment_indices_stay_fixed() {
    let mut imports = wasm_encoder::ImportSection::new();
    for name in ["dead", "table-init", "global-init", "passive", "active"] {
        imports.import(
            crate::RUNTIME_IMPORT_NAMESPACE,
            name,
            EntityType::Function(0),
        );
    }
    for name in ["dead-global", "element-offset", "data-offset"] {
        imports.import(
            crate::RUNTIME_IMPORT_NAMESPACE,
            name,
            EntityType::Global(global_type()),
        );
    }
    let (code, mut table) = owned_body(5, &[]);
    let mut tables = wasm_encoder::TableSection::new();
    tables.table_with_init(
        wasm_encoder::TableType {
            element_type: wasm_encoder::RefType::FUNCREF,
            table64: false,
            minimum: 1,
            maximum: None,
            shared: false,
        },
        &ConstExpr::ref_func(1),
    );
    let mut memories = wasm_encoder::MemorySection::new();
    memories.memory(wasm_encoder::MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    let mut globals = wasm_encoder::GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: ValType::Ref(wasm_encoder::RefType::FUNCREF),
            mutable: false,
            shared: false,
        },
        &ConstExpr::ref_func(2),
    );
    let mut elements = wasm_encoder::ElementSection::new();
    elements.active(
        Some(0),
        &ConstExpr::global_get(1),
        wasm_encoder::Elements::Functions(Cow::Borrowed(&[4])),
    );
    elements.passive(wasm_encoder::Elements::Functions(Cow::Borrowed(&[3])));
    elements.declared(wasm_encoder::Elements::Functions(Cow::Borrowed(&[0, 1, 2])));
    elements.declared(wasm_encoder::Elements::Functions(Cow::Borrowed(&[0])));
    let mut data = wasm_encoder::DataSection::new();
    data.active(0, &ConstExpr::global_get(2), [42]);
    let mut m = Module::new();
    m.section(&types())
        .section(&imports)
        .section(&functions())
        .section(&tables)
        .section(&memories)
        .section(&globals)
        .section(&elements)
        .section(&code)
        .section(&data);
    let input = m.finish();
    validate(&input);
    let output = compact(input, &mut table).unwrap();
    validate(&output);
    assert_eq!(
        import_names(&output)
            .into_iter()
            .map(|(_, n)| n)
            .collect::<Vec<_>>(),
        vec![
            "table-init",
            "global-init",
            "passive",
            "active",
            "element-offset",
            "data-offset"
        ]
    );
    let mut segment_lists = Vec::new();
    let mut offsets = Vec::new();
    let mut init_refs = Vec::new();
    for p in parser().parse_all(&output) {
        match p.unwrap() {
            Payload::TableSection(r) => {
                for t in r {
                    if let wasmparser::TableInit::Expr(e) = t.unwrap().init {
                        let mut ops = e.get_operators_reader();
                        if let wasmparser::Operator::RefFunc { function_index } =
                            ops.read().unwrap()
                        {
                            init_refs.push(function_index);
                        } else {
                            panic!()
                        }
                    }
                }
            }
            Payload::GlobalSection(r) => {
                for g in r {
                    let g = g.unwrap();
                    let mut ops = g.init_expr.get_operators_reader();
                    if let wasmparser::Operator::RefFunc { function_index } = ops.read().unwrap() {
                        init_refs.push(function_index);
                    } else {
                        panic!()
                    }
                }
            }
            Payload::ElementSection(r) => {
                for e in r {
                    let e = e.unwrap();
                    if let ElementKind::Active { offset_expr, .. } = e.kind {
                        let mut ops = offset_expr.get_operators_reader();
                        if let wasmparser::Operator::GlobalGet { global_index } =
                            ops.read().unwrap()
                        {
                            offsets.push(global_index);
                        } else {
                            panic!()
                        }
                    }
                    let ElementItems::Functions(f) = e.items else {
                        panic!()
                    };
                    segment_lists.push(f.into_iter().map(Result::unwrap).collect::<Vec<_>>());
                }
            }
            Payload::DataSection(r) => {
                for d in r {
                    let d = d.unwrap();
                    assert_eq!(d.data, [42]);
                    let wasmparser::DataKind::Active { offset_expr, .. } = d.kind else {
                        panic!()
                    };
                    let mut ops = offset_expr.get_operators_reader();
                    if let wasmparser::Operator::GlobalGet { global_index } = ops.read().unwrap() {
                        offsets.push(global_index);
                    } else {
                        panic!()
                    }
                }
            }
            _ => {}
        }
    }
    assert_eq!(init_refs, vec![0, 1]);
    assert_eq!(offsets, vec![0, 1]);
    assert_eq!(segment_lists, vec![vec![3], vec![2], vec![0, 1], vec![]]);
    assert_eq!(table.summaries()[0].wasm_index, 4);
}

#[test]
fn exact_function_import_retains_its_kind_or_reports_feature_validation_failure() {
    let mut imports = wasm_encoder::ImportSection::new();
    imports.import("host", "exact", EntityType::FunctionExact(0));
    imports.import(
        crate::RUNTIME_IMPORT_NAMESPACE,
        "dead",
        EntityType::Function(0),
    );
    let (code, mut table) = owned_body(2, &[Instruction::Call(0)]);
    let mut m = Module::new();
    m.section(&types())
        .section(&imports)
        .section(&functions())
        .section(&code);
    let input = m.finish();
    let admission = wasmparser::Validator::new_with_features(
        crate::runtime_artifact::program_validation_features(),
    )
    .validate_all(&input);
    match admission {
        Ok(_) => {
            let output = compact(input, &mut table).unwrap();
            validate(&output);
            assert_eq!(table.summaries()[0].wasm_index, 1);
            let mut count = 0;
            for p in parser().parse_all(&output) {
                if let Payload::ImportSection(r) = p.unwrap() {
                    for i in r.into_imports() {
                        assert!(matches!(i.unwrap().ty, TypeRef::FuncExact(0)));
                        count += 1;
                    }
                }
            }
            assert_eq!(count, 1);
        }
        Err(error) => {
            let actual = compact(input, &mut table).unwrap_err().to_string();
            assert!(
                actual.contains("compiler program import compaction failed"),
                "{actual}"
            );
            assert!(
                actual.contains(&error.message()),
                "expected precise validation reason {error}, got {actual}"
            );
        }
    }
}
