//! Tests of one `i32` against a run of consecutive constants.
//!
//! The emitter asks whether a tag is one of several by or-ing equality
//! tests:
//!
//! ```text
//! i32.const 0
//! local.get $tag  i32.const 6  i32.eq  i32.or
//! local.get $tag  i32.const 7  i32.eq  i32.or
//! local.get $tag  i32.const 8  i32.eq  i32.or
//! local.get $tag  i32.const 9  i32.eq  i32.or
//! ```
//!
//! and whether it is none of several by and-ing inequality tests. Cranelift
//! compiles each test and each `or` as written. When the constants are the
//! integers from `low` to `low + count - 1`, the chain is one unsigned
//! comparison instead:
//!
//! ```text
//! tag == low || ... || tag == low + count - 1   <=>   tag - low <u count
//! tag != low && ... && tag != low + count - 1   <=>   tag - low >=u count
//! ```
//!
//! with the subtraction wrapping at 2^32, which holds for every `i32`: the
//! subtraction maps exactly the `count` listed values to `0..count`. Both
//! sides are 0 or 1.
//!
//! [`SetTests`] remembers, for the CLIF value of each `i32.eq`, `i32.ne` and
//! `i32.eqz` with a constant operand and of each `i32.or` and `i32.and` of
//! such values, which test of which value it is. An `i32.or` of two
//! membership tests of one value, or an `i32.and` of two non-membership
//! tests, is the test of the union of their constants; when the union is
//! such a run, the comparison above is emitted for it instead of the `bor`
//! or `band`. What is remembered is a property of the values, which are
//! pure functions of the tested value, so it holds wherever they are in
//! scope. The tests that a chain no longer uses are left to Cranelift's
//! dead-code removal.

use cranelift_codegen::ir::{self, InstBuilder, condcodes::IntCC, types::I32};
use cranelift_frontend::FunctionBuilder;
use smallvec::SmallVec;
use std::collections::HashMap;

/// What a truth value says of `subject`.
#[derive(Clone, PartialEq, Eq, Debug)]
struct SetTest {
    subject: ir::Value,
    /// Distinct constants in ascending order.
    constants: SmallVec<[u32; 4]>,
    /// Whether the truth value is one exactly when `subject` is one of
    /// `constants`; otherwise it is one exactly when `subject` is none of
    /// them.
    member: bool,
}

/// The set tests among the `i32` values of the function being translated.
#[derive(Default)]
pub(crate) struct SetTests {
    tests: HashMap<ir::Value, SetTest>,
}

/// The constant `value` is, when an `iconst` defines it.
fn constant(func: &ir::Function, value: ir::Value) -> Option<u32> {
    let inst = func.dfg.value_def(value).inst()?;
    match func.dfg.insts[inst] {
        ir::InstructionData::UnaryImm {
            opcode: ir::Opcode::Iconst,
            imm,
        } => Some(imm.bits() as u32),
        _ => None,
    }
}

impl SetTests {
    /// Record that `result` is `left == right` (`equal`) or `left != right`
    /// of two `i32` values.
    pub(crate) fn compared(
        &mut self,
        func: &ir::Function,
        result: ir::Value,
        left: ir::Value,
        right: ir::Value,
        equal: bool,
    ) {
        let (subject, constant) = match (constant(func, left), constant(func, right)) {
            (None, Some(constant)) => (left, constant),
            (Some(constant), None) => (right, constant),
            _ => return,
        };
        self.tests.insert(
            result,
            SetTest {
                subject,
                constants: SmallVec::from_slice(&[constant]),
                member: equal,
            },
        );
    }

    /// Record that `result` is `subject == 0` of an `i32`.
    pub(crate) fn zero_tested(&mut self, result: ir::Value, subject: ir::Value) {
        self.tests.insert(
            result,
            SetTest {
                subject,
                constants: SmallVec::from_slice(&[0]),
                member: true,
            },
        );
    }

    /// Translate `i32.or`.
    pub(crate) fn or(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        left: ir::Value,
        right: ir::Value,
    ) -> ir::Value {
        // A chain starts from zero: `0 | x` is `x`, with what is known of it.
        match (constant(builder.func, left), constant(builder.func, right)) {
            (Some(0), _) => return right,
            (_, Some(0)) => return left,
            _ => {}
        }
        self.combined(builder, left, right, true)
            .unwrap_or_else(|| builder.ins().bor(left, right))
    }

