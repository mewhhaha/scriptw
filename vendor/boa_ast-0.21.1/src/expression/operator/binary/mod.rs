//! Binary expression nodes.
//!
//! A Binary expression comprises any operation between two expressions (excluding assignments),
//! such as:
//! - [Logic operations][logic] (`||`, `&&`).
//! - [Relational math][relat] (`==`, `<`).
//! - [Bit manipulation][bit] (`^`, `|`).
//! - [Arithmetic][arith] (`+`, `%`).
//! - The [comma operator][comma] (`,`)
//!
//! [logic]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators#binary_logical_operators
//! [relat]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators#relational_operators
//! [bit]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators#binary_bitwise_operators
//! [arith]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators#arithmetic_operators
//! [comma]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Comma_Operator

mod op;

use crate::{
    Span, Spanned,
    expression::{Expression, This},
    function::PrivateName,
    visitor::{VisitWith, Visitor, VisitorMut},
};
use boa_interner::{Interner, ToInternedString};
use core::ops::ControlFlow;

pub use op::*;

/// Binary operations require two operands, one before the operator and one after the operator.
///
/// See the [module level documentation][self] for more information.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[derive(Debug, PartialEq)]
pub struct Binary {
    op: BinaryOp,
    lhs: Box<Expression>,
    rhs: Box<Expression>,
}

