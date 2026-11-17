//! Optional cross-unit source hints resolve callable sites only in their own
//! retained parse. Runtime wrappers, calls and environments remain unchanged.

use super::finite_function_source::{bounded_candidates, MAX_SOURCE_CANDIDATES};
use super::*;

impl ScriptLowerer<'_> {
    pub(super) fn finite_returned_callable_source_candidates(
        &self,
        call: &boa_ast::expression::Call,
        callees: &[FiniteSourceValue],
    ) -> Vec<FiniteSourceValue> {
        if !callees.iter().any(|callee| {
            matches!(callee, FiniteSourceValue::Function(id)
                if StandardBuiltinId::from_function_id(id)
                    == Some(StandardBuiltinId::ShadowRealmPrototypeEvaluate))
        }) {
            return Vec::new();
        }
        let Some(source) = call.args().first() else {
            return Vec::new();
        };
        if matches!(source, Expression::Spread(_)) {
            return Vec::new();
        }
        bounded_candidates(
            self.function_source_value_candidates(source)
                .into_iter()
                .filter_map(|candidate| {
                    self.analysis
                        .allocations
                        .prepared_sources
                        .returned_shadow_callable_source(candidate.text()?)
                        .map(FiniteSourceValue::ReturnedCallableSource)
                }),
        )
    }

    pub(super) fn register_returned_callable_argument_candidates(
        &self,
        callees: &[FiniteSourceValue],
        arguments: &[Expression],
    ) {
        let sources = callees
            .iter()
            .filter_map(|callee| match callee {
                FiniteSourceValue::ReturnedCallableSource(source) => Some(*source),
                _ => None,
            })
            .collect::<Vec<_>>();
        if sources.is_empty() {
            return;
        }
        let arguments = arguments
            .iter()
            .take_while(|argument| !matches!(argument, Expression::Spread(_)))
            .map(|argument| {
                self.function_source_value_candidates(argument)
                    .into_iter()
                    .filter_map(|value| value.text().map(str::to_owned))
                    .take(MAX_SOURCE_CANDIDATES)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for source in sources {
            self.analysis
                .allocations
                .prepared_sources
                .observe_returned_callable_arguments(source, &arguments);
        }
    }

    pub(super) fn seed_returned_callable_parameter_candidates(&mut self, parsed: &ParsedScript) {
        if !matches!(
            self.analysis.script_instantiation,
            ScriptInstantiation::Prepared(PreparedScriptKind::ShadowRealmEvaluate)
        ) {
            return;
        }
        let requests = self
            .analysis
            .allocations
            .prepared_sources
            .returned_callable_parameter_hints(parsed);
        let mut observations = BTreeMap::new();
        for (key, index, texts) in requests {
            // The child's own completed analysis is the sole FunctionId owner.
            let Some(function_id) = self.analysis.function_expr_ids.get(&key) else {
                continue;
            };
            observations.insert(
                (function_id.clone(), index),
                texts
                    .into_iter()
                    .map(FiniteSourceValue::SupplementalText)
                    .collect::<Vec<_>>(),
            );
        }
        self.merge_function_source_parameter_candidates(observations);
    }
}
