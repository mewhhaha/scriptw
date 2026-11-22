//! Three private shape roots per runtime instance: the two native metadata
//! shapes and the iterator result shape. They retain primitive key strings
//! and immutable attributes only, never functions, values or Realms.
use super::*;

impl GcI32Constant for crate::heap::DescriptorWord {
    fn encode(self) -> i32 {
        self.as_i32()
    }
}

impl RuntimeSchema {
    pub(crate) fn load_native_metadata_shape(
        &self,
        configurable: I32Local,
        output: &GcLocal<CompactDataShape, Nullable>,
        function: &mut Function,
    ) {
        configurable.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        output.replace(
            self.configurable_native_metadata.load_nullable(function),
            function,
        );
        function.instruction(&Instruction::Else);
        output.replace(self.fixed_native_metadata.load_nullable(function), function);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn publish_native_metadata_shape(
        &self,
        configurable: I32Local,
        shape: &GcLocal<CompactDataShape>,
        function: &mut Function,
    ) {
        configurable.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        shape.load(self, function);
        emit_root_set(function, self.configurable_native_metadata);
        function.instruction(&Instruction::Else);
        shape.load(self, function);
        emit_root_set(function, self.fixed_native_metadata);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn load_iterator_result_shape(
        &self,
        output: &GcLocal<CompactDataShape, Nullable>,
        function: &mut Function,
    ) {
        output.replace(self.iterator_result_shape.load_nullable(function), function);
    }

    pub(crate) fn publish_iterator_result_shape(
        &self,
        shape: &GcLocal<CompactDataShape>,
        function: &mut Function,
    ) {
        shape.load(self, function);
        emit_root_set(function, self.iterator_result_shape);
    }
}
