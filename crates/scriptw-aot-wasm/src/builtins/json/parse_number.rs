//! JSON Numbers: the scanner accumulates the decimal significand and exponent
//! while it validates the token, so a short token needs no String.
//!
//! The value is RoundMVResult of the token's mathematical value. When the
//! significand `w` is at most 2^53 and the decimal exponent `e` lies in
//! -22..=22, `w` and `10^|e|` are both exact binary64 values, so one IEEE
//! multiplication or division (round to nearest, ties to even) yields the
//! correctly rounded result. A zero significand is zero for any exponent.
//! Every other token keeps the exact decimal converter.
use super::parse_cursor::{json_cursor_digit, JsonCursor};
use super::*;

/// Every significand below this bound accepts one more digit without i64
/// overflow; a larger significand already exceeds 2^53.
const SIGNIFICAND_DIGIT_BOUND: i64 = 100_000_000_000_000_000;
/// An exponent at this magnitude cannot reach the -22..=22 window with a
/// significand of at most 2^53; accumulation stops there.
const EXPONENT_DIGIT_BOUND: i64 = 10_000;
const EXACT_SIGNIFICAND_LIMIT: i64 = 1 << 53;
const EXACT_POWER_OF_TEN_LIMIT: i64 = 22;

/// The three digit sequences of the JSON number grammar.
#[derive(Clone, Copy)]
enum JsonDigits {
    Integer,
    Fraction,
    Exponent,
}

struct JsonNumberLocals {
    significand: I64Local,
    /// Fraction digits folded into the significand.
    scale: I64Local,
    exponent: I64Local,
    /// A digit the exact window cannot represent was seen.
    inexact: I32Local,
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_json_parse_number(
        &mut self,
        cursor: &JsonCursor<'_>,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let start = s.reserve_i32_local(f);
        let unit = s.reserve_i32_local(f);
        let negative = s.reserve_i32_local(f);
        let number = JsonNumberLocals {
            significand: s.reserve_i64_local(f),
            scale: s.reserve_i64_local(f),
            exponent: s.reserve_i64_local(f),
            inexact: s.reserve_i32_local(f),
        };
        cursor.at.load(f);
        start.store(f);
        negative.set_constant(0, f);
        number.significand.set_constant(0, f);
        number.inexact.set_constant(0, f);
        cursor.peek(unit, s, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(45));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        negative.set_constant(1, f);
        cursor.advance(f);
        cursor.peek(unit, s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        unit.load(f);
        f.instruction(&Instruction::I32Const(48));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        cursor.advance(f);
        cursor.peek(unit, s, f);
        f.instruction(&Instruction::Else);
        self.emit_json_number_digits(
            cursor,
            unit,
            &number,
            JsonDigits::Integer,
            realm,
            message,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        // Fast path for exact plain integers (no `.` or `e`/`E`, exact within 2^53).
        // Both integer arms above leave `unit == cursor.peek()`.
        let bits = s.reserve_i64_local(f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(46));
        f.instruction(&Instruction::I32Ne);
        unit.load(f);
        f.instruction(&Instruction::I32Const(32));
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::I32Const(101));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        number.inexact.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        number.significand.load(f);
        f.instruction(&Instruction::I64Const(EXACT_SIGNIFICAND_LIMIT));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let value = s.reserve_f64_local(f);
        number.significand.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        value.store(f);
        negative.load(f);
        self.open_frame(ControlFrameKind::If, f);
        value.load(f);
        f.instruction(&Instruction::F64Neg);
        value.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.load(f);
        f.instruction(&Instruction::I64ReinterpretF64);
        bits.store(f);
        s.release_f64_local(value, f);
        f.instruction(&Instruction::Else);
        let exponent_negative = s.reserve_i32_local(f);
        exponent_negative.set_constant(0, f);
        number.scale.set_constant(0, f);
        number.exponent.set_constant(0, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(46));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        cursor.advance(f);
        self.emit_json_number_digits(
            cursor,
            unit,
            &number,
            JsonDigits::Fraction,
            realm,
            message,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        unit.load(f);
        f.instruction(&Instruction::I32Const(32));
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::I32Const(101));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        cursor.advance(f);
        cursor.peek(unit, s, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(43));
        f.instruction(&Instruction::I32Eq);
        unit.load(f);
        f.instruction(&Instruction::I32Const(45));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(45));
        f.instruction(&Instruction::I32Eq);
        exponent_negative.store(f);
        cursor.advance(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_json_number_digits(
            cursor,
            unit,
            &number,
            JsonDigits::Exponent,
            realm,
            message,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        // e = ±exponent - scale. Fraction digits each lower it by one.
        let decimal_exponent = s.reserve_i64_local(f);
        exponent_negative.load(f);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(0));
        number.exponent.load(f);
        f.instruction(&Instruction::I64Sub);
        decimal_exponent.store(f);
        f.instruction(&Instruction::Else);
        number.exponent.load(f);
        decimal_exponent.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        decimal_exponent.load(f);
        number.scale.load(f);
        f.instruction(&Instruction::I64Sub);
        decimal_exponent.store(f);
        let exact = s.reserve_i32_local(f);
        number.significand.load(f);
        f.instruction(&Instruction::I64Eqz);
        number.inexact.load(f);
        f.instruction(&Instruction::I32Eqz);
        number.significand.load(f);
        f.instruction(&Instruction::I64Const(EXACT_SIGNIFICAND_LIMIT));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        decimal_exponent.load(f);
        f.instruction(&Instruction::I64Const(EXACT_POWER_OF_TEN_LIMIT));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64Const(2 * EXACT_POWER_OF_TEN_LIMIT));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Or);
        exact.store(f);
        exact.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_exact_number(&number, decimal_exponent, negative, bits, f);
        f.instruction(&Instruction::Else);
        // The token is a validated StrDecimalLiteral with its sign, the
        // span StringToNumber hands the same exact converter.
        let token = s
            .reserve_gc_local(f)
            .initialize(cursor.slice_string(start, cursor.at, self, f)?, f);
        s.call_helper(
            crate::runtime_helpers::DecimalToBinary64Arguments::new(&token),
            self.runtime_helper_base()?,
            f,
        )
        .store(bits, f);
        token.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(exact, f);
        s.release_i64_local(decimal_exponent, f);
        s.release_i32_local(exponent_negative, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        out.set_number(bits, f);
        s.release_i64_local(bits, f);
        s.release_i32_local(number.inexact, f);
        s.release_i64_local(number.exponent, f);
        s.release_i64_local(number.scale, f);
        s.release_i64_local(number.significand, f);
        s.release_i32_local(negative, f);
        s.release_i32_local(unit, f);
        s.release_i32_local(start, f);
        Ok(())
    }