    /// Translate `i32.and`.
    pub(crate) fn and(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        left: ir::Value,
        right: ir::Value,
    ) -> ir::Value {
        // A chain starts from one: a test is 0 or 1, so `1 & test` is the
        // test.
        match (constant(builder.func, left), constant(builder.func, right)) {
            (Some(1), _) if self.tests.contains_key(&right) => return right,
            (_, Some(1)) if self.tests.contains_key(&left) => return left,
            _ => {}
        }
        self.combined(builder, left, right, false)
            .unwrap_or_else(|| builder.ins().band(left, right))
    }

    /// The value of `left | right` (`member`) or `left & right` when both
    /// are tests of that kind of one value: the comparison for a run of
    /// constants, or the plain operation, remembered as the test of the
    /// union. `None` when they are not.
    fn combined(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        left: ir::Value,
        right: ir::Value,
        member: bool,
    ) -> Option<ir::Value> {
        let (first, second) = (self.tests.get(&left)?, self.tests.get(&right)?);
        if first.subject != second.subject || first.member != member || second.member != member {
            return None;
        }
        let subject = first.subject;
        let mut constants = first.constants.clone();
        constants.extend(second.constants.iter().copied());
        constants.sort_unstable();
        constants.dedup();

        let low = constants[0];
        let count = constants.len() as u32;
        // Distinct and ascending, so a run exactly when the ends are `count -
        // 1` apart; a test of a single constant stays the test it was.
        let run = count >= 2 && constants[constants.len() - 1] - low == count - 1;
        let value = if run {
            let offset = match low {
                0 => subject,
                low => {
                    let negated = builder.ins().iconst(I32, i64::from(low.wrapping_neg()));
                    builder.ins().iadd(subject, negated)
                }
            };
            let count = builder.ins().iconst(I32, i64::from(count));
            let condition = if member {
                IntCC::UnsignedLessThan
            } else {
                IntCC::UnsignedGreaterThanOrEqual
            };
            let inside = builder.ins().icmp(condition, offset, count);
            builder.ins().uextend(I32, inside)
        } else if member {
            builder.ins().bor(left, right)
        } else {
            builder.ins().band(left, right)
        };
        self.tests.insert(
            value,
            SetTest {
                subject,
                constants,
                member,
            },
        );
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::{AbiParam, Function, Signature, UserFuncName};
    use cranelift_codegen::isa::{CallConv, TargetFrontendConfig};
    use cranelift_codegen::settings;
    use cranelift_codegen::verifier::verify_function;
    use cranelift_frontend::FunctionBuilderContext;

    const FRONTEND: TargetFrontendConfig = TargetFrontendConfig {
        default_call_conv: CallConv::SystemV,
        pointer_width: target_lexicon::PointerWidth::U64,
        page_size_align_log2: 12,
    };

    /// What a chain compiled to.
    #[derive(Debug, PartialEq, Eq)]
    enum Shape {
        /// `subject + addend <cc> count`, zero-extended.
        Range {
            subject: ir::Value,
            addend: u32,
            condition: IntCC,
            count: u32,
        },
        /// A `bor` or `band`.
        Plain(ir::Opcode),
        /// The equality test it started as.
        Compare,
    }

    /// Build a function `(i32 x, i32 y) -> i32` that returns what `chain`
    /// yields, and describe that value.
    fn shape(
        chain: impl FnOnce(&mut Chain<'_, '_>, ir::Value, ir::Value) -> ir::Value,
    ) -> (Shape, ir::Value) {
        let mut signature = Signature::new(CallConv::SystemV);
        signature.params.push(AbiParam::new(I32));
        signature.params.push(AbiParam::new(I32));
        signature.returns.push(AbiParam::new(I32));
        let mut func = Function::with_name_signature(UserFuncName::testcase("sets"), signature);
        let mut context = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let [x, y] = *builder.block_params(entry) else {
            unreachable!()
        };
        let mut tests = SetTests::default();
        let result = chain(
            &mut Chain {
                builder: &mut builder,
                tests: &mut tests,
            },
            x,
            y,
        );
        builder.ins().return_(&[result]);
        builder.finalize(FRONTEND);
        let flags = settings::Flags::new(settings::builder());
        if let Err(errors) = verify_function(&func, &flags) {
            panic!("{errors}\n{}", func.display());
        }

        let dfg = &func.dfg;
        let definition = |value: ir::Value| dfg.value_def(value).unwrap_inst();
        let shape = match dfg.insts[definition(result)] {
            ir::InstructionData::Binary { opcode, .. } => Shape::Plain(opcode),
            ir::InstructionData::Unary {
                opcode: ir::Opcode::Uextend,
                arg,
            } => match dfg.insts[definition(arg)] {
                ir::InstructionData::IntCompare {
                    cond,
                    args: [left, right],
                    ..
                } => match (constant(&func, left), constant(&func, right)) {
                    (None, Some(count)) if cond != IntCC::Equal && cond != IntCC::NotEqual => {
                        let (subject, addend) = match dfg.value_def(left).inst() {
                            Some(inst) => match dfg.insts[inst] {
                                ir::InstructionData::Binary {
                                    opcode: ir::Opcode::Iadd,
                                    args: [subject, addend],
                                } => (subject, constant(&func, addend).unwrap()),
                                _ => (left, 0),
                            },
                            None => (left, 0),
                        };
                        Shape::Range {
                            subject,
                            addend,
                            condition: cond,
                            count,
                        }
                    }
                    _ => Shape::Compare,
                },
                other => panic!("unexpected {other:?}"),
            },
            other => panic!("unexpected {other:?}"),
        };
        (shape, x)
    }

    struct Chain<'a, 'b> {
        builder: &'a mut FunctionBuilder<'b>,
        tests: &'a mut SetTests,
    }

    impl Chain<'_, '_> {
        /// `value == constant` or `value != constant`, as `i32.eq` and
        /// `i32.ne` are translated.
        fn compare(&mut self, value: ir::Value, constant: u32, equal: bool) -> ir::Value {
            let constant = self.builder.ins().iconst(I32, i64::from(constant));
            let condition = if equal { IntCC::Equal } else { IntCC::NotEqual };
            let compared = self.builder.ins().icmp(condition, value, constant);
            let result = self.builder.ins().uextend(I32, compared);
            self.tests
                .compared(self.builder.func, result, value, constant, equal);
            result
        }

        fn is(&mut self, value: ir::Value, constant: u32) -> ir::Value {
            self.compare(value, constant, true)
        }

        fn is_not(&mut self, value: ir::Value, constant: u32) -> ir::Value {
            self.compare(value, constant, false)
        }

        fn or(&mut self, left: ir::Value, right: ir::Value) -> ir::Value {
            self.tests.or(self.builder, left, right)
        }

        fn and(&mut self, left: ir::Value, right: ir::Value) -> ir::Value {
            self.tests.and(self.builder, left, right)
        }
    }

    #[test]
    fn a_chain_over_a_run_of_constants_is_one_comparison() {
        let (shape, x) = shape(|chain, x, _| {
            let mut any = chain.is(x, 6);
            for constant in 7..=9 {
                let next = chain.is(x, constant);
                any = chain.or(any, next);
            }
            any
        });
        assert_eq!(
            shape,
            Shape::Range {
                subject: x,
                addend: 6u32.wrapping_neg(),
                condition: IntCC::UnsignedLessThan,
                count: 4,
            }
        );
    }

    #[test]
    fn a_chain_may_start_from_its_identity() {
        let (any, x) = shape(|chain, x, _| {
            let mut any = chain.builder.ins().iconst(I32, 0);
            for constant in 6..=9 {
                let next = chain.is(x, constant);
                any = chain.or(any, next);
            }
            any
        });
        assert_eq!(
            any,
            Shape::Range {
                subject: x,
                addend: 6u32.wrapping_neg(),
                condition: IntCC::UnsignedLessThan,
                count: 4,
            }
        );
        let (none, x) = shape(|chain, x, _| {
            let mut none = chain.builder.ins().iconst(I32, 1);
            for constant in 2..=3 {
                let next = chain.is_not(x, constant);
                none = chain.and(next, none);
            }
            none
        });
        assert_eq!(
            none,
            Shape::Range {
                subject: x,
                addend: 2u32.wrapping_neg(),
                condition: IntCC::UnsignedGreaterThanOrEqual,
                count: 2,
            }
        );
        // `1 & y` of a value that is no test is not `y`.
        let (masked, _) = shape(|chain, _, y| {
            let one = chain.builder.ins().iconst(I32, 1);
            chain.and(one, y)
        });
        assert_eq!(masked, Shape::Plain(ir::Opcode::Band));
    }

    #[test]
    fn a_run_from_zero_needs_no_subtraction() {
        let (shape, x) = shape(|chain, x, _| {
            let zero = chain.is(x, 0);
            let one = chain.is(x, 1);
            chain.or(one, zero)
        });
        assert_eq!(
            shape,
            Shape::Range {
                subject: x,
                addend: 0,
                condition: IntCC::UnsignedLessThan,
                count: 2,
            }
        );
    }

    #[test]
    fn a_test_for_zero_is_a_test_of_the_constant() {
        let (shape, x) = shape(|chain, x, _| {
            // As `i32.eqz` is translated.
            let compared = chain.builder.ins().icmp_imm_s(IntCC::Equal, x, 0);
            let zero = chain.builder.ins().uextend(I32, compared);
            chain.tests.zero_tested(zero, x);
            let one = chain.is(x, 1);
            let two = chain.is(x, 2);
            let low = chain.or(zero, one);
            chain.or(low, two)
        });
        assert_eq!(
            shape,
            Shape::Range {
                subject: x,
                addend: 0,
                condition: IntCC::UnsignedLessThan,
                count: 3,
            }
        );
    }

    #[test]
    fn a_chain_of_inequalities_is_the_opposite_comparison() {
        let (shape, x) = shape(|chain, x, _| {
            let mut none = chain.is_not(x, 3);
            for constant in [5, 4] {
                let next = chain.is_not(x, constant);
                none = chain.and(none, next);
            }
            none
        });
        assert_eq!(
            shape,
            Shape::Range {
                subject: x,
                addend: 3u32.wrapping_neg(),
                condition: IntCC::UnsignedGreaterThanOrEqual,
                count: 3,
            }
        );
    }

    #[test]
    fn a_run_that_a_later_test_completes_is_found() {
        let (shape, x) = shape(|chain, x, _| {
            let low = chain.is(x, 1);
            let high = chain.is(x, 3);
            let apart = chain.or(low, high);
            let between = chain.is(x, 2);
            chain.or(apart, between)
        });
        assert_eq!(
            shape,
            Shape::Range {
                subject: x,
                addend: 1u32.wrapping_neg(),
                condition: IntCC::UnsignedLessThan,
                count: 3,
            }
        );
    }

    #[test]
    fn constants_are_a_run_as_unsigned_numbers_only() {
        // -2, -1 and 0 are consecutive as signed numbers; as unsigned ones
        // 0 is far from the other two, and the comparison is unsigned.
        let (around_zero, _) = shape(|chain, x, _| {
            let low = chain.is(x, 0xffff_fffe);
            let high = chain.is(x, 0);
            let apart = chain.or(low, high);
            let between = chain.is(x, 0xffff_ffff);
            chain.or(apart, between)
        });
        assert_eq!(around_zero, Shape::Plain(ir::Opcode::Bor));
        // The two largest values are a run of their own.
        let (largest, x) = shape(|chain, x, _| {
            let low = chain.is(x, 0xffff_fffe);
            let high = chain.is(x, 0xffff_ffff);
            chain.or(low, high)
        });
        assert_eq!(
            largest,
            Shape::Range {
                subject: x,
                addend: 2,
                condition: IntCC::UnsignedLessThan,
                count: 2,
            }
        );
    }

    #[test]
    fn other_combinations_stay_as_written() {
        // Constants with a gap.
        let (gap, _) = shape(|chain, x, _| {
            let one = chain.is(x, 1);
            let three = chain.is(x, 3);
            chain.or(one, three)
        });
        assert_eq!(gap, Shape::Plain(ir::Opcode::Bor));
        // Two values.
        let (subjects, _) = shape(|chain, x, y| {
            let first = chain.is(x, 1);
            let second = chain.is(y, 2);
            chain.or(first, second)
        });
        assert_eq!(subjects, Shape::Plain(ir::Opcode::Bor));
        // An `or` of inequalities and an `and` of equalities are no set
        // tests.
        let (mixed, _) = shape(|chain, x, _| {
            let first = chain.is_not(x, 1);
            let second = chain.is_not(x, 2);
            chain.or(first, second)
        });
        assert_eq!(mixed, Shape::Plain(ir::Opcode::Bor));
        let (both, _) = shape(|chain, x, _| {
            let first = chain.is(x, 1);
            let second = chain.is(x, 2);
            chain.and(first, second)
        });
        assert_eq!(both, Shape::Plain(ir::Opcode::Band));
        // The same constant twice is the one test.
        let (twice, _) = shape(|chain, x, _| {
            let first = chain.is(x, 1);
            let second = chain.is(x, 1);
            chain.or(first, second)
        });
        assert_eq!(twice, Shape::Plain(ir::Opcode::Bor));
        // A value that is not a test.
        let (plain, _) = shape(|chain, x, y| {
            let first = chain.is(x, 1);
            chain.or(first, y)
        });
        assert_eq!(plain, Shape::Plain(ir::Opcode::Bor));
    }
}
