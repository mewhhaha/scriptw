//! Captured bindings the closed region reads but never writes.
//!
//! Only source code can assign a binding, and the sealed plan admits no
//! operation that runs source code (no call, property access, coercion or
//! environment operation besides these reads). A statically resolved
//! declarative cell therefore holds one value for the whole region, so each
//! is read once at loop entry. An uninitialized cell, or a non-Number value
//! where the region uses it arithmetically, is an entry miss: the original
//! loop then performs every read, ReferenceError and coercion itself.
use super::*;
use crate::gc_types::{RuntimeSchema, ScalarValue, ValueLocals};

/// The fixture shape of global-bound-deep reads twenty captured names.
const MAX_CAPTURED: usize = 32;
const MAX_INVARIANT_TESTS: usize = 16;

/// One statically resolved environment slot.
struct CapturedRead {
    slot: u32,
    hops: u32,
    /// Some arithmetic or relational use requires a Number snapshot.
    number: bool,
}

/// A side-effect-free operand of an invariant `===`/`!==`.
#[derive(Clone, Copy, PartialEq)]
enum InvariantOperand {
    Captured(usize),
    Undefined,
    Null,
    Boolean(bool),
    Number(u64),
}

impl InvariantOperand {
    /// A literal's value; a captured operand is already in its snapshot.
    fn literal(self, function: &mut Function, schema: &RuntimeSchema) -> Option<ValueLocals> {
        let literal = match self {
            Self::Captured(_) => return None,
            Self::Undefined => ScalarValue::Undefined,
            Self::Null => ScalarValue::Null,
            Self::Boolean(value) => ScalarValue::Boolean(value),
            Self::Number(bits) => ScalarValue::NumberBits(bits as i64),
        };
        let value = schema.reserve_value_local(function);
        value.set_scalar(literal, function);
        Some(value)
    }
}

impl InvariantOperand {
    fn value<'v>(
        self,
        captured: &'v [ValueLocals],
        literal: Option<&'v ValueLocals>,
    ) -> &'v ValueLocals {
        match self {
            Self::Captured(index) => &captured[index],
            Self::Undefined | Self::Null | Self::Boolean(_) | Self::Number(_) => {
                literal.expect("a literal operand owns its value")
            }
        }
    }
}

/// IsStrictlyEqual of two invariant operands, evaluated once at entry.
#[derive(PartialEq)]
struct InvariantTest {
    left: InvariantOperand,
    right: InvariantOperand,
}

/// Sealed by the loop proof; empty for a region without captured reads.
#[derive(Default)]
pub(super) struct CapturedReads {
    reads: Vec<CapturedRead>,
    tests: Vec<InvariantTest>,
}

/// Classification of an identifier the region reads: a private Number
/// local stays with the binding proof, everything else needs a slot.
enum Atom {
    Captured { slot: u32, hops: u32 },
    Literal(InvariantOperand),
}

impl CapturedReads {
    /// Whether this body may snapshot captured cells at all. A body whose
    /// own records are visible to source direct eval keeps the generic path.
    pub(super) fn admitted(builder: &FunctionBuilder<'_>) -> bool {
        builder.direct_eval_execution_context_local().is_none() && !builder.has_eval_environment()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.reads.is_empty()
    }

    fn atom(builder: &FunctionBuilder<'_>, value: &TypedExpr) -> Option<Atom> {
        Some(match &value.expr {
            ExprIr::Identifier(name) => {
                // `this` is not an environment cell of this compilation,
                // and private locals belong to the Number binding proof.
                if name == LEXICAL_THIS_NAME || builder.private_local_binding(name).is_some() {
                    return None;
                }
                // The original loop reads exactly this storage: an
                // Identifier carries an already resolved name and
                // `lookup_binding` wins over every global path.
                let BindingStorage::EnvSlot { slot, hops } = builder.lookup_binding(name)? else {
                    return None;
                };
                Atom::Captured { slot, hops }
            }
            ExprIr::Undefined => Atom::Literal(InvariantOperand::Undefined),
            ExprIr::Null => Atom::Literal(InvariantOperand::Null),
            ExprIr::Boolean(value) => Atom::Literal(InvariantOperand::Boolean(*value)),
            ExprIr::Number(bits) => Atom::Literal(InvariantOperand::Number(*bits)),
            _ => return None,
        })
    }

