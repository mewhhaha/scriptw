//! One source-ordered constructor loop over compiler-validated passive recipes.
use super::*;
use crate::data::{CompactArrayOpcode, CompactArrayRecipe, COMPACT_ARRAY_RECIPE_ROW_BYTES};
use crate::gc_types::{
    ArrayIndexKeyConstruction, ByteArray, I32Local, NonNullable, ScalarValue, StringValue,
    TemplateSiteTable,
};
use crate::runtime_helpers::{
    CompactArrayLiteralArguments, CompactArrayLiteralParameters, HelperParameters,
};

impl FunctionBuilder<'_> {
    pub(super) fn compile_compact_array_literal(
        &mut self,
        recipe: CompactArrayRecipe,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let prototype = s.reserve_value_local(f);
        self.emit_source_literal_prototype_to_value(
            crate::environments::global_environment::SourceLiteralPrototype::Array,
            &prototype,
            f,
        );
        let offset = s.reserve_i32_local(f);
        offset.set_constant(recipe.offset() as i32, f);
        let bytes = s.reserve_i32_local(f);
        bytes.set_constant(recipe.bytes() as i32, f);
        let depth = s.reserve_i32_local(f);
        depth.set_constant(recipe.depth() as i32, f);
        s.call_helper(
            CompactArrayLiteralArguments::new(
                offset,
                bytes,
                depth,
                &prototype,
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            f,
        )
        .store(self.completion(), f);
        output.copy_from(self.completion().value(), f);
        for local in [offset, bytes, depth] {
            s.release_i32_local(local, f);
        }
        prototype.clear(f);
        self.emit_propagate_current_throw_if_needed(f);
        Ok(())
    }

    /// The ByteArray never escapes this helper. A checked recipe constructor
    /// alone supplies its passive bounds, balanced frames and closed opcodes.
    fn load_compact_array_word(
        &self,
        tape: &GcLocal<ByteArray>,
        cursor: I32Local,
        offset: u32,
        width: u32,
        output: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let index = s.reserve_i32_local(f);
        let byte = s.reserve_i32_local(f);
        output.set_constant(0, f);
        for part in 0..width {
            cursor.load(f);
            f.instruction(&Instruction::I32Const((offset + part) as i32));
            f.instruction(&Instruction::I32Add);
            index.store(f);
            s.array_type::<ByteArray>()
                .read(tape, index, s, f)
                .store(byte, f);
            output.load(f);
            byte.load(f);
            f.instruction(&Instruction::I64ExtendI32U);
            if part != 0 {
                f.instruction(&Instruction::I64Const(i64::from(part * 8)));
                f.instruction(&Instruction::I64Shl);
            }
            f.instruction(&Instruction::I64Or);
            output.store(f);
        }
        s.release_i32_local(byte, f);
        s.release_i32_local(index, f);
    }

    pub(crate) fn compile_compact_array_literal_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::CompactArrayLiteral);
        let parameters = self.helper_parameters::<CompactArrayLiteralParameters>(&mut f);
        let s = self.runtime_schema();
        let tape = s
            .reserve_gc_local::<ByteArray, NonNullable>(&mut f)
            .initialize(s.compact_array_recipe_data(&parameters, &mut f), &mut f);
        let arrays = s
            .reserve_gc_local::<TemplateSiteTable, NonNullable>(&mut f)
            .initialize(
                s.array_type::<TemplateSiteTable>().filled(
                    GcOperand::null(s),
                    parameters.depth,
                    &mut f,
                ),
                &mut f,
            );
        let indices = ArrayIndexKeyConstruction::allocate(s, parameters.depth, &mut f);
        let value = s.reserve_value_local(&mut f);
        value.set_undefined(&mut f);
        let cursor = s.reserve_i32_local(&mut f);
        cursor.set_constant(0, &mut f);
        let depth = s.reserve_i32_local(&mut f);
        depth.set_constant(0, &mut f);
        let slot = s.reserve_i32_local(&mut f);
        let kind = s.reserve_i32_local(&mut f);
        let recognized = s.reserve_i32_local(&mut f);
        let publish = s.reserve_i32_local(&mut f);
        let advance = s.reserve_i32_local(&mut f);
        let ordinal = s.reserve_i64_local(&mut f);
        let payload = s.reserve_i64_local(&mut f);
        let result = s.reserve_completion(&mut f);
        result.initialize(&mut f);
        let done = self.open_frame(ControlFrameKind::Block, &mut f);
        let next = self.open_frame(ControlFrameKind::Loop, &mut f);
        cursor.load(&mut f);
        parameters.bytes.load(&mut f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, &mut f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.load_compact_array_word(&tape, cursor, 0, 4, payload, &mut f);
        payload.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        kind.store(&mut f);
        self.load_compact_array_word(&tape, cursor, 8, 8, payload, &mut f);
        recognized.set_constant(0, &mut f);
        publish.set_constant(0, &mut f);
        advance.set_constant(0, &mut f);
        for opcode in CompactArrayOpcode::ALL.iter().copied() {
            kind.load(&mut f);
            f.instruction(&Instruction::I32Const(opcode.word() as i32));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, &mut f);
            recognized.set_constant(1, &mut f);
            match opcode {
                CompactArrayOpcode::Begin => {
                    let array = s
                        .reserve_gc_local::<ArrayObject, NonNullable>(&mut f)
                        .initialize(
                            self.emit_alloc_array_payload_with_length_and_prototype(
                                payload,
                                &parameters.prototype,
                                &mut f,
                            )?,
                            &mut f,
                        );
                    s.array_type::<TemplateSiteTable>().write(
                        &arrays,
                        depth,
                        GcOperand::nullable_reference(&array, s),
                        s,
                        &mut f,
                    );
                    ordinal.set_constant(0, &mut f);
                    indices.write(depth, ordinal, s, &mut f);
                    array.clear(&mut f);
                    depth.load(&mut f);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Add);
                    depth.store(&mut f);
                }
                CompactArrayOpcode::End => {
                    depth.load(&mut f);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Sub);
                    depth.store(&mut f);
                    let array = s
                        .reserve_gc_local::<ArrayObject, NonNullable>(&mut f)
                        .initialize(
                            s.array_type::<TemplateSiteTable>()
                                .read(&arrays, depth, s, &mut f)
                                .reference()
                                .require_non_null(&mut f),
                            &mut f,
                        );
                    value.set_reference(&array, s, &mut f);
                    s.array_type::<TemplateSiteTable>().write(
                        &arrays,
                        depth,
                        GcOperand::null(s),
                        s,
                        &mut f,
                    );
                    array.clear(&mut f);
                    depth.load(&mut f);
                    f.instruction(&Instruction::I32Eqz);
                    self.open_frame(ControlFrameKind::If, &mut f);
                    result.set_normal(&value, &mut f);
                    self.emit_branch_to_target(done, &mut f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    publish.set_constant(1, &mut f);
                    advance.set_constant(1, &mut f);
                }
                CompactArrayOpcode::Hole => {
                    advance.set_constant(1, &mut f);
                }
                CompactArrayOpcode::Undefined => {
                    value.set_undefined(&mut f);
                    publish.set_constant(1, &mut f);
                    advance.set_constant(1, &mut f);
                }
                CompactArrayOpcode::Null => {
                    value.set_scalar(ScalarValue::Null, &mut f);
                    publish.set_constant(1, &mut f);
                    advance.set_constant(1, &mut f);
                }
                CompactArrayOpcode::Boolean => {
                    payload.load(&mut f);
                    f.instruction(&Instruction::I32WrapI64);
                    slot.store(&mut f);
                    value.set_boolean(slot, &mut f);
                    publish.set_constant(1, &mut f);
                    advance.set_constant(1, &mut f);
                }
                CompactArrayOpcode::Number => {
                    value.set_number(payload, &mut f);
                    publish.set_constant(1, &mut f);
                    advance.set_constant(1, &mut f);
                }
                CompactArrayOpcode::String => {
                    let string = s
                        .reserve_gc_local::<StringValue, NonNullable>(&mut f)
                        .initialize(
                            self.current_program().literal_operand(
                                s,
                                payload,
                                self.runtime_helper_base()?,
                                &mut f,
                            ),
                            &mut f,
                        );
                    value.set_reference(&string, s, &mut f);
                    string.clear(&mut f);
                    publish.set_constant(1, &mut f);
                    advance.set_constant(1, &mut f);
                }
            }
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        recognized.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        advance.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        depth.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        slot.store(&mut f);
        indices.read(slot, ordinal, s, &mut f);
        publish.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        let parent = s
            .reserve_gc_local::<ArrayObject, NonNullable>(&mut f)
            .initialize(
                s.array_type::<TemplateSiteTable>()
                    .read(&arrays, slot, s, &mut f)
                    .reference()
                    .require_non_null(&mut f),
                &mut f,
            );
        // Validated recipes supply unique in-range ordinals in private arrays.
        self.emit_array_indexed_publish_default_data(&parent, ordinal, &value, &mut f)?;
        parent.clear(&mut f);
        value.set_undefined(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        ordinal.load(&mut f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        ordinal.store(&mut f);
        indices.write(slot, ordinal, s, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.load(&mut f);
        f.instruction(&Instruction::I32Const(
            COMPACT_ARRAY_RECIPE_ROW_BYTES as i32,
        ));
        f.instruction(&Instruction::I32Add);
        cursor.store(&mut f);
        self.emit_branch_to_target(next, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        result.emit(&mut f);
        result.clear(&mut f);
        value.clear(&mut f);
        indices.clear(&mut f);
        arrays.clear(&mut f);
        tape.clear(&mut f);
        for local in [cursor, depth, slot, kind, recognized, publish, advance] {
            s.release_i32_local(local, &mut f);
        }
        for local in [ordinal, payload] {
            s.release_i64_local(local, &mut f);
        }
        parameters.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
