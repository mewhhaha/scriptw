#![cfg(test)]
use super::*;
use std::borrow::Cow;
use wasm_encoder::{
    CodeSection, ConstExpr, DataCountSection, DataSection, ElementSection, Elements, EntityType,
    ExportKind, ExportSection, Function, FunctionSection, GlobalSection, GlobalType, HeapType,
    ImportSection, Instruction, Module, RefType, StorageType, TableSection, TableType, TypeSection,
    ValType,
};
fn image(value: u8, inline: bool) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    types.ty().array(&StorageType::I8, true);
    types.ty().function(
        [],
        [ValType::Ref(RefType {
            nullable: false,
            heap_type: HeapType::Concrete(1),
        })],
    );
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import("R", "unused", EntityType::Function(0));
    imports.import("R", "used", EntityType::Function(0));
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(2);
    module.section(&functions);
    let mut exports = ExportSection::new();
    exports.export("fresh", ExportKind::Func, 2);
    module.section(&exports);
    let mut elements = ElementSection::new();
    elements.declared(Elements::Functions(Cow::Borrowed(&[0, 1, 2])));
    module.section(&elements);
    if !inline {
        module.section(&DataCountSection { count: 1 });
    }
    let mut body = Function::new([]);
    body.instruction(&Instruction::Call(1));
    if inline {
        body.instruction(&Instruction::I32Const(i32::from(value)));
        body.instruction(&Instruction::ArrayNewFixed {
            array_type_index: 1,
            array_size: 1,
        });
    } else {
        body.instruction(&Instruction::I32Const(0));
        body.instruction(&Instruction::I32Const(1));
        body.instruction(&Instruction::ArrayNewData {
            array_type_index: 1,
            array_data_index: 0,
        });
    }
    body.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&body);
    module.section(&code);
    if !inline {
        let mut data = DataSection::new();
        data.passive([value]);
        module.section(&data);
    }
    module.finish()
}
#[test]
fn passive_payloads_keep_distinct_images_and_share_actual_code() {
    let first = image(7, false);
    let second = image(9, false);
    let a = plan::Plan::read(&first).unwrap().unwrap();
    let b = plan::Plan::read(&second).unwrap().unwrap();
    assert_eq!(
        reencode::encode(&first, &a).unwrap(),
        reencode::encode(&second, &b).unwrap()
    );
    assert_eq!(a.images[0].bytes(), [7]);
    assert_eq!(b.images[0].bytes(), [9]);
    assert_eq!(a.retained_functions(), 1);
    assert!(!a.function_map.contains_key(&0));
    assert_eq!(a.function(1), Some(0));
    assert_eq!(
        a.factories[0].construction(),
        NativeProgramArrayConstruction::Whole
    );
}
#[test]
fn packed_constant_images_use_a_fresh_constructor_at_every_original_site() {
    let first = image(17, true);
    let second = image(19, true);
    let a = plan::Plan::read(&first).unwrap().unwrap();
    let b = plan::Plan::read(&second).unwrap().unwrap();
    assert_eq!(
        reencode::encode(&first, &a).unwrap(),
        reencode::encode(&second, &b).unwrap()
    );
    assert_ne!(a.images[0].bytes(), b.images[0].bytes());
    assert_eq!(a.factories.len(), 1);
    let encoded = reencode::encode(&first, &a).unwrap();
    let mut calls = Vec::new();
    let mut allocations = 0;
    for payload in wasmparser::Parser::new(0).parse_all(&encoded) {
        if let wasmparser::Payload::CodeSectionEntry(body) = payload.unwrap() {
            let mut reader = body.get_operators_reader().unwrap();
            while !reader.eof() {
                match reader.read().unwrap() {
                    wasmparser::Operator::Call { function_index } => calls.push(function_index),
                    wasmparser::Operator::ArrayNewFixed { .. } => allocations += 1,
                    _ => {}
                }
            }
        }
    }
    assert_eq!(calls, [0, 1]);
    assert_eq!(allocations, 0); // Call(1) constructs, it never returns an imported result array.
}
#[test]
fn transformed_indices_and_gc_signatures_validate_with_source_data_removed() {
    for inline in [false, true] {
        let bytes = image(255, inline);
        let plan = plan::Plan::read(&bytes).unwrap().unwrap();
        let transformed = reencode::encode(&bytes, &plan).unwrap();
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
            .validate_all(&transformed)
            .unwrap();
        let mut imports = Vec::new();
        for payload in wasmparser::Parser::new(0).parse_all(&transformed) {
            match payload.unwrap() {
                wasmparser::Payload::ImportSection(reader) => {
                    for import in reader.into_imports() {
                        let import = import.unwrap();
                        imports.push((import.module.to_string(), import.name.to_string()));
                    }
                }
                wasmparser::Payload::DataSection(reader) => assert_eq!(reader.count(), 0),
                _ => {}
            }
        }
        assert!(imports.contains(&("R".into(), "used".into())));
        assert!(!imports.contains(&("R".into(), "unused".into())));
    }
}
#[test]
fn malformed_source_and_start_sections_are_not_minted_as_templates() {
    assert!(plan::Plan::read(b"bad wasm").is_err());
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import("R", "start", EntityType::Function(0));
    module.section(&imports);
    module.section(&wasm_encoder::StartSection { function_index: 0 });
    assert!(plan::Plan::read(&module.finish()).is_err());
}
#[test]
fn function_and_indirect_names_follow_the_same_pruned_index_map() {
    let mut bytes = image(7, false);
    let mut names = wasm_encoder::NameSection::new();
    let mut functions = wasm_encoder::NameMap::new();
    functions.append(0, "removed");
    functions.append(1, "retained");
    functions.append(2, "defined");
    names.functions(&functions);
    let mut local = wasm_encoder::NameMap::new();
    local.append(0, "operand");
    let mut indirect = wasm_encoder::IndirectNameMap::new();
    indirect.append(0, &local);
    indirect.append(1, &local);
    names.locals(&indirect);
    bytes.push(0);
    wasm_encoder::Encode::encode(&names, &mut bytes);
    let plan = plan::Plan::read(&bytes).unwrap().unwrap();
    let encoded = reencode::encode(&bytes, &plan).unwrap();
    let mut function_names = Vec::new();
    let mut local_owners = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(&encoded) {
        if let wasmparser::Payload::CustomSection(section) = payload.unwrap() {
            if let wasmparser::KnownCustom::Name(reader) = section.as_known() {
                for name in reader {
                    match name.unwrap() {
                        wasmparser::Name::Function(reader) => {
                            for row in reader {
                                let row = row.unwrap();
                                function_names.push((row.index, row.name.to_owned()));
                            }
                        }
                        wasmparser::Name::Local(reader) => {
                            for row in reader {
                                local_owners.push(row.unwrap().index);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    assert_eq!(
        function_names,
        [(0, "retained".to_owned()), (2, "defined".to_owned())]
    );
    assert_eq!(local_owners, [0]);
}
#[test]
fn other_data_operators_keep_the_self_contained_native_path() {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import("R", "unused", EntityType::Function(0));
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    module.section(&DataCountSection { count: 1 });
    let mut function = Function::new([]);
    function.instruction(&Instruction::DataDrop(0));
    function.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&function);
    module.section(&code);
    let mut data = DataSection::new();
    data.passive([7]);
    module.section(&data);
    assert!(plan::Plan::read(&module.finish()).unwrap().is_none());
    assert!(plan::Plan::read(b"\0asm\x01\0\0\0").unwrap().is_none());
}
#[test]
fn every_runtime_function_reference_site_is_retained_in_the_initial_scan() {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut imports = ImportSection::new();
    for name in ["export", "global", "table", "element", "call", "declared"] {
        imports.import("R", name, EntityType::Function(0));
    }
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let table_type = TableType {
        element_type: RefType::FUNCREF,
        table64: false,
        minimum: 1,
        maximum: None,
        shared: false,
    };
    let mut tables = TableSection::new();
    tables.table_with_init(table_type, &ConstExpr::ref_func(2));
    module.section(&tables);
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: ValType::FUNCREF,
            mutable: false,
            shared: false,
        },
        &ConstExpr::ref_func(1),
    );
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("function", ExportKind::Func, 0);
    module.section(&exports);
    let mut elements = ElementSection::new();
    elements.passive(Elements::Expressions(
        RefType::FUNCREF,
        Cow::Owned(vec![ConstExpr::ref_func(3)]),
    ));
    elements.declared(Elements::Functions(Cow::Borrowed(&[1, 2, 3, 5])));
    module.section(&elements);
    let mut function = Function::new([]);
    function.instruction(&Instruction::ReturnCall(4));
    function.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&function);
    module.section(&code);
    let plan = plan::Plan::read(&module.finish()).unwrap().unwrap();
    assert_eq!(
        plan.function_map.keys().copied().collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
}

#[path = "tests/literal_operands.rs"]
mod literal_operand_tests;

#[path = "tests/program_owned_payload.rs"]
mod program_owned_payload;

#[path = "tests/emitted_names.rs"]
mod emitted_names;

#[path = "tests/shared_emitted_bodies.rs"]
mod shared_emitted_bodies;

#[path = "tests/import_compaction.rs"]
mod import_compaction;
