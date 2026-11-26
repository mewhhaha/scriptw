#![cfg(test)]
use super::*;
use crate::native_program_contexts::{FunctionContext, ModuleContext, Schema};

// A wire fixture checks malformed context witnesses independently of the emitter.
fn fixture(value: i32, local: u32, ready: u32, initializer: u32) -> Vec<u8> {
    // The payload keeps its actual ProgramInstance field ordinal; the fixture
    // fills every earlier field with an i32.
    let payload_field = crate::gc_types::ProgramInstanceLocals::native_payload_field();
    let schema = Schema {
        program_type: 1,
        payload_type: 0,
        payload_field,
    };
    let reference = |index, nullable| {
        ValType::Ref(RefType {
            nullable,
            heap_type: HeapType::Concrete(index),
        })
    };
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().array(&StorageType::I8, false);
    let mut fields = vec![
        wasm_encoder::FieldType {
            element_type: StorageType::Val(ValType::I32),
            mutable: false
        };
        payload_field as usize
    ];
    fields.push(wasm_encoder::FieldType {
        element_type: StorageType::Val(reference(0, true)),
        mutable: false,
    });
    types.ty().struct_(fields);
    types.ty().function([], [reference(1, false)]);
    types.ty().function([reference(1, false)], [ValType::I32]);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import("R", "unused", EntityType::Function(3));
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(2);
    functions.function(3);
    module.section(&functions);
    let mut exports = ExportSection::new();
    exports.export(
        crate::runtime_function::PROGRAM_INITIALIZE_EXPORT,
        ExportKind::Func,
        initializer,
    );
    exports.export("read", ExportKind::Func, 2);
    module.section(&exports);
    let mut bootstrap = Function::new([]);
    for _ in 0..payload_field {
        bootstrap.instruction(&Instruction::I32Const(0));
    }
    bootstrap.instruction(&Instruction::RefNull(HeapType::Concrete(0)));
    bootstrap.instruction(&Instruction::StructNew(1));
    bootstrap.instruction(&Instruction::End);
    let mut body = Function::new([(1, reference(1, true))]);
    body.instruction(&Instruction::LocalGet(0));
    body.instruction(&Instruction::LocalSet(1));
    body.instruction(&Instruction::I32Const(value));
    body.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&bootstrap);
    code.function(&body);
    module.section(&code);
    module.section(
        &crate::native_program_contexts::section(&[
            ModuleContext {
                body: 0,
                context: FunctionContext::Bootstrap {
                    schema,
                    payload_operator: Some(payload_field),
                },
            },
            ModuleContext {
                body: 1,
                context: FunctionContext::Program {
                    schema,
                    local,
                    ready,
                },
            },
        ])
        .unwrap(),
    );
    let mut operands = Vec::new();
    for word in [1_u32, 1, 1, 2] {
        operands.extend_from_slice(&word.to_le_bytes());
    }
    operands.push(0);
    operands.extend_from_slice(&value.to_le_bytes());
    module.section(&wasm_encoder::CustomSection {
        name: Cow::Borrowed(crate::program_literal_operands::SECTION_NAME),
        data: Cow::Owned(operands),
    });
    module.finish()
}

#[test]
fn owned_literals_share_valid_code_with_a_typed_payload_initializer_and_fresh_data() {
    let first = fixture(17, 1, 2, 1);
    let second = fixture(29, 1, 2, 1);
    let a = plan::Plan::read(&first).unwrap().unwrap();
    let b = plan::Plan::read(&second).unwrap().unwrap();
    let code = reencode::encode(&first, &a).unwrap();
    assert_eq!(code, reencode::encode(&second, &b).unwrap());
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&code)
        .unwrap();
    let a = NativeProgramPayload::from_plan(&a).unwrap().unwrap();
    let b = NativeProgramPayload::from_plan(&b).unwrap().unwrap();
    assert!(a.permits_instance_reuse());
    assert_ne!(a.bytes(), b.bytes());
    assert_eq!(&a.bytes()[..4], &17_i32.to_le_bytes());
    assert_eq!(&b.bytes()[..4], &29_i32.to_le_bytes());
}

#[test]
fn owner_context_rejects_parameters_missing_locals_unready_bindings_and_wrong_bootstrap() {
    for (local, ready, initializer) in [(0, 2, 1), (2, 2, 1), (1, 1, 1), (1, 2, 2)] {
        assert!(plan::Plan::read(&fixture(17, local, ready, initializer)).is_err());
    }
}
