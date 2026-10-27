//! Own indexed data value probe; deferred property values resolve on demand. `found` is separate from the
//! value: a present undefined is never confused with a hole or an accessor.
use super::*;
use crate::gc_types::ArrayObject;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_array_own_indexed_value(
        &mut self,
        array: &GcLocal<ArrayObject>,
        index: I64Local,
        output: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        found.set_constant(0, f);
        let storage = self.emit_array_index_storage(array, f);
        s.array_storage_is_packed(&storage, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_packed_array_value(&storage, index, output, found, f);
        f.instruction(&Instruction::Else);
        let descriptor = self.emit_array_indexed_descriptor(array, index, f);
        descriptor.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let present = s
            .reserve_gc_local(f)
            .initialize(descriptor.load(s, f).require_non_null(f), f);
        let flags = s.reserve_i32_local(f);
        s.field(PropertyDescriptorSchema::FLAGS)
            .read(&present, s, f)
            .store(flags, f);
        flags.load(f);
        f.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let stored = s.reserve_gc_local(f).initialize(
            s.field(PropertyDescriptorSchema::VALUE)
                .read(&present, s, f)
                .reference(),
            f,
        );
        self.emit_property_value_into(&stored, output, f);
        found.set_constant(1, f);
        stored.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(flags, f);
        present.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        descriptor.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        storage.clear(f);
    }
}
