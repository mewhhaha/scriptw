use super::*;
use crate::functions::ArgumentListConstruction;
use crate::gc_types::{CodeUnitArray, Nullable, StringConstruction, StringValueSchema};

/// Only completed strings enter this list. User capture Gets and conversions
/// happen once before append; final publication only copies their UTF-16 units.
pub(super) struct SubstitutionPieces(ArgumentListConstruction);

impl SubstitutionPieces {
    pub(super) fn new(b: &FunctionBuilder<'_>, f: &mut Function) -> Self {
        Self(ArgumentListConstruction::new(b, f))
    }

    pub(super) fn append(
        &self,
        string: &GcLocal<StringValue>,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        let value = s.reserve_value_local(f);
        value.set_reference(string, s, f);
        self.0.append(&value, s, f);
        value.clear(f);
    }

    pub(super) fn publish(
        self,
        b: &mut FunctionBuilder<'_>,
        total: I64Local,
        f: &mut Function,
    ) -> GcLocal<StringValue> {
        let s = b.runtime_schema();
        let pieces = self.0.finish(b, f);
        let length = s.reserve_i32_local(f);
        total.load(f);
        f.instruction(&Instruction::I32WrapI64);
        length.store(f);
        let construction = StringConstruction::allocate(s, s.reserve_gc_local(f), length, f);
        let destination = s.reserve_i32_local(f);
        destination.set_constant(0, f);
        copy_values_into(b, &pieces, &construction, destination, f);
        pieces.clear(f);
        let result = s
            .reserve_gc_local(f)
            .initialize(construction.publish(s, f), f);
        s.release_i32_local(destination, f);
        s.release_i32_local(length, f);
        result
    }
}

fn copy_values_into(
    b: &mut FunctionBuilder<'_>,
    pieces: &GcLocal<ValueArray>,
    construction: &StringConstruction,
    destination: I32Local,
    f: &mut Function,
) {
    let s = b.runtime_schema();
    let index = s.reserve_i32_local(f);
    let count = s.reserve_i32_local(f);
    let value = s.reserve_value_local(f);
    index.set_constant(0, f);
    s.array_type::<ValueArray>().length(pieces, s, f);
    count.store(f);
    let done = b.open_frame(ControlFrameKind::Block, f);
    let next = b.open_frame(ControlFrameKind::Loop, f);
    index.load(f);
    count.load(f);
    f.instruction(&Instruction::I32GeU);
    b.emit_branch_if_to_target(done, f);
    b.emit_argument_vector_entry_to_value(pieces, index, &value, f);
    let string = s
        .reserve_gc_local(f)
        .initialize(value.cast_reference::<StringValue>(s, f), f);
    copy_piece_into(s, construction, destination, &string, f);
    string.clear(f);
    index.load(f);
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    index.store(f);
    b.emit_branch_to_target(next, f);
    b.pop_control(ControlFrameKind::Loop);
    f.instruction(&Instruction::End);
    b.pop_control(ControlFrameKind::Block);
    f.instruction(&Instruction::End);
    value.clear(f);
    for local in [index, count] {
        s.release_i32_local(local, f);
    }
}

fn copy_piece_into(
    s: &RuntimeSchema,
    construction: &StringConstruction,
    destination: I32Local,
    string: &GcLocal<StringValue>,
    f: &mut Function,
) {
    let units = s.reserve_gc_local(f).initialize(
        s.struct_type::<StringValue>()
            .field(StringValueSchema::CODE_UNITS)
            .read(string, s, f)
            .reference(),
        f,
    );
    let length = s.reserve_i32_local(f);
    s.array_type::<CodeUnitArray>().length(&units, s, f);
    length.store(f);
    construction.copy_units_from(destination, &units, length, s, f);
    destination.load(f);
    length.load(f);
    f.instruction(&Instruction::I32Add);
    destination.store(f);
    s.release_i32_local(length, f);
    units.clear(f);
}

const INLINE_PIECES: usize = 8;

/// Completed strings stay in typed local roots before the spill list is needed.
/// Publication consumes the owner and copies every UTF-16 span exactly once.
#[must_use = "replacement pieces must be consumed by final publication"]
pub(super) struct ReplacementPieces {
    pieces: SubstitutionPieces,
    inline: [GcLocal<StringValue, Nullable>; INLINE_PIECES],
    count: I32Local,
    total: I64Local,
}

