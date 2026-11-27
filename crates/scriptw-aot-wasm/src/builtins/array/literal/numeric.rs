//! Bounded fresh literals can install one complete packed Number backing.
use super::*;
use crate::gc_types::{ArrayObjectSchema, I64Local, NumberBitsI64Array, NumberPresenceU64Array};

/// Bounds both retained element roots and the additional source-body code.
/// Elisions keep their existing path and explicit absent-slot semantics.
pub(super) struct SmallDenseArrayLiteral<'a> {
    elements: &'a [TypedExpr],
}
impl<'a> SmallDenseArrayLiteral<'a> {
    pub(super) fn new(elements: &'a [TypedExpr]) -> Option<Self> {
        if !(1..=8).contains(&elements.len())
            || elements
                .iter()
                .any(|element| matches!(element.expr, ExprIr::ArrayHole))
        {
            return None;
        }
        Some(Self { elements })
    }

    fn length(&self) -> u32 {
        u32::try_from(self.elements.len()).expect("small literal length validated at admission")
    }

    fn capacity(&self) -> u32 {
        // Keep the existing packed-storage growth and snapshot invariant.
        self.length().next_power_of_two().max(4)
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn compile_small_dense_array_literal(
        &mut self,
        source: SmallDenseArrayLiteral<'_>,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // ArrayCreate and its source-Realm prototype precede element effects.
        let mut literal = FreshArrayLiteral::new(source.length(), self, f)?;
        if source
            .elements
            .iter()
            .all(|element| matches!(element.expr, ExprIr::Number(_)))
        {
            let mut bits = Vec::with_capacity(source.elements.len());
            for element in source.elements {
                let payload = s.reserve_i64_local(f);
                self.compile_number_payload(element, payload, f)?;
                self.emit_propagate_current_throw_if_needed(f);
                bits.push(payload);
            }
            self.emit_dense_number_literal_backing(&literal, &source, bits.iter().copied(), f);
            for payload in bits {
                s.release_i64_local(payload, f);
            }
            literal.finish(output, self, f);
            return Ok(());
        }

        let mut values = Vec::with_capacity(source.elements.len());
        for element in source.elements {
            let value = s.reserve_value_local(f);
            self.compile_expr_to_value(element, &value, f)?;
            self.emit_propagate_current_throw_if_needed(f);
            values.push(value);
        }
        // Default definitions on this private Array call no user code.
        // Root the evaluated values until either representation consumes them.
        for (ordinal, value) in values.iter().enumerate() {
            value.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
            f.instruction(&Instruction::I32Eq);
            if ordinal != 0 {
                f.instruction(&Instruction::I32And);
            }
        }
        self.open_frame(ControlFrameKind::If, f);
        self.emit_dense_number_literal_backing(
            &literal,
            &source,
            values.iter().map(ValueLocals::scalar),
            f,
        );
        f.instruction(&Instruction::Else);
        for (ordinal, value) in values.iter().enumerate() {
            let ordinal =
                u32::try_from(ordinal).expect("small literal ordinal validated at admission");
            literal.publish_element(ordinal, value, self, f)?;
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        literal.finish(output, self, f);
        for value in values {
            value.clear(f);
        }
        Ok(())
    }

    fn emit_dense_number_literal_backing(
        &self,
        literal: &FreshArrayLiteral,
        source: &SmallDenseArrayLiteral<'_>,
        bits: impl IntoIterator<Item = I64Local>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let numbers = s.reserve_gc_local(f).initialize(
            s.array_type::<NumberBitsI64Array>().fixed(
                bits.into_iter()
                    .map(GcOperand::i64_local)
                    .chain((source.length()..source.capacity()).map(|_| GcOperand::i64(0))),
                f,
            ),
            f,
        );
        let presence = s.reserve_gc_local(f).initialize(
            s.array_type::<NumberPresenceU64Array>()
                .fixed([GcOperand::i64((1_i64 << source.length()) - 1)], f),
            f,
        );
        // FreshArrayLiteral alone owns this initially empty PackedNumbers
        // store. Both complete backings are rooted before any field changes;
        // these writes allocate nothing and cannot expose partial state.
        s.field(ArrayObjectSchema::NUMBERS).write(
            &literal.array,
            GcOperand::reference(&numbers, s),
            s,
            f,
        );
        s.field(ArrayObjectSchema::NUMBER_PRESENT).write(
            &literal.array,
            GcOperand::reference(&presence, s),
            s,
            f,
        );
        s.field(ArrayObjectSchema::COUNT).write(
            &literal.array,
            GcOperand::i64(i64::from(source.length())),
            s,
            f,
        );
        presence.clear(f);
        numbers.clear(f);
    }
}