impl Binary {
    /// Creates a `BinOp` AST Expression.
    #[inline]
    #[must_use]
    pub fn new(op: BinaryOp, lhs: Expression, rhs: Expression) -> Self {
        Self {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    /// Gets the binary operation of the Expression.
    #[inline]
    #[must_use]
    pub const fn op(&self) -> BinaryOp {
        self.op
    }

    /// Gets the left hand side of the binary operation.
    #[inline]
    #[must_use]
    pub const fn lhs(&self) -> &Expression {
        &self.lhs
    }

    /// Gets the right hand side of the binary operation.
    #[inline]
    #[must_use]
    pub const fn rhs(&self) -> &Expression {
        &self.rhs
    }

    /// Gets the left hand side of the binary operation.
    #[inline]
    #[must_use]
    pub fn lhs_mut(&mut self) -> &mut Expression {
        &mut self.lhs
    }

    /// Gets the right hand side of the binary operation.
    #[inline]
    #[must_use]
    pub fn rhs_mut(&mut self) -> &mut Expression {
        &mut self.rhs
    }
}

impl Spanned for Binary {
    #[inline]
    fn span(&self) -> Span {
        // Scriptw: walk a left-deep chain down to its first operand instead
        // of recursing once per operator.
        let mut first = &self.lhs;
        while let Expression::Binary(binary) = &**first {
            first = &binary.lhs;
        }
        Span::new(first.span().start(), self.rhs.span().end())
    }
}

/// Scriptw: clones a left-deep chain from its first operator outwards, so
/// cloning a chain of any length does not recurse once per operator.
impl Clone for Binary {
    fn clone(&self) -> Self {
        let mut outer = Vec::new();
        let mut node = self;
        while let Expression::Binary(lhs) = &*node.lhs {
            outer.push(node);
            node = lhs;
        }
        let mut copy = Self {
            op: node.op,
            lhs: node.lhs.clone(),
            rhs: node.rhs.clone(),
        };
        for node in outer.into_iter().rev() {
            copy = Self {
                op: node.op,
                lhs: Box::new(Expression::Binary(copy)),
                rhs: node.rhs.clone(),
            };
        }
        copy
    }
}

/// Scriptw: drops binary operands that are themselves binary expressions
/// from an explicit stack, so dropping a chain of any length (left-deep from
/// `a + b + c`, or right-deep) does not recurse once per operator.
impl Drop for Binary {
    fn drop(&mut self) {
        fn detach(operand: &mut Expression, pending: &mut Vec<Binary>) {
            if matches!(operand, Expression::Binary(_)) {
                let placeholder = Expression::This(This::new(Span::EMPTY));
                if let Expression::Binary(binary) = core::mem::replace(operand, placeholder) {
                    pending.push(binary);
                }
            }
        }
        if !matches!(*self.lhs, Expression::Binary(_))
            && !matches!(*self.rhs, Expression::Binary(_))
        {
            return;
        }
        let mut pending = Vec::new();
        detach(&mut self.lhs, &mut pending);
        detach(&mut self.rhs, &mut pending);
        while let Some(mut binary) = pending.pop() {
            detach(&mut binary.lhs, &mut pending);
            detach(&mut binary.rhs, &mut pending);
        }
    }
}

impl ToInternedString for Binary {
    #[inline]
    fn to_interned_string(&self, interner: &Interner) -> String {
        format!(
            "{} {} {}",
            self.lhs.to_interned_string(interner),
            self.op,
            self.rhs.to_interned_string(interner)
        )
    }
}

impl From<Binary> for Expression {
    #[inline]
    fn from(op: Binary) -> Self {
        Self::Binary(op)
    }
}

impl VisitWith for Binary {
    fn visit_with<'a, V>(&'a self, visitor: &mut V) -> ControlFlow<V::BreakTy>
    where
        V: Visitor<'a>,
    {
        if !V::VISITS_BINARY_CHAINS_ITERATIVELY {
            visitor.visit_expression(&self.lhs)?;
            return visitor.visit_expression(&self.rhs);
        }
        // Scriptw: visits the operands of a left-deep chain in source order
        // without recursing once per operator (see the trait constant).
        let mut right_operands = Vec::new();
        let mut node = self;
        let first = loop {
            right_operands.push(&*node.rhs);
            match &*node.lhs {
                Expression::Binary(lhs) => node = lhs,
                first => break first,
            }
        };
        visitor.visit_expression(first)?;
        for operand in right_operands.into_iter().rev() {
            visitor.visit_expression(operand)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_with_mut<'a, V>(&'a mut self, visitor: &mut V) -> ControlFlow<V::BreakTy>
    where
        V: VisitorMut<'a>,
    {
        if !V::VISITS_BINARY_CHAINS_ITERATIVELY {
            visitor.visit_expression_mut(&mut self.lhs)?;
            return visitor.visit_expression_mut(&mut self.rhs);
        }
        let mut right_operands = Vec::new();
        let mut node = self;
        let first = loop {
            let Self { lhs, rhs, .. } = node;
            right_operands.push(&mut **rhs);
            match &mut **lhs {
                Expression::Binary(lhs) => node = lhs,
                first => break first,
            }
        };
        visitor.visit_expression_mut(first)?;
        for operand in right_operands.into_iter().rev() {
            visitor.visit_expression_mut(operand)?;
        }
        ControlFlow::Continue(())
    }
}

/// Binary [relational][relat] `In` expression with a private name on the left hand side.
///
/// Because the left hand side must be a private name, this is a separate type from [`Binary`].
///
/// [relat]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators#relational_operators
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[derive(Clone, Debug, PartialEq)]
pub struct BinaryInPrivate {
    lhs: PrivateName,
    rhs: Box<Expression>,
}

impl BinaryInPrivate {
    /// Creates a `BinaryInPrivate` AST Expression.
    #[inline]
    #[must_use]
    pub fn new(lhs: PrivateName, rhs: Expression) -> Self {
        Self {
            lhs,
            rhs: Box::new(rhs),
        }
    }

    /// Gets the left hand side of the binary operation.
    #[inline]
    #[must_use]
    pub const fn lhs(&self) -> &PrivateName {
        &self.lhs
    }

    /// Gets the right hand side of the binary operation.
    #[inline]
    #[must_use]
    pub const fn rhs(&self) -> &Expression {
        &self.rhs
    }
}

impl Spanned for BinaryInPrivate {
    #[inline]
    fn span(&self) -> Span {
        Span::new(self.lhs.span().start(), self.rhs.span().end())
    }
}

impl ToInternedString for BinaryInPrivate {
    #[inline]
    fn to_interned_string(&self, interner: &Interner) -> String {
        format!(
            "#{} in {}",
            interner.resolve_expect(self.lhs.description()),
            self.rhs.to_interned_string(interner)
        )
    }
}

impl From<BinaryInPrivate> for Expression {
    #[inline]
    fn from(op: BinaryInPrivate) -> Self {
        Self::BinaryInPrivate(op)
    }
}

impl VisitWith for BinaryInPrivate {
    fn visit_with<'a, V>(&'a self, visitor: &mut V) -> ControlFlow<V::BreakTy>
    where
        V: Visitor<'a>,
    {
        visitor.visit_private_name(&self.lhs)?;
        visitor.visit_expression(&self.rhs)
    }

    fn visit_with_mut<'a, V>(&'a mut self, visitor: &mut V) -> ControlFlow<V::BreakTy>
    where
        V: VisitorMut<'a>,
    {
        visitor.visit_private_name_mut(&mut self.lhs)?;
        visitor.visit_expression_mut(&mut self.rhs)
    }
}
