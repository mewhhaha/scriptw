use super::finite_function_source::{bounded_candidates, MAX_SOURCE_CANDIDATES};
use super::*;

// These bounds limit optional discovery only. Existing source tuples remain
// intact when any hint budget is exceeded.
const MAX_BODY_HINT_BYTES: usize = 64 * 1024;

enum BodyHintRefusal {
    ReferenceCapacity,
    PossibleNestedSourceOrHostRequest,
}

struct BodyReferences<'syntax> {
    interner: &'syntax Interner,
    names: BTreeSet<String>,
}

impl BodyReferences<'_> {
    fn record(
        &mut self,
        identifier: &boa_ast::expression::Identifier,
    ) -> ControlFlow<BodyHintRefusal> {
        self.names
            .insert(self.interner.resolve_expect(identifier.sym()).to_string());
        if self.names.len() > MAX_SOURCE_CANDIDATES {
            ControlFlow::Break(BodyHintRefusal::ReferenceCapacity)
        } else {
            ControlFlow::Continue(())
        }
    }
}

impl<'ast> Visitor<'ast> for BodyReferences<'_> {
    type BreakTy = BodyHintRefusal;

    fn visit_expression(&mut self, expression: &'ast Expression) -> ControlFlow<Self::BreakTy> {
        // Preparing a guessed tuple must not discover additional nested source
        // or host resources. Existing tuples can still admit these bodies.
        if matches!(
            expression,
            Expression::Call(_)
                | Expression::New(_)
                | Expression::ImportCall(_)
                | Expression::TaggedTemplate(_)
        ) || matches!(expression, Expression::Optional(optional) if optional.chain().iter().any(|operation| matches!(operation.kind(), OptionalOperationKind::Call { .. })))
        {
            return ControlFlow::Break(BodyHintRefusal::PossibleNestedSourceOrHostRequest);
        }
        if let Expression::Identifier(identifier) = expression {
            self.record(identifier)?;
        }
        expression.visit_with(self)
    }

    fn visit_property_definition(
        &mut self,
        property: &'ast PropertyDefinition,
    ) -> ControlFlow<Self::BreakTy> {
        if let PropertyDefinition::IdentifierReference(identifier) = property {
            self.record(identifier)?;
        }
        property.visit_with(self)
    }
}

impl ScriptLowerer<'_> {
    fn parameter_conversion_needs_body_hints(&self, expression: &Expression) -> bool {
        if super::binary_chain_longer_than(expression, super::MAX_SOURCE_CANDIDATE_CHAIN) {
            return false;
        }
        match Self::unwrap_parenthesized_expr(expression) {
            Expression::Binary(binary)
                if binary.op() == BinaryOp::Arithmetic(ArithmeticOp::Add) =>
            {
                self.parameter_conversion_needs_body_hints(binary.lhs())
                    || self.parameter_conversion_needs_body_hints(binary.rhs())
            }
            expression => {
                let candidates = self.function_source_value_candidates(expression);
                !candidates
                    .iter()
                    .any(|candidate| candidate.primary_text().is_some())
                    && candidates
                        .iter()
                        .any(|candidate| matches!(candidate, FiniteSourceValue::Record(_)))
            }
        }
    }

    fn parameter_source_candidates_with_body_hints(
        &self,
        expression: &Expression,
        references: &[FiniteSourceValue],
    ) -> Vec<FiniteSourceValue> {
        let existing = self.function_source_value_candidates(expression);
        if super::binary_chain_longer_than(expression, super::MAX_SOURCE_CANDIDATE_CHAIN) {
            return existing;
        }
        let hints = match Self::unwrap_parenthesized_expr(expression) {
            Expression::Binary(binary)
                if binary.op() == BinaryOp::Arithmetic(ArithmeticOp::Add) =>
            {
                let left =
                    self.parameter_source_candidates_with_body_hints(binary.lhs(), references);
                let right =
                    self.parameter_source_candidates_with_body_hints(binary.rhs(), references);
                bounded_candidates(left.iter().flat_map(|left| {
                    right.iter().filter_map(move |right| {
                        if left.text()?.len().saturating_add(right.text()?.len())
                            > MAX_BODY_HINT_BYTES
                        {
                            return None;
                        }
                        left.concatenate_text(right)
                    })
                }))
            }
            expression if self.parameter_conversion_needs_body_hints(expression) => {
                references.to_vec()
            }
            _ => Vec::new(),
        };
        bounded_candidates(existing.into_iter().chain(hints))
    }

    pub(super) fn function_source_argument_candidates_with_body_hints(
        &self,
        kind: DynamicFunctionKind,
        arguments: &[&Expression],
    ) -> Vec<Vec<String>> {
        let existing = self.function_source_argument_candidates(arguments);
        let Some((body, parameters)) = arguments.split_last() else {
            return existing;
        };
        if existing.len() == MAX_SOURCE_CANDIDATES
            || !parameters
                .iter()
                .any(|argument| self.parameter_conversion_needs_body_hints(argument))
        {
            return existing;
        }
        let body_candidates = self.function_source_value_candidates(body);
        let parser_kind = match kind {
            DynamicFunctionKind::Ordinary => scriptw_front::FunctionParseKind::Ordinary,
            DynamicFunctionKind::Generator => scriptw_front::FunctionParseKind::Generator,
            DynamicFunctionKind::Async => scriptw_front::FunctionParseKind::Async,
            DynamicFunctionKind::AsyncGenerator => scriptw_front::FunctionParseKind::AsyncGenerator,
        };
        let mut bytes = 0usize;
        let mut names = BTreeSet::new();
        let mut eligible_bodies = Vec::new();
        for candidate in &body_candidates {
            let Some(body) = candidate.text() else {
                continue;
            };
            bytes = bytes.saturating_add(body.len());
            if bytes > MAX_BODY_HINT_BYTES {
                return existing;
            }
            // A speculative parse must not record eager host module requests.
            // The ordinary prepared-source cache records those during admission.
            let Ok(parsed) = scriptw_front::prepare_dynamic_function_utf16(
                parser_kind,
                &[decode_js_string_utf16(body)],
            ) else {
                continue;
            };
            let references = parsed.with_compiler_session(|script, interner| {
                let mut visitor = BodyReferences {
                    interner,
                    names: BTreeSet::new(),
                };
                script
                    .visit_with(&mut visitor)
                    .map_continue(|()| visitor.names)
            });
            let references = match references {
                ControlFlow::Continue(references) => references,
                ControlFlow::Break(BodyHintRefusal::PossibleNestedSourceOrHostRequest) => continue,
                ControlFlow::Break(BodyHintRefusal::ReferenceCapacity) => return existing,
            };
            names.extend(references);
            if names.len() > MAX_SOURCE_CANDIDATES {
                return existing;
            }
            eligible_bodies.push(candidate.clone());
        }
        if names.is_empty() {
            return existing;
        }
        let references = names
            .into_iter()
            .map(FiniteSourceValue::SupplementalText)
            .collect::<Vec<_>>();
        let mut slots = parameters
            .iter()
            .map(|argument| self.parameter_source_candidates_with_body_hints(argument, &references))
            .collect::<Vec<_>>();
        slots.push(eligible_bodies);
        let expanded = Self::source_candidate_tuples(&slots, true);
        let mut tuples = existing.iter().cloned().collect::<BTreeSet<_>>();
        tuples.extend(expanded);
        if tuples.len() > MAX_SOURCE_CANDIDATES {
            existing
        } else {
            tuples.into_iter().collect()
        }
    }
}
