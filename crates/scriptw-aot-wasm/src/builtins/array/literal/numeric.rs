//! Bounded fresh literals can install one complete packed Number backing.
use super::*;
use crate::gc_types::{ArrayObjectSchema, NumberBitsI64Array, NumberPresenceU64Array};

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
        let mut values = Vec::with_capacity(source.elements.len());
        for element in source.elements {
            let value = s.reserve_value_local(f);
            self.compile_expr_to_value(element, &value, f)?;
            self.emit_propagate_current_throw_if_needed(f);
            values.push(value);
        }
        // The private Array cannot escape through an element expression.
        // Its default own definitions call no user code, so their publication
        // can follow evaluation. Keep every captured reference rooted until
        // either representation has consumed it, including across allocations.
        for (ordinal, value) in values.iter().enumerate() {
            value.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
            f.instruction(&Instruction::I32Eq);
            if ordinal != 0 {
                f.instruction(&Instruction::I32And);
            }
        }
        self.open_frame(ControlFrameKind::If, f);
        let numbers = s.reserve_gc_local(f).initialize(
            s.array_type::<NumberBitsI64Array>().fixed(
                values
                    .iter()
                    .map(|value| GcOperand::i64_local(value.scalar()))
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
        let storage = s
            .reserve_gc_local(f)
            .initialize(literal.array.load(s, f), f);
        // FreshArrayLiteral alone owns this initially empty PackedNumbers
        // store. Both complete backings are rooted before any field changes;
        // these writes allocate nothing and cannot expose partial state.
        s.field(ArrayObjectSchema::NUMBERS).write(
            &storage,
            GcOperand::reference(&numbers, s),
            s,
            f,
        );
        s.field(ArrayObjectSchema::NUMBER_PRESENT).write(
            &storage,
            GcOperand::reference(&presence, s),
            s,
            f,
        );
        s.field(ArrayObjectSchema::COUNT).write(
            &storage,
            GcOperand::i64(i64::from(source.length())),
            s,
            f,
        );
        storage.clear(f);
        presence.clear(f);
        numbers.clear(f);
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
}
