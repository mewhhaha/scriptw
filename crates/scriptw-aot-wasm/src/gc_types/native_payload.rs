//! Wire-level access for the checked native-template GC payload schema.
use wasm_encoder::{Instruction, ValType};

pub(crate) fn load(
    function: &mut wasm_encoder::Function,
    local: u32,
    schema: crate::native_program_contexts::Schema,
) {
    function.instruction(&Instruction::LocalGet(local));
    function.instruction(&Instruction::StructGet {
        struct_type_index: schema.program_type,
        field_index: schema.payload_field,
    });
    // Only template bodies contain rewritten source operands. Their checked
    // initializer publishes the required non-null payload argument.
    function.instruction(&Instruction::RefAsNonNull);
}

pub(crate) fn append_literal_helper(
    code: &mut wasm_encoder::CodeSection,
    schema: crate::native_program_contexts::Schema,
) {
    let array_type = schema.payload_type;
    let mut function = wasm_encoder::Function::new([(2, ValType::I32)]);
    // Parameters: ordinal0, immutable payload1; locals: cursor2, limit3.
    let word = |function: &mut wasm_encoder::Function| {
        for byte in 0..4 {
            function.instruction(&Instruction::LocalGet(1));
            function.instruction(&Instruction::LocalGet(2));
            if byte != 0 {
                function.instruction(&Instruction::I32Const(byte));
                function.instruction(&Instruction::I32Add);
            }
            function.instruction(&Instruction::ArrayGetU(array_type));
            if byte != 0 {
                function.instruction(&Instruction::I32Const(byte * 8));
                function.instruction(&Instruction::I32Shl);
                function.instruction(&Instruction::I32Or);
            }
        }
    };
    function.instruction(&Instruction::LocalGet(1));
    function.instruction(&Instruction::ArrayLen);
    function.instruction(&Instruction::I32Const(4));
    function.instruction(&Instruction::I32Sub);
    function.instruction(&Instruction::LocalSet(2));
    word(&mut function);
    function.instruction(&Instruction::LocalSet(3));
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::LocalGet(3));
    function.instruction(&Instruction::I32GeU);
    function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
    function.instruction(&Instruction::Unreachable);
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::LocalGet(1));
    function.instruction(&Instruction::ArrayLen);
    function.instruction(&Instruction::I32Const(8));
    function.instruction(&Instruction::I32Sub);
    function.instruction(&Instruction::LocalSet(2));
    word(&mut function);
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::I32Const(4));
    function.instruction(&Instruction::I32Mul);
    function.instruction(&Instruction::I32Add);
    function.instruction(&Instruction::LocalSet(2));
    word(&mut function);
    function.instruction(&Instruction::End);
    code.function(&function);
}
