use super::finite_function_source::{bounded_candidates, MAX_SOURCE_CANDIDATES};
use super::*;

#[derive(Default)]
struct SourceReturns<'ast> {
    expressions: Vec<&'ast Expression>,
}

impl<'ast> Visitor<'ast> for SourceReturns<'ast> {
    type BreakTy = ();

    fn visit_return(&mut self, statement: &'ast AstReturn) -> ControlFlow<Self::BreakTy> {
        if let Some(expression) = statement.target() {
            self.expressions.push(expression);
        }
        ControlFlow::Continue(())
    }

    fn visit_function_body(&mut self, _: &'ast FunctionBody) -> ControlFlow<Self::BreakTy> {
        // A returned function contributes its own return expressions only when
        // the iterator protocol reaches that function's call position.
        ControlFlow::Continue(())
    }
}

fn property_candidates(candidates: Vec<FiniteSourceValue>, name: &str) -> Vec<FiniteSourceValue> {
    bounded_candidates(
        candidates
            .into_iter()
            .flat_map(|candidate| match candidate {
                FiniteSourceValue::Record(mut properties) => {
                    properties.remove(name).unwrap_or_default()
                }
                FiniteSourceValue::Text(_)
                | FiniteSourceValue::SupplementalText(_)
                | FiniteSourceValue::ReturnedCallableSource(_)
                | FiniteSourceValue::Function(_)
                | FiniteSourceValue::FunctionConstructor(_)
                | FiniteSourceValue::Array(_) => Vec::new(),
            }),
    )
}

impl ScriptLowerer<'_> {
    pub(super) fn finite_source_computed_key_candidate(
        &self,
        expression: &Expression,
    ) -> Option<String> {
        let candidate = match Self::unwrap_parenthesized_expr(expression) {
            Expression::PropertyAccess(PropertyAccess::Simple(access)) => {
                match (
                    Self::unwrap_parenthesized_expr(access.target()),
                    access.field(),
                ) {
                    (Expression::Identifier(target), PropertyAccessField::Const(member))
                        if self.interner.resolve_expect(target.sym()).to_string()
                            == SYMBOL_NAME =>
                    {
                        let member = self.interner.resolve_expect(member.sym()).to_string();
                        WellKnownSymbol::from_member_name(SymbolMemberName::new(&member))
                            .map(shape_namespace_key)
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        // Source preparation may retain this possible key after an observable
        // Get erased intrinsic identity proof. It never replaces the key's
        // runtime evaluation or proves that the iterator uses this property.
        candidate
            .or_else(|| self.aot_source_text(expression))
            .or_else(|| {
                let Expression::Identifier(identifier) =
                    Self::unwrap_parenthesized_expr(expression)
                else {
                    return None;
                };
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                let mut keys = self
                    .finite_binding_source_candidates(&name)?
                    .iter()
                    .filter_map(FiniteSourceValue::text);
                let key = keys.next()?;
                // This single-key interface must not arbitrarily select one
                // spelling from a binding with several retained text candidates.
                keys.next().is_none().then(|| key.to_owned())
            })
    }

    pub(super) fn register_finite_source_property_assignment(
        &mut self,
        access: &boa_ast::expression::access::SimplePropertyAccess,
        value: &Expression,
    ) {
        let Expression::Identifier(identifier) = Self::unwrap_parenthesized_expr(access.target())
        else {
            return;
        };
        let name = self.interner.resolve_expect(identifier.sym()).to_string();
        let Some(binding_key) = self.finite_source_binding_key(&name) else {
            return;
        };
        let key = match access.field() {
            PropertyAccessField::Const(identifier) => {
                Some(self.interner.resolve_expect(identifier.sym()).to_string())
            }
            PropertyAccessField::Expr(expression) => {
                self.finite_source_computed_key_candidate(expression)
            }
        };
        let Some(key) = key else { return };
        let candidates = self.function_source_value_candidates(value);
        if candidates.is_empty() {
            return;
        }
        let properties = self
            .function_source_binding_candidates
            .entry(binding_key)
            .or_default()
            .make_mut();
        if let Some(FiniteSourceValue::Record(properties)) = properties
            .iter_mut()
            .find(|candidate| matches!(candidate, FiniteSourceValue::Record(_)))
        {
            let previous = properties.remove(&key).unwrap_or_default();
            properties.insert(
                key,
                bounded_candidates(previous.into_iter().chain(candidates)),
            );
        } else if properties.len() < MAX_SOURCE_CANDIDATES {
            properties.push(FiniteSourceValue::Record(BTreeMap::from([(
                key, candidates,
            )])));
        }
    }

    pub(super) fn finite_source_call_returns(
        &self,
        callees: Vec<FiniteSourceValue>,
    ) -> Vec<FiniteSourceValue> {
        bounded_candidates(callees.into_iter().flat_map(|callee| {
            let FiniteSourceValue::Function(function_id) = callee else {
                return Vec::new();
            };
            let Some(plan) = self.analysis.function_plans.get(&function_id) else {
                return Vec::new();
            };
            let mut returns = SourceReturns::default();
            for statement in plan.body.statements() {
                let _ = statement.visit_with(&mut returns);
            }
            returns
                .expressions
                .into_iter()
                .flat_map(|expression| self.function_source_value_candidates(expression))
                .collect::<Vec<_>>()
        }))
    }

    pub(super) fn finite_spread_source_candidates(
        &self,
        expression: &Expression,
    ) -> Vec<FiniteSourceValue> {
        let candidates = self.function_source_value_candidates(expression);
        let array_elements = candidates.iter().flat_map(|candidate| match candidate {
            FiniteSourceValue::Array(elements) => elements.clone(),
            FiniteSourceValue::Text(_)
            | FiniteSourceValue::SupplementalText(_)
            | FiniteSourceValue::ReturnedCallableSource(_)
            | FiniteSourceValue::Function(_)
            | FiniteSourceValue::FunctionConstructor(_)
            | FiniteSourceValue::Record(_) => Vec::new(),
        });
        let iterator_methods = property_candidates(
            candidates.clone(),
            &shape_namespace_key(WellKnownSymbol::Iterator),
        );
        let iterators = self.finite_source_call_returns(iterator_methods);
        let next_methods = property_candidates(iterators, "next");
        let results = self.finite_source_call_returns(next_methods);
        // This follows only the two finite source call positions of the
        // iterator protocol. It does not execute either function, interpret
        // `done`, or prove which value will be the first collected argument.
        bounded_candidates(array_elements.chain(property_candidates(results, "value")))
    }
}