    /// One required digit, then every following one, folded into `number`.
    #[allow(clippy::too_many_arguments)]
    fn emit_json_number_digits(
        &mut self,
        cursor: &JsonCursor<'_>,
        unit: I32Local,
        number: &JsonNumberLocals,
        digits: JsonDigits,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        cursor.peek(unit, s, f);
        json_cursor_digit(unit, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_bad_text(realm, message, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let (accumulator, bound) = match digits {
            JsonDigits::Integer | JsonDigits::Fraction => {
                (number.significand, SIGNIFICAND_DIGIT_BOUND)
            }
            JsonDigits::Exponent => (number.exponent, EXPONENT_DIGIT_BOUND),
        };
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        json_cursor_digit(unit, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, f);
        accumulator.load(f);
        f.instruction(&Instruction::I64Const(bound));
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        accumulator.load(f);
        f.instruction(&Instruction::I64Const(10));
        f.instruction(&Instruction::I64Mul);
        unit.load(f);
        f.instruction(&Instruction::I32Const(48));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Add);
        accumulator.store(f);
        if matches!(digits, JsonDigits::Fraction) {
            number.scale.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Add);
            number.scale.store(f);
        }
        f.instruction(&Instruction::Else);
        // A dropped significand digit changes the value's scale or its
        // trailing digits; a saturated exponent leaves the exact window.
        number.inexact.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.advance(f);
        cursor.peek(unit, s, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        Ok(())
    }

    /// `±w × 10^e` or `±w / 10^-e` for an exact `w` and `|e| <= 22`; powers of
    /// ten up to 10^22 are exact, so the running product never rounds.
    fn emit_json_exact_number(
        &mut self,
        number: &JsonNumberLocals,
        decimal_exponent: I64Local,
        negative: I32Local,
        bits: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let power = s.reserve_f64_local(f);
        let remaining = s.reserve_i64_local(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
        power.store(f);
        // A zero significand may carry any exponent; it needs no power.
        number.significand.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        remaining.set_constant(0, f);
        f.instruction(&Instruction::Else);
        decimal_exponent.load(f);
        f.instruction(&Instruction::I64Const(0));
        decimal_exponent.load(f);
        f.instruction(&Instruction::I64Sub);
        decimal_exponent.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64GeS);
        f.instruction(&Instruction::Select);
        remaining.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        remaining.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(done, f);
        power.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(10.0)));
        f.instruction(&Instruction::F64Mul);
        power.store(f);
        remaining.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        remaining.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let value = s.reserve_f64_local(f);
        number.significand.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        value.store(f);
        decimal_exponent.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, f);
        value.load(f);
        power.load(f);
        f.instruction(&Instruction::F64Div);
        value.store(f);
        f.instruction(&Instruction::Else);
        value.load(f);
        power.load(f);
        f.instruction(&Instruction::F64Mul);
        value.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // The sign applies last, so "-0" and "-0.0e5" are -0.
        negative.load(f);
        self.open_frame(ControlFrameKind::If, f);
        value.load(f);
        f.instruction(&Instruction::F64Neg);
        value.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.load(f);
        f.instruction(&Instruction::I64ReinterpretF64);
        bits.store(f);
        s.release_f64_local(value, f);
        s.release_i64_local(remaining, f);
        s.release_f64_local(power, f);
    }
}
