use super::*;
use crate::gc_types::{
    BoundFunction, BoundFunctionSchema, GcLocal, GcOperand, I32Local, RuntimeSchema, StoredValue,
    ValueArray, ValueLocals,
};

/// A rooted view of the actual bound exotic. Its identity remains the original
/// BoundFunction reference; no invoker FunctionObject or environment is made.
pub(crate) struct BoundFunctionRecordLocals {
    target: ValueLocals,
    this_value: ValueLocals,
    arguments: GcLocal<ValueArray>,
    constructable: I32Local,
}

impl BoundFunctionRecordLocals {
    pub(crate) fn target(&self) -> &ValueLocals {
        &self.target
    }
    pub(crate) fn this_value(&self) -> &ValueLocals {
        &self.this_value
    }
    pub(crate) fn arguments(&self) -> &GcLocal<ValueArray> {
        &self.arguments
    }
    pub(crate) fn clear(self, schema: &RuntimeSchema, function: &mut Function) {
        schema.release_i32_local(self.constructable, function);
        self.arguments.clear(function);
        self.this_value.clear(function);
        self.target.clear(function);
    }
}

fn stored_value(
    stored: GcLocal<StoredValue>,
    schema: &RuntimeSchema,
    function: &mut Function,
) -> ValueLocals {
    let value = schema.reserve_value_local(function);
    schema
        .struct_type::<StoredValue>()
        .read_into(&stored, &value, schema, function);
    stored.clear(function);
    value
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_load_bound_function_record(
        &self,
        bound: &GcLocal<BoundFunction>,
        function: &mut Function,
    ) -> BoundFunctionRecordLocals {
        let schema = self.runtime_schema();
        let record = schema.struct_type::<BoundFunction>();
        let target = schema.reserve_gc_local(function).initialize(
            record
                .field(BoundFunctionSchema::TARGET)
                .read(bound, schema, function)
                .reference(),
            function,
        );
        let target = stored_value(target, schema, function);
        let this_value = schema.reserve_gc_local(function).initialize(
            record
                .field(BoundFunctionSchema::THIS_VALUE)
                .read(bound, schema, function)
                .reference(),
            function,
        );
        let this_value = stored_value(this_value, schema, function);
        let arguments = schema.reserve_gc_local(function).initialize(
            record
                .field(BoundFunctionSchema::ARGUMENTS)
                .read(bound, schema, function)
                .reference(),
            function,
        );
        let constructable = schema.reserve_i32_local(function);
        record
            .field(BoundFunctionSchema::CONSTRUCTABLE)
            .read(bound, schema, function)
            .store(constructable, function);
        BoundFunctionRecordLocals {
            target,
            this_value,
            arguments,
            constructable,
        }
    }

    /// Concatenate argument lists by copying StoredValue references. Object,
    /// Symbol and primitive reference identity is preserved for every element.
    pub(crate) fn emit_concat_argument_vectors(
        &self,
        lhs: &GcLocal<ValueArray>,
        rhs: &GcLocal<ValueArray>,
        function: &mut Function,
    ) -> GcLocal<ValueArray> {
        let schema = self.runtime_schema();
        let array = schema.array_type::<ValueArray>();
        let lhs_length = schema.reserve_i32_local(function);
        let rhs_length = schema.reserve_i32_local(function);
        let total_length = schema.reserve_i32_local(function);
        let source_index = schema.reserve_i32_local(function);
        let destination_index = schema.reserve_i32_local(function);
        array.length(lhs, schema, function);
        lhs_length.store(function);
        array.length(rhs, schema, function);
        rhs_length.store(function);
        lhs_length.load(function);
        rhs_length.load(function);
        function.instruction(&Instruction::I32Add);
        total_length.store(function);
        total_length.load(function);
        lhs_length.load(function);
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);

        let empty_value = schema.reserve_value_local(function);
        empty_value.set_undefined(function);
        let empty_element = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&empty_value, function),
            function,
        );
        empty_value.clear(function);
        let destination = schema.reserve_gc_local(function).initialize(
            array.filled(
                GcOperand::reference(&empty_element, schema),
                total_length,
                function,
            ),
            function,
        );
        empty_element.clear(function);
        function.instruction(&Instruction::I32Const(0));
        destination_index.store(function);
        for (source, length) in [(lhs, lhs_length), (rhs, rhs_length)] {
            function.instruction(&Instruction::I32Const(0));
            source_index.store(function);
            function.instruction(&Instruction::Block(BlockType::Empty));
            function.instruction(&Instruction::Loop(BlockType::Empty));
            source_index.load(function);
            length.load(function);
            function.instruction(&Instruction::I32GeU);
            function.instruction(&Instruction::BrIf(1));
            let element = schema.reserve_gc_local(function).initialize(
                array
                    .read(source, source_index, schema, function)
                    .reference(),
                function,
            );
            array.write(
                &destination,
                destination_index,
                GcOperand::reference(&element, schema),
                schema,
                function,
            );
            element.clear(function);
            source_index.load(function);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32Add);
            source_index.store(function);
            destination_index.load(function);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32Add);
            destination_index.store(function);
            function.instruction(&Instruction::Br(0));
            function.instruction(&Instruction::End);
            function.instruction(&Instruction::End);
        }
        schema.release_i32_local(destination_index, function);
        schema.release_i32_local(source_index, function);
        schema.release_i32_local(total_length, function);
        schema.release_i32_local(rhs_length, function);
        schema.release_i32_local(lhs_length, function);
        destination
    }
}
