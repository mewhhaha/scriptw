//! The original vector ABI only unpacks arguments and tail-enters one source body.
use super::*;

pub(super) fn compile(
    source: &crate::function_entry::PlannedFunctionEntry,
    schema: &RuntimeSchema,
) -> Result<EmittedFunction, EmitError> {
    let entry = source.local_arguments().ok_or_else(|| {
        EmitError::unsupported("local argument adapter requires its source-bound entry")
    })?;
    let mut function = Function::with_parameters(
        LocalDeclarations::from_types([]),
        schema.signature(source.signature()).parameters().to_vec(),
    );
    let callable = schema.parameter_gc::<FunctionObject, NonNullable>(0, &mut function);
    let this_value = schema.parameter_value(1, &mut function);
    let new_target = schema.parameter_value(4, &mut function);
    let count = schema.parameter_i64(7, &mut function);
    let vector = schema.parameter_gc::<ValueArray, Nullable>(8, &mut function);
    let caller_realm = schema.parameter_gc::<RealmRecord, NonNullable>(9, &mut function);
    let program = schema.parameter_gc::<ProgramInstance, NonNullable>(10, &mut function);
    // Validate the existing ABI without allocating an empty replacement vector.
    vector.load(schema, &mut function);
    function.instruction(&Instruction::RefIsNull);
    function.instruction(&Instruction::If(BlockType::Empty));
    count.load(&mut function);
    function.instruction(&Instruction::I64Eqz);
    function.instruction(&Instruction::I32Eqz);
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::Unreachable);
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::Else);
    let values = schema.reserve_gc_local(&mut function).initialize(
        vector
            .load(schema, &mut function)
            .require_non_null(&mut function),
        &mut function,
    );
    schema
        .array_type::<ValueArray>()
        .length(&values, schema, &mut function);
    function.instruction(&Instruction::I64ExtendI32U);
    count.load(&mut function);
    function.instruction(&Instruction::I64Ne);
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::Unreachable);
    function.instruction(&Instruction::End);
    values.clear(&mut function);
    function.instruction(&Instruction::End);

    let arguments = [
        schema.reserve_value_local(&mut function),
        schema.reserve_value_local(&mut function),
    ];
    for (index, argument) in arguments.iter().enumerate() {
        argument.set_undefined(&mut function);
        count.load(&mut function);
        function.instruction(&Instruction::I64Const(index as i64));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        let values = schema.reserve_gc_local(&mut function).initialize(
            vector
                .load(schema, &mut function)
                .require_non_null(&mut function),
            &mut function,
        );
        let slot = schema.reserve_i32_local(&mut function);
        slot.set_constant(index as i32, &mut function);
        let value = schema.reserve_gc_local(&mut function).initialize(
            schema
                .array_type::<ValueArray>()
                .read(&values, slot, schema, &mut function)
                .reference(),
            &mut function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&value, argument, schema, &mut function);
        value.clear(&mut function);
        schema.release_i32_local(slot, &mut function);
        values.clear(&mut function);
        function.instruction(&Instruction::End);
    }
    callable.load(schema, &mut function);
    this_value.emit(&mut function);
    new_target.emit(&mut function);
    for argument in &arguments {
        argument.emit(&mut function);
    }
    caller_realm.load(schema, &mut function);
    program.load(schema, &mut function);
    entry.emit_return_call_instruction(&mut function);
    function.instruction(&Instruction::End);
    Ok(EmittedFunction::local_arguments_adapter(
        source.clone(),
        function,
    ))
}
