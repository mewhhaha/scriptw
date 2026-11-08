//! An Arguments alias retains its binding cell independently of the descriptor.

use super::*;
use crate::gc_types::{
    ArgumentsObject, ArgumentsObjectSchema, ArgumentsParameterMap, BindingCell, GcLocal, I32Local,
    Nullable, ValueLocals,
};

impl FunctionBuilder<'_> {
    /// Capture before Define changes either the descriptor or ParameterMap.
    pub(crate) fn emit_arguments_index_mapping(
        &mut self,
        arguments: &GcLocal<ArgumentsObject>,
        index: I32Local,
        function: &mut Function,
    ) -> GcLocal<BindingCell, Nullable> {
        let schema = self.runtime_schema();
        let mapping = schema
            .reserve_gc_local::<BindingCell, Nullable>(function)
            .initialize_null(schema, function);
        let map = schema
            .reserve_gc_local::<ArgumentsParameterMap, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<ArgumentsObject>()
                    .field(ArgumentsObjectSchema::PARAMETER_MAP)
                    .read(arguments, schema, function)
                    .reference(),
                function,
            );
        let done = self.open_frame(ControlFrameKind::Block, function);
        map.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        index.load(function);
        schema
            .array_type::<ArgumentsParameterMap>()
            .length(&map, schema, function);
        function.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        mapping.replace(
            schema
                .array_type::<ArgumentsParameterMap>()
                .read(&map, index, schema, function)
                .reference(),
            function,
        );
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        map.clear(function);
        mapping
    }

    /// Preserve the ordinary descriptor value when this index has no alias.
    pub(crate) fn emit_arguments_parameter_map_read(
        &mut self,
        mapping: &GcLocal<BindingCell, Nullable>,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        mapping.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<BindingCell>()
            .read_value(mapping, result, schema, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_arguments_parameter_map_write(
        &mut self,
        mapping: &GcLocal<BindingCell, Nullable>,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        mapping.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<BindingCell>()
            .write_value(mapping, value, schema, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }
}