    fn intern(&mut self, slot: u32, hops: u32, number: bool) -> Option<usize> {
        if let Some(index) = self
            .reads
            .iter()
            .position(|read| read.slot == slot && read.hops == hops)
        {
            self.reads[index].number |= number;
            return Some(index);
        }
        if self.reads.len() == MAX_CAPTURED {
            return None;
        }
        self.reads.push(CapturedRead { slot, hops, number });
        Some(self.reads.len() - 1)
    }

    /// A Number use of a captured identifier.
    pub(super) fn number(
        &mut self,
        builder: &FunctionBuilder<'_>,
        value: &TypedExpr,
    ) -> Option<usize> {
        let Atom::Captured { slot, hops } = Self::atom(builder, value)? else {
            return None;
        };
        self.intern(slot, hops, true)
    }

    /// `left === right` where both operands are invariant and at least one
    /// is a captured read. Returns the test's entry slot.
    pub(super) fn strict_test(
        &mut self,
        builder: &FunctionBuilder<'_>,
        left: &TypedExpr,
        right: &TypedExpr,
    ) -> Option<usize> {
        let left = Self::atom(builder, left)?;
        let right = Self::atom(builder, right)?;
        if matches!((&left, &right), (Atom::Literal(_), Atom::Literal(_))) {
            return None;
        }
        let mut operand = |atom: Atom| match atom {
            Atom::Captured { slot, hops } => self
                .intern(slot, hops, false)
                .map(InvariantOperand::Captured),
            Atom::Literal(literal) => Some(literal),
        };
        let test = InvariantTest {
            left: operand(left)?,
            right: operand(right)?,
        };
        if let Some(index) = self.tests.iter().position(|existing| existing == &test) {
            return Some(index);
        }
        if self.tests.len() == MAX_INVARIANT_TESTS {
            return None;
        }
        self.tests.push(test);
        Some(self.tests.len() - 1)
    }

    /// Reads every cell once and folds initialization and Number admission
    /// into `accepted`. The snapshot is evaluated whatever `accepted` holds:
    /// each read is a plain cell load, and a rejected entry never uses it.
    pub(super) fn guard(
        &self,
        builder: &mut FunctionBuilder<'_>,
        accepted: I32Local,
        function: &mut Function,
    ) -> Result<CapturedSnapshot, EmitError> {
        let schema = builder.runtime_schema();
        let mut values = Vec::with_capacity(self.reads.len());
        let mut numbers = Vec::with_capacity(self.reads.len());
        for read in &self.reads {
            let value = schema.reserve_value_local(function);
            if let Some(initialized) =
                builder.read_env_slot_snapshot(read.slot, read.hops, &value, function)
            {
                accepted.load(function);
                initialized.load(function);
                function.instruction(&Instruction::I32And);
                accepted.store(function);
                schema.release_i32_local(initialized, function);
            }
            numbers.push(read.number.then(|| {
                accepted.load(function);
                value.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
                function.instruction(&Instruction::I32Eq);
                function.instruction(&Instruction::I32And);
                accepted.store(function);
                let number = schema.reserve_f64_local(function);
                value.scalar().load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                number.store(function);
                number
            }));
            values.push(value);
        }
        let mut tests = Vec::with_capacity(self.tests.len());
        for test in &self.tests {
            let left = test.left.literal(function, schema);
            let right = test.right.literal(function, schema);
            builder.emit_tagged_payload_equality_i32(
                test.left.value(&values, left.as_ref()),
                test.right.value(&values, right.as_ref()),
                function,
            )?;
            let result = schema.reserve_i32_local(function);
            result.store(function);
            tests.push(result);
            for literal in [right, left].into_iter().flatten() {
                literal.clear(function);
            }
        }
        for value in values.into_iter().rev() {
            value.clear(function);
        }
        Ok(CapturedSnapshot { numbers, tests })
    }
}

/// The entry values both loop paths may read until release.
pub(super) struct CapturedSnapshot {
    numbers: Vec<Option<F64Local>>,
    tests: Vec<I32Local>,
}

impl CapturedSnapshot {
    pub(super) fn load_number(&self, index: usize, function: &mut Function) {
        self.numbers[index]
            .expect("a Number use admitted its snapshot")
            .load(function);
    }

    pub(super) fn load_test(&self, index: usize, function: &mut Function) {
        self.tests[index].load(function);
    }

    pub(super) fn release(self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let schema = builder.runtime_schema();
        for test in self.tests.into_iter().rev() {
            schema.release_i32_local(test, function);
        }
        for number in self.numbers.into_iter().rev().flatten() {
            schema.release_f64_local(number, function);
        }
    }
}