impl ReplacementPieces {
    pub(super) fn new(b: &FunctionBuilder<'_>, f: &mut Function) -> Self {
        let s = b.runtime_schema();
        let total = s.reserve_i64_local(f);
        total.set_constant(0, f);
        let count = s.reserve_i32_local(f);
        count.set_constant(0, f);
        Self {
            pieces: SubstitutionPieces::new(b, f),
            inline: std::array::from_fn(|_| s.reserve_gc_local(f).initialize_null(s, f)),
            count,
            total,
        }
    }

    pub(super) fn append(
        &self,
        b: &mut FunctionBuilder<'_>,
        text: &GcLocal<StringValue>,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = b.runtime_schema();
        let length = s.reserve_i64_local(f);
        b.emit_native_gc_string_length(text, length, f);
        self.total.load(f);
        length.load(f);
        f.instruction(&Instruction::I64Add);
        self.total.store(f);
        self.total.load(f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        f.instruction(&Instruction::I64GtU);
        b.emit_native_string_error_if(
            RuntimeErrorMessage::REPLACEMENT_RESULT_WOULD_EXCEED_MAXIMUM_STRING_LENGTH,
            NativeErrorKind::RangeError,
            output,
            exit,
            f,
        )?;
        // Empty pieces have no observable identity and contribute no units.
        // Keep their completed conversions, but allocate no list node for them.
        length.load(f);
        f.instruction(&Instruction::I64Eqz);
        b.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Else);
        self.count.load(f);
        f.instruction(&Instruction::I32Const(INLINE_PIECES as i32));
        f.instruction(&Instruction::I32LtU);
        b.open_frame(ControlFrameKind::If, f);
        for (index, slot) in self.inline.iter().enumerate() {
            self.count.load(f);
            f.instruction(&Instruction::I32Const(index as i32));
            f.instruction(&Instruction::I32Eq);
            b.open_frame(ControlFrameKind::If, f);
            slot.replace(text.load(s, f).nullable(), f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::Else);
        self.pieces.append(text, s, f);
        b.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Nonempty piece count cannot exceed the checked total UTF-16 length.
        self.count.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        self.count.store(f);
        b.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(length, f);
        Ok(())
    }

    pub(super) fn publish(
        self,
        b: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> GcLocal<StringValue> {
        let s = b.runtime_schema();
        let rest = s.reserve_gc_local(f).initialize(s.empty_value_array(f), f);
        self.count.load(f);
        f.instruction(&Instruction::I32Const(INLINE_PIECES as i32));
        f.instruction(&Instruction::I32GtU);
        b.open_frame(ControlFrameKind::If, f);
        // Flatten and drop list nodes before allocating the final backing.
        let finished = self.pieces.0.finish(b, f);
        rest.replace(finished.load(s, f), f);
        finished.clear(f);
        b.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let length = s.reserve_i32_local(f);
        self.total.load(f);
        f.instruction(&Instruction::I32WrapI64);
        length.store(f);
        let construction = StringConstruction::allocate(s, s.reserve_gc_local(f), length, f);
        let destination = s.reserve_i32_local(f);
        destination.set_constant(0, f);
        for slot in self.inline {
            slot.load(s, f);
            f.instruction(&Instruction::RefIsNull);
            f.instruction(&Instruction::I32Eqz);
            b.open_frame(ControlFrameKind::If, f);
            let string = s
                .reserve_gc_local(f)
                .initialize(slot.load(s, f).require_non_null(f), f);
            copy_piece_into(s, &construction, destination, &string, f);
            string.clear(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            slot.clear(f);
        }
        self.count.load(f);
        f.instruction(&Instruction::I32Const(INLINE_PIECES as i32));
        f.instruction(&Instruction::I32GtU);
        b.open_frame(ControlFrameKind::If, f);
        copy_values_into(b, &rest, &construction, destination, f);
        b.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        rest.clear(f);
        let result = s
            .reserve_gc_local(f)
            .initialize(construction.publish(s, f), f);
        s.release_i32_local(destination, f);
        s.release_i32_local(length, f);
        s.release_i32_local(self.count, f);
        s.release_i64_local(self.total, f);
        result
    }
}
